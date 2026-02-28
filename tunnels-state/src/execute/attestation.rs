//! Attestation transaction handlers.
//!
//! Handles SubmitAttestation and FinalizeAttestation.

use tunnels_core::crypto::sha256;
use tunnels_core::serialization::serialize;
use tunnels_core::{Attestation, AttestationStatus, Denomination, SignedTransaction, U256};

use crate::accumulator::finalize_attestation_units;
use crate::bond::{lock_attestation_bond, return_attestation_bond};
use crate::error::{StateError, StateResult};
use crate::state::StateWriter;

use super::context::ExecutionContext;

/// Execute a SubmitAttestation transaction.
///
/// # Validation
/// - Project must exist
/// - Signer is the contributor (existing active identity)
/// - Units > 0
/// - Bond poster must exist and be active
/// - Bond amount >= project.attestation_bond
pub fn execute_submit_attestation<S: StateWriter>(
    state: &mut S,
    ctx: &ExecutionContext,
    signer_address: &[u8; 20],
    signed_tx: &SignedTransaction,
    project_id: &[u8; 20],
    units: u64,
    evidence_hash: &[u8; 32],
    bond_poster: &[u8; 20],
    bond_amount: U256,
    bond_denomination: &Denomination,
) -> StateResult<[u8; 20]> {
    // Verify project exists
    let project = state
        .get_project(project_id)
        .ok_or(StateError::ProjectNotFound {
            project_id: *project_id,
        })?
        .clone();

    // Verify signer (contributor) exists and is active
    let contributor = state
        .get_identity(signer_address)
        .ok_or(StateError::IdentityNotFound {
            address: *signer_address,
        })?;
    if !contributor.active {
        return Err(StateError::IdentityDeactivated {
            address: *signer_address,
        });
    }

    // Verify units > 0
    if units == 0 {
        return Err(StateError::ZeroUnits);
    }

    // Verify bond poster exists and is active
    let poster = state
        .get_identity(bond_poster)
        .ok_or(StateError::BondPosterNotFound {
            bond_poster: *bond_poster,
        })?;
    if !poster.active {
        return Err(StateError::BondPosterDeactivated {
            bond_poster: *bond_poster,
        });
    }

    // Verify bond amount >= project.attestation_bond
    let project_min_bond = U256::from(project.attestation_bond);
    if bond_amount < project_min_bond {
        return Err(StateError::BondBelowMinimum {
            provided: bond_amount,
            required: project_min_bond,
        });
    }

    // Derive attestation_id from hash of the signed transaction
    let tx_bytes = serialize(signed_tx).expect("SignedTransaction serialization should not fail");
    let hash = sha256(&tx_bytes);
    let mut attestation_id = [0u8; 20];
    attestation_id.copy_from_slice(&hash[..20]);

    // Check attestation doesn't already exist
    if state.attestation_exists(&attestation_id) {
        return Err(StateError::AttestationAlreadyExists { attestation_id });
    }

    // Lock the bond
    lock_attestation_bond(
        state,
        &attestation_id,
        bond_poster,
        bond_amount,
        bond_denomination,
        project_min_bond, // minimum required
    )?;

    // Create attestation
    let attestation = Attestation {
        attestation_id,
        project_id: *project_id,
        contributor: *signer_address,
        units,
        evidence_hash: *evidence_hash,
        bond_poster: *bond_poster,
        bond_amount,
        bond_denomination: *bond_denomination,
        status: AttestationStatus::Pending,
        created_at: ctx.timestamp,
        finalized_at: None,
    };

    state.insert_attestation(attestation);

    Ok(attestation_id)
}

