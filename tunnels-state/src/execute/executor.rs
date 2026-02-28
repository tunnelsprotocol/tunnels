//! Transaction executor - main entry point for state transitions.
//!
//! This module contains the `apply_transaction` function which validates
//! and executes signed transactions, updating the protocol state.

use tunnels_core::crypto::{derive_address, verify};
use tunnels_core::serialization::serialize;
use tunnels_core::{SignedTransaction, Transaction};

use crate::error::{StateError, StateResult};
use crate::state::StateWriter;

use super::attestation::{execute_finalize_attestation, execute_submit_attestation};
use super::challenge::{
    execute_challenge_attestation, execute_resolve_challenge, execute_respond_to_challenge,
};
use super::context::ExecutionContext;
use super::identity::{
    execute_create_agent, execute_create_identity, execute_deactivate_agent,
    execute_update_profile_hash,
};
use super::project::execute_register_project;
use super::revenue::{execute_claim_reserve, execute_claim_revenue, execute_deposit_revenue};

/// Apply a signed transaction to the protocol state.
///
/// This is the main entry point for state transitions. It:
/// 1. Verifies the signature
/// 2. Verifies proof-of-work (TODO: implement PoW validation)
/// 3. Dispatches to the appropriate transaction handler
/// 4. Returns the result
///
/// # Arguments
/// - `state`: Mutable protocol state
/// - `signed_tx`: The signed transaction to apply
/// - `block_timestamp`: Current block timestamp
///
/// # Returns
/// - `Ok(())` if the transaction was applied successfully
/// - `Err(StateError)` if validation or execution failed
pub fn apply_transaction<S: StateWriter>(
    state: &mut S,
    signed_tx: &SignedTransaction,
    block_timestamp: u64,
) -> StateResult<()> {
    let ctx = ExecutionContext::with_timestamp(block_timestamp);
    apply_transaction_with_context(state, signed_tx, &ctx)
}