/// Execute a FinalizeAttestation transaction.
///
/// This is a permissionless operation - anyone can call it once the
/// verification window has expired.
///
/// # Validation
/// - Attestation must exist with status Pending
/// - Verification window must have expired
pub fn execute_finalize_attestation<S: StateWriter>(
    state: &mut S,
    ctx: &ExecutionContext,
    attestation_id: &[u8; 20],
) -> StateResult<u64> {
    // Get attestation
    let attestation = state
        .get_attestation(attestation_id)
        .ok_or(StateError::AttestationNotFound {
            attestation_id: *attestation_id,
        })?
        .clone();

    // Verify status is Pending
    if attestation.status != AttestationStatus::Pending {
        return Err(StateError::AttestationNotPending {
            attestation_id: *attestation_id,
            status: attestation.status,
        });
    }

    // Get project for verification window
    let project = state
        .get_project(&attestation.project_id)
        .ok_or(StateError::ProjectNotFound {
            project_id: attestation.project_id,
        })?;

    // Verify verification window has expired
    let expires_at = attestation.created_at + project.verification_window;
    if ctx.timestamp < expires_at {
        return Err(StateError::VerificationWindowNotExpired {
            attestation_id: *attestation_id,
            expires_at,
            current: ctx.timestamp,
        });
    }

    // Return full bond (no burn)
    return_attestation_bond(
        state,
        attestation_id,
        &attestation.bond_poster,
        &attestation.bond_denomination,
    )?;

    // Finalize the attestation and update accumulator
    finalize_attestation_units(
        state,
        attestation_id,
        &attestation.project_id,
        &attestation.contributor,
        attestation.units,
        &attestation.bond_denomination,
        ctx.timestamp,
    )?;

    Ok(attestation.units)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execute::identity::execute_create_identity;
    use crate::execute::project::{execute_register_project, MIN_VERIFICATION_WINDOW};
    use crate::state::{ProtocolState, StateReader};
    use tunnels_core::crypto::sign;
    use tunnels_core::serialization::serialize;
    use tunnels_core::transaction::mine_pow;
    use tunnels_core::{KeyPair, RecipientType, Transaction};

    fn test_context() -> ExecutionContext {
        ExecutionContext::test_context()
    }

    fn test_denom() -> Denomination {
        *b"USD\0\0\0\0\0"
    }

    fn create_identity(state: &mut ProtocolState, ctx: &ExecutionContext) -> ([u8; 20], KeyPair) {
        let kp = KeyPair::generate();
        let address = execute_create_identity(state, ctx, &kp.public_key()).unwrap();
        (address, kp)
    }

    fn create_project(
        state: &mut ProtocolState,
        ctx: &ExecutionContext,
        creator_addr: &[u8; 20],
        creator_kp: &KeyPair,
    ) -> [u8; 20] {
        let tx = Transaction::RegisterProject {
            creator: *creator_addr,
            rho: 1000,
            phi: 500,
            gamma: 1500,
            delta: 0,
            phi_recipient: *creator_addr,
            gamma_recipient: *creator_addr,
            gamma_recipient_type: RecipientType::Identity,
            reserve_authority: *creator_addr,
            deposit_authority: *creator_addr,
            adjudicator: *creator_addr,
            predecessor: None,
            verification_window: MIN_VERIFICATION_WINDOW,
            attestation_bond: 100,
            challenge_bond: 50,
            evidence_hash: None,
        };

        let tx_bytes = serialize(&tx).unwrap();
        let signature = sign(creator_kp.signing_key(), &tx_bytes);
        let (pow_nonce, pow_hash) = mine_pow(&tx, &creator_kp.public_key(), &signature, 1);

        let signed_tx = SignedTransaction {
            tx: tx.clone(),
            signer: creator_kp.public_key(),
            signature,
            pow_nonce,
            pow_hash,
        };

        execute_register_project(state, ctx, creator_addr, &signed_tx, &tx).unwrap()
    }

    fn create_submit_attestation_tx(
        kp: &KeyPair,
        project_id: [u8; 20],
        units: u64,
        bond_poster: [u8; 20],
        bond_amount: U256,
    ) -> SignedTransaction {
        let tx = Transaction::SubmitAttestation {
            project_id,
            units,
            evidence_hash: [0xAB; 32],
            bond_poster,
            bond_amount,
            bond_denomination: test_denom(),
        };

        let tx_bytes = serialize(&tx).unwrap();
        let signature = sign(kp.signing_key(), &tx_bytes);
        let (pow_nonce, pow_hash) = mine_pow(&tx, &kp.public_key(), &signature, 1);

        SignedTransaction {
            tx,
            signer: kp.public_key(),
            signature,
            pow_nonce,
            pow_hash,
        }
    }

    #[test]
    fn test_submit_attestation() {
        let mut state = ProtocolState::new();
        let ctx = test_context();

        let (creator_addr, creator_kp) = create_identity(&mut state, &ctx);
        let project_id = create_project(&mut state, &ctx, &creator_addr, &creator_kp);

        let (contributor_addr, contributor_kp) = create_identity(&mut state, &ctx);
        let signed_tx = create_submit_attestation_tx(
            &contributor_kp,
            project_id,
            100,
            contributor_addr,
            U256::from(1000u64),
        );

        let attestation_id = execute_submit_attestation(
            &mut state,
            &ctx,
            &contributor_addr,
            &signed_tx,
            &project_id,
            100,
            &[0xAB; 32],
            &contributor_addr,
            U256::from(1000u64),
            &test_denom(),
        )
        .unwrap();

        let attestation = state.get_attestation(&attestation_id).unwrap();
        assert_eq!(attestation.status, AttestationStatus::Pending);
        assert_eq!(attestation.units, 100);
        assert_eq!(attestation.contributor, contributor_addr);

        // Bond should be locked
        assert_eq!(
            state.get_locked_bond(&attestation_id),
            Some(U256::from(1000u64))
        );
    }

    #[test]
    fn test_submit_attestation_zero_units_fails() {
        let mut state = ProtocolState::new();
        let ctx = test_context();

        let (creator_addr, creator_kp) = create_identity(&mut state, &ctx);
        let project_id = create_project(&mut state, &ctx, &creator_addr, &creator_kp);

        let (contributor_addr, contributor_kp) = create_identity(&mut state, &ctx);
        let signed_tx = create_submit_attestation_tx(
            &contributor_kp,
            project_id,
            0, // Zero units
            contributor_addr,
            U256::from(1000u64),
        );

        let result = execute_submit_attestation(
            &mut state,
            &ctx,
            &contributor_addr,
            &signed_tx,
            &project_id,
            0,
            &[0xAB; 32],
            &contributor_addr,
            U256::from(1000u64),
            &test_denom(),
        );

        assert!(matches!(result, Err(StateError::ZeroUnits)));
    }

    #[test]
    fn test_submit_attestation_bond_below_minimum_fails() {
        let mut state = ProtocolState::new();
        let ctx = test_context();

        let (creator_addr, creator_kp) = create_identity(&mut state, &ctx);
        let project_id = create_project(&mut state, &ctx, &creator_addr, &creator_kp);

        let (contributor_addr, contributor_kp) = create_identity(&mut state, &ctx);
        // Project attestation_bond is 100, submitting with 50 should fail
        let signed_tx = create_submit_attestation_tx(
            &contributor_kp,
            project_id,
            100,
            contributor_addr,
            U256::from(50u64), // Below project minimum (100)
        );

        let result = execute_submit_attestation(
            &mut state,
            &ctx,
            &contributor_addr,
            &signed_tx,
            &project_id,
            100,
            &[0xAB; 32],
            &contributor_addr,
            U256::from(50u64),
            &test_denom(),
        );

        assert!(matches!(result, Err(StateError::BondBelowMinimum { .. })));
    }

    #[test]
    fn test_finalize_attestation() {
        let mut state = ProtocolState::new();
        let ctx = test_context();

        let (creator_addr, creator_kp) = create_identity(&mut state, &ctx);
        let project_id = create_project(&mut state, &ctx, &creator_addr, &creator_kp);

        let (contributor_addr, contributor_kp) = create_identity(&mut state, &ctx);
        let signed_tx = create_submit_attestation_tx(
            &contributor_kp,
            project_id,
            100,
            contributor_addr,
            U256::from(1000u64),
        );

        let attestation_id = execute_submit_attestation(
            &mut state,
            &ctx,
            &contributor_addr,
            &signed_tx,
            &project_id,
            100,
            &[0xAB; 32],
            &contributor_addr,
            U256::from(1000u64),
            &test_denom(),
        )
        .unwrap();

        // Advance time past verification window
        let finalize_ctx = ExecutionContext::with_timestamp(
            ctx.timestamp + MIN_VERIFICATION_WINDOW + 1,
        );

        let units = execute_finalize_attestation(&mut state, &finalize_ctx, &attestation_id).unwrap();
        assert_eq!(units, 100);

        let attestation = state.get_attestation(&attestation_id).unwrap();
        assert_eq!(attestation.status, AttestationStatus::Finalized);

        // Bond should be unlocked and credited (full return)
        assert!(state.get_locked_bond(&attestation_id).is_none());
        assert_eq!(
            state.get_claimable(&contributor_addr, &test_denom()),
            U256::from(1000u64)
        );
    }

    #[test]
    fn test_finalize_attestation_window_not_expired_fails() {
        let mut state = ProtocolState::new();
        let ctx = test_context();

        let (creator_addr, creator_kp) = create_identity(&mut state, &ctx);
        let project_id = create_project(&mut state, &ctx, &creator_addr, &creator_kp);

        let (contributor_addr, contributor_kp) = create_identity(&mut state, &ctx);
        let signed_tx = create_submit_attestation_tx(
            &contributor_kp,
            project_id,
            100,
            contributor_addr,
            U256::from(1000u64),
        );

        let attestation_id = execute_submit_attestation(
            &mut state,
            &ctx,
            &contributor_addr,
            &signed_tx,
            &project_id,
            100,
            &[0xAB; 32],
            &contributor_addr,
            U256::from(1000u64),
            &test_denom(),
        )
        .unwrap();

        // Try to finalize before window expires
        let result = execute_finalize_attestation(&mut state, &ctx, &attestation_id);

        assert!(matches!(
            result,
            Err(StateError::VerificationWindowNotExpired { .. })
        ));
    }
}