/// Apply a signed transaction with a custom execution context.
///
/// This variant allows specifying devnet-specific parameters like
/// shorter verification windows.
///
/// # Arguments
/// - `state`: Mutable protocol state
/// - `signed_tx`: The signed transaction to apply
/// - `ctx`: Execution context with validation parameters
///
/// # Returns
/// - `Ok(())` if the transaction was applied successfully
/// - `Err(StateError)` if validation or execution failed
pub fn apply_transaction_with_context<S: StateWriter>(
    state: &mut S,
    signed_tx: &SignedTransaction,
    ctx: &ExecutionContext,
) -> StateResult<()> {
    // Verify signature
    let tx_bytes = serialize(&signed_tx.tx)
        .map_err(|_| StateError::InvalidSignature)?;

    verify(&signed_tx.signer, &tx_bytes, &signed_tx.signature)
        .map_err(|_| StateError::InvalidSignature)?;

    // Derive signer address
    let signer_address = derive_address(&signed_tx.signer);

    // TODO: Verify proof-of-work
    // let required_difficulty = state.get_tx_difficulty_state().current_difficulty;
    // if !verify_pow(signed_tx, required_difficulty) {
    //     return Err(StateError::InvalidProofOfWork);
    // }

    // Dispatch to appropriate handler
    match &signed_tx.tx {
        Transaction::CreateIdentity { .. } => {
            execute_create_identity(state, ctx, &signed_tx.signer)?;
        }

        Transaction::CreateAgent { parent, public_key } => {
            execute_create_agent(state, ctx, &signer_address, parent, public_key)?;
        }

        Transaction::UpdateProfileHash {
            identity,
            hash,
        } => {
            execute_update_profile_hash(state, ctx, &signer_address, identity, hash)?;
        }

        Transaction::DeactivateAgent { agent } => {
            execute_deactivate_agent(state, ctx, &signer_address, agent)?;
        }

        Transaction::RegisterProject { .. } => {
            execute_register_project(state, ctx, &signer_address, signed_tx, &signed_tx.tx)?;
        }

        Transaction::SubmitAttestation {
            project_id,
            units,
            evidence_hash,
            bond_poster,
            bond_amount,
            bond_denomination,
        } => {
            execute_submit_attestation(
                state,
                ctx,
                &signer_address,
                signed_tx,
                project_id,
                *units,
                evidence_hash,
                bond_poster,
                *bond_amount,
                bond_denomination,
            )?;
        }

        Transaction::FinalizeAttestation { attestation_id } => {
            execute_finalize_attestation(state, ctx, attestation_id)?;
        }

        Transaction::ChallengeAttestation {
            attestation_id,
            evidence_hash,
            bond_poster,
            bond_amount,
        } => {
            execute_challenge_attestation(
                state,
                ctx,
                &signer_address,
                signed_tx,
                attestation_id,
                evidence_hash,
                *bond_poster,
                *bond_amount,
            )?;
        }

        Transaction::RespondToChallenge {
            challenge_id,
            evidence_hash,
        } => {
            execute_respond_to_challenge(
                state,
                ctx,
                &signer_address,
                challenge_id,
                evidence_hash,
            )?;
        }

        Transaction::ResolveChallenge {
            challenge_id,
            ruling,
        } => {
            execute_resolve_challenge(state, ctx, &signer_address, challenge_id, ruling)?;
        }

        Transaction::DepositRevenue {
            project_id,
            amount,
            denomination,
        } => {
            execute_deposit_revenue(state, ctx, &signer_address, project_id, *amount, denomination)?;
        }

        Transaction::ClaimRevenue {
            project_id,
            denomination,
        } => {
            // Signer is the contributor claiming their revenue
            execute_claim_revenue(state, ctx, &signer_address, project_id, &signer_address, denomination)?;
        }

        Transaction::ClaimReserve {
            project_id,
            denomination,
            amount,
        } => {
            // Reserve authority can claim to themselves
            execute_claim_reserve(
                state,
                ctx,
                &signer_address,
                project_id,
                &signer_address,
                *amount,
                denomination,
            )?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{ProtocolState, StateReader};
    use tunnels_core::crypto::{derive_address, sha256, sign};
    use tunnels_core::serialization::serialize;
    use tunnels_core::transaction::mine_pow;
    use tunnels_core::{KeyPair, U256};

    fn create_signed_identity_tx(kp: &KeyPair) -> SignedTransaction {
        let tx = Transaction::CreateIdentity {
            public_key: kp.public_key(),
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
    fn test_apply_create_identity() {
        let mut state = ProtocolState::new();
        let kp = KeyPair::generate();
        let signed_tx = create_signed_identity_tx(&kp);

        apply_transaction(&mut state, &signed_tx, 1000).unwrap();

        let address = derive_address(&kp.public_key());
        assert!(state.identity_exists(&address));
    }

    #[test]
    fn test_apply_invalid_signature() {
        let mut state = ProtocolState::new();
        let kp = KeyPair::generate();
        let other_kp = KeyPair::generate();

        let tx = Transaction::CreateIdentity {
            public_key: kp.public_key(),
        };

        let tx_bytes = serialize(&tx).unwrap();
        // Sign with wrong key
        let signature = sign(other_kp.signing_key(), &tx_bytes);
        let (pow_nonce, pow_hash) = mine_pow(&tx, &kp.public_key(), &signature, 1);

        let signed_tx = SignedTransaction {
            tx,
            signer: kp.public_key(),
            signature,
            pow_nonce,
            pow_hash,
        };

        let result = apply_transaction(&mut state, &signed_tx, 1000);
        assert!(matches!(result, Err(StateError::InvalidSignature)));
    }

    #[test]
    fn test_apply_create_identity_already_exists() {
        let mut state = ProtocolState::new();
        let kp = KeyPair::generate();
        let signed_tx = create_signed_identity_tx(&kp);

        // First time succeeds
        apply_transaction(&mut state, &signed_tx, 1000).unwrap();

        // Second time fails
        let result = apply_transaction(&mut state, &signed_tx, 1001);
        assert!(matches!(
            result,
            Err(StateError::IdentityAlreadyExists { .. })
        ));
    }

    #[test]
    fn test_full_attestation_lifecycle() {
        use tunnels_core::RecipientType;

        let mut state = ProtocolState::new();

        // Create identity
        let kp = KeyPair::generate();
        let signed_tx = create_signed_identity_tx(&kp);
        apply_transaction(&mut state, &signed_tx, 1000).unwrap();
        let address = derive_address(&kp.public_key());

        // Register project
        let project_tx = Transaction::RegisterProject {
            creator: address,
            rho: 1000,
            phi: 500,
            gamma: 1500,
            delta: 0,
            phi_recipient: address,
            gamma_recipient: address,
            gamma_recipient_type: RecipientType::Identity,
            reserve_authority: address,
            deposit_authority: address,
            adjudicator: address,
            predecessor: None,
            verification_window: 86400,
            attestation_bond: 100,
            challenge_bond: 0,
            evidence_hash: None,
        };

        let tx_bytes = serialize(&project_tx).unwrap();
        let signature = sign(kp.signing_key(), &tx_bytes);
        let (pow_nonce, pow_hash) = mine_pow(&project_tx, &kp.public_key(), &signature, 1);

        let signed_project_tx = SignedTransaction {
            tx: project_tx.clone(),
            signer: kp.public_key(),
            signature,
            pow_nonce,
            pow_hash,
        };

        apply_transaction(&mut state, &signed_project_tx, 1001).unwrap();

        // Get project ID
        let project_hash = sha256(&serialize(&signed_project_tx).unwrap());
        let mut project_id = [0u8; 20];
        project_id.copy_from_slice(&project_hash[..20]);

        // Submit attestation
        let denom = *b"USD\0\0\0\0\0";
        let attest_tx = Transaction::SubmitAttestation {
            project_id,
            units: 100,
            evidence_hash: [0xAB; 32],
            bond_poster: address,
            bond_amount: U256::from(1000u64),
            bond_denomination: denom,
        };

        let tx_bytes = serialize(&attest_tx).unwrap();
        let signature = sign(kp.signing_key(), &tx_bytes);
        let (pow_nonce, pow_hash) = mine_pow(&attest_tx, &kp.public_key(), &signature, 1);

        let signed_attest_tx = SignedTransaction {
            tx: attest_tx,
            signer: kp.public_key(),
            signature,
            pow_nonce,
            pow_hash,
        };

        apply_transaction(&mut state, &signed_attest_tx, 1002).unwrap();

        // Get attestation ID
        let attest_hash = sha256(&serialize(&signed_attest_tx).unwrap());
        let mut attestation_id = [0u8; 20];
        attestation_id.copy_from_slice(&attest_hash[..20]);

        // Verify attestation exists and is pending
        let attestation = state.get_attestation(&attestation_id).unwrap();
        assert_eq!(attestation.status, tunnels_core::AttestationStatus::Pending);

        // Finalize attestation (after verification window)
        let finalize_tx = Transaction::FinalizeAttestation { attestation_id };

        let tx_bytes = serialize(&finalize_tx).unwrap();
        let signature = sign(kp.signing_key(), &tx_bytes);
        let (pow_nonce, pow_hash) = mine_pow(&finalize_tx, &kp.public_key(), &signature, 1);

        let signed_finalize_tx = SignedTransaction {
            tx: finalize_tx,
            signer: kp.public_key(),
            signature,
            pow_nonce,
            pow_hash,
        };

        // Finalize after window expires
        apply_transaction(&mut state, &signed_finalize_tx, 1002 + 604801).unwrap();

        // Verify attestation is finalized
        let attestation = state.get_attestation(&attestation_id).unwrap();
        assert_eq!(attestation.status, tunnels_core::AttestationStatus::Finalized);
    }
}
