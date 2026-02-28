//! Challenge transaction handlers.
//!
//! Handles ChallengeAttestation, RespondToChallenge, and ResolveChallenge.

use tunnels_core::crypto::sha256;
use tunnels_core::serialization::serialize;
use tunnels_core::{
    AttestationStatus, Challenge, ChallengeRuling, ChallengeStatus, SignedTransaction, U256,
};

use crate::accumulator::finalize_attestation_units;
use crate::bond::{
    forfeit_attestation_bond_to_challenger, forfeit_challenge_bond_to_attestor,
    lock_challenge_bond, return_attestation_bond, return_challenge_bond,
};
use crate::error::{StateError, StateResult};
use crate::state::StateWriter;

use super::context::ExecutionContext;

/// Execute a ChallengeAttestation transaction.
///
/// # Validation
/// - Attestation must exist with status Pending
/// - Must be within verification window
/// - Challenger must be a different identity than contributor
/// - If project.challenge_bond > 0, bond_poster and bond_amount required
pub fn execute_challenge_attestation<S: StateWriter>(
    state: &mut S,
    ctx: &ExecutionContext,
    signer_address: &[u8; 20],
    signed_tx: &SignedTransaction,
    attestation_id: &[u8; 20],
    evidence_hash: &[u8; 32],
    bond_poster: Option<[u8; 20]>,
    bond_amount: U256,
) -> StateResult<[u8; 20]> {
    // Get attestation
    let attestation = state
        .get_attestation(attestation_id)
        .ok_or(StateError::AttestationNotFound {
            attestation_id: *attestation_id,
        })?
        .clone();

    // Verify attestation is Pending
    if attestation.status != AttestationStatus::Pending {
        return Err(StateError::AttestationNotChallengeable {
            attestation_id: *attestation_id,
        });
    }

    // Get project
    let project = state
        .get_project(&attestation.project_id)
        .ok_or(StateError::ProjectNotFound {
            project_id: attestation.project_id,
        })?
        .clone();

    // Verify within verification window
    let expires_at = attestation.created_at + project.verification_window;
    if ctx.timestamp >= expires_at {
        return Err(StateError::VerificationWindowExpired {
            attestation_id: *attestation_id,
            expired_at: expires_at,
            current: ctx.timestamp,
        });
    }

    // Verify signer exists and is active
    let challenger = state
        .get_identity(signer_address)
        .ok_or(StateError::IdentityNotFound {
            address: *signer_address,
        })?;
    if !challenger.active {
        return Err(StateError::IdentityDeactivated {
            address: *signer_address,
        });
    }

    // Verify challenger is not the contributor
    if signer_address == &attestation.contributor {
        return Err(StateError::ChallengerIsContributor {
            identity: *signer_address,
        });
    }

    // Check if a challenge already exists for this attestation
    if state.get_challenge_for_attestation(attestation_id).is_some() {
        return Err(StateError::ChallengeAlreadyExists {
            attestation_id: *attestation_id,
        });
    }

    // Derive challenge_id from hash of the signed transaction
    let tx_bytes = serialize(signed_tx).expect("SignedTransaction serialization should not fail");
    let hash = sha256(&tx_bytes);
    let mut challenge_id = [0u8; 20];
    challenge_id.copy_from_slice(&hash[..20]);

    // Handle challenge bond
    let (final_bond_poster, final_bond_amount) = if project.challenge_bond > 0 {
        // Bond is required
        let poster = bond_poster.ok_or(StateError::ChallengeBondPosterRequired)?;

        // Verify bond poster exists and is active
        let poster_identity = state
            .get_identity(&poster)
            .ok_or(StateError::BondPosterNotFound { bond_poster: poster })?;
        if !poster_identity.active {
            return Err(StateError::BondPosterDeactivated { bond_poster: poster });
        }

        // Lock the challenge bond
        lock_challenge_bond(
            state,
            &challenge_id,
            &poster,
            bond_amount,
            &attestation.bond_denomination,
            project.challenge_bond,
        )?;

        (Some(poster), bond_amount)
    } else {
        // No bond required
        (None, U256::zero())
    };

    // Create challenge
    let challenge = Challenge {
        challenge_id,
        attestation_id: *attestation_id,
        challenger: *signer_address,
        evidence_hash: *evidence_hash,
        response_hash: None,
        bond_poster: final_bond_poster,
        bond_amount: final_bond_amount,
        status: ChallengeStatus::Open,
        created_at: ctx.timestamp,
    };

    state.insert_challenge(challenge);

    // Update attestation status to Challenged
    state.update_attestation(attestation_id, |att| {
        att.status = AttestationStatus::Challenged;
    });

    Ok(challenge_id)
}

/// Execute a RespondToChallenge transaction.
///
/// # Validation
/// - Challenge must exist with status Open
/// - Signer must be the original contributor
pub fn execute_respond_to_challenge<S: StateWriter>(
    state: &mut S,
    _ctx: &ExecutionContext,
    signer_address: &[u8; 20],
    challenge_id: &[u8; 20],
    evidence_hash: &[u8; 32],
) -> StateResult<()> {
    // Get challenge
    let challenge = state
        .get_challenge(challenge_id)
        .ok_or(StateError::ChallengeNotFound {
            challenge_id: *challenge_id,
        })?
        .clone();

    // Verify challenge is Open
    if challenge.status != ChallengeStatus::Open {
        return Err(StateError::ChallengeCannotRespond {
            challenge_id: *challenge_id,
            status: challenge.status,
        });
    }

    // Get attestation to verify signer is the contributor
    let attestation = state
        .get_attestation(&challenge.attestation_id)
        .ok_or(StateError::AttestationNotFound {
            attestation_id: challenge.attestation_id,
        })?;

    // Verify signer is the contributor
    if signer_address != &attestation.contributor {
        return Err(StateError::UnauthorizedSigner {
            expected: attestation.contributor,
            actual: *signer_address,
        });
    }

    // Update challenge with response
    state.update_challenge(challenge_id, |ch| {
        ch.response_hash = Some(*evidence_hash);
        ch.status = ChallengeStatus::Responded;
    });

    Ok(())
}

/// Execute a ResolveChallenge transaction.
///
/// # Validation
/// - Challenge must exist with status Open or Responded
/// - Signer must be the project's adjudicator
pub fn execute_resolve_challenge<S: StateWriter>(
    state: &mut S,
    ctx: &ExecutionContext,
    signer_address: &[u8; 20],
    challenge_id: &[u8; 20],
    ruling: &ChallengeRuling,
) -> StateResult<()> {
    // Get challenge
    let challenge = state
        .get_challenge(challenge_id)
        .ok_or(StateError::ChallengeNotFound {
            challenge_id: *challenge_id,
        })?
        .clone();

    // Verify challenge is not already resolved
    if challenge.status == ChallengeStatus::ResolvedValid
        || challenge.status == ChallengeStatus::ResolvedInvalid
    {
        return Err(StateError::ChallengeAlreadyResolved {
            challenge_id: *challenge_id,
            status: challenge.status,
        });
    }

    // Get attestation
    let attestation = state
        .get_attestation(&challenge.attestation_id)
        .ok_or(StateError::AttestationNotFound {
            attestation_id: challenge.attestation_id,
        })?
        .clone();

    // Get project to verify adjudicator
    let project = state
        .get_project(&attestation.project_id)
        .ok_or(StateError::ProjectNotFound {
            project_id: attestation.project_id,
        })?;

    // Verify signer is the adjudicator
    if signer_address != &project.adjudicator {
        return Err(StateError::NotAdjudicator {
            adjudicator: project.adjudicator,
            signer: *signer_address,
        });
    }

    match ruling {
        ChallengeRuling::Valid => {
            // Attestation is valid - attestation stands
            // - Return full attestation bond
            // - Forfeit challenge bond to attestation bond poster
            // - Finalize attestation

            return_attestation_bond(
                state,
                &challenge.attestation_id,
                &attestation.bond_poster,
                &attestation.bond_denomination,
            )?;

            if challenge.bond_poster.is_some() {
                forfeit_challenge_bond_to_attestor(
                    state,
                    challenge_id,
                    &attestation.bond_poster,
                    &attestation.bond_denomination,
                )?;
            }

            // Finalize attestation
            finalize_attestation_units(
                state,
                &challenge.attestation_id,
                &attestation.project_id,
                &attestation.contributor,
                attestation.units,
                &attestation.bond_denomination,
                ctx.timestamp,
            )?;

            // Update challenge status
            state.update_challenge(challenge_id, |ch| {
                ch.status = ChallengeStatus::ResolvedValid;
            });
        }
        ChallengeRuling::Invalid => {
            // Attestation is invalid - challenge succeeds
            // - Forfeit attestation bond to challenger
            // - Return challenge bond (if any)
            // - Reject attestation

            forfeit_attestation_bond_to_challenger(
                state,
                &challenge.attestation_id,
                &challenge.challenger,
                &attestation.bond_denomination,
            )?;

            if challenge.bond_poster.is_some() {
                return_challenge_bond(
                    state,
                    challenge_id,
                    &challenge.challenger,
                    &attestation.bond_denomination,
                )?;
            }

            // Update attestation to Rejected
            state.update_attestation(&challenge.attestation_id, |att| {
                att.status = AttestationStatus::Rejected;
            });

            // Update challenge status
            state.update_challenge(challenge_id, |ch| {
                ch.status = ChallengeStatus::ResolvedInvalid;
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execute::attestation::execute_submit_attestation;
    use crate::execute::identity::execute_create_identity;
    use crate::execute::project::{execute_register_project, MIN_VERIFICATION_WINDOW};
    use crate::state::{ProtocolState, StateReader};
    use tunnels_core::crypto::sign;
    use tunnels_core::serialization::serialize;
    use tunnels_core::transaction::mine_pow;
    use tunnels_core::{Denomination, KeyPair, RecipientType, Transaction};

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

    fn create_project_with_challenge_bond(
        state: &mut ProtocolState,
        ctx: &ExecutionContext,
        creator_addr: &[u8; 20],
        creator_kp: &KeyPair,
        challenge_bond: u64,
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
            challenge_bond,
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

    fn submit_attestation(
        state: &mut ProtocolState,
        ctx: &ExecutionContext,
        project_id: &[u8; 20],
        contributor_addr: &[u8; 20],
        contributor_kp: &KeyPair,
        units: u64,
    ) -> [u8; 20] {
        let tx = Transaction::SubmitAttestation {
            project_id: *project_id,
            units,
            evidence_hash: [0xAB; 32],
            bond_poster: *contributor_addr,
            bond_amount: U256::from(1000u64),
            bond_denomination: test_denom(),
        };

        let tx_bytes = serialize(&tx).unwrap();
        let signature = sign(contributor_kp.signing_key(), &tx_bytes);
        let (pow_nonce, pow_hash) = mine_pow(&tx, &contributor_kp.public_key(), &signature, 1);

        let signed_tx = SignedTransaction {
            tx,
            signer: contributor_kp.public_key(),
            signature,
            pow_nonce,
            pow_hash,
        };

        execute_submit_attestation(
            state,
            ctx,
            contributor_addr,
            &signed_tx,
            project_id,
            units,
            &[0xAB; 32],
            contributor_addr,
            U256::from(1000u64),
            &test_denom(),
        )
        .unwrap()
    }

    fn create_challenge_tx(
        kp: &KeyPair,
        attestation_id: [u8; 20],
        bond_poster: Option<[u8; 20]>,
        bond_amount: U256,
    ) -> SignedTransaction {
        let tx = Transaction::ChallengeAttestation {
            attestation_id,
            evidence_hash: [0xCD; 32],
            bond_poster,
            bond_amount,
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
    fn test_challenge_attestation() {
        let mut state = ProtocolState::new();
        let ctx = test_context();

        let (creator_addr, creator_kp) = create_identity(&mut state, &ctx);
        let project_id = create_project_with_challenge_bond(&mut state, &ctx, &creator_addr, &creator_kp, 0);

        let (contributor_addr, contributor_kp) = create_identity(&mut state, &ctx);
        let attestation_id = submit_attestation(
            &mut state,
            &ctx,
            &project_id,
            &contributor_addr,
            &contributor_kp,
            100,
        );

        let (challenger_addr, challenger_kp) = create_identity(&mut state, &ctx);
        let signed_tx = create_challenge_tx(&challenger_kp, attestation_id, None, U256::zero());

        let challenge_id = execute_challenge_attestation(
            &mut state,
            &ctx,
            &challenger_addr,
            &signed_tx,
            &attestation_id,
            &[0xCD; 32],
            None,
            U256::zero(),
        )
        .unwrap();

        let challenge = state.get_challenge(&challenge_id).unwrap();
        assert_eq!(challenge.status, ChallengeStatus::Open);
        assert_eq!(challenge.challenger, challenger_addr);

        let attestation = state.get_attestation(&attestation_id).unwrap();
        assert_eq!(attestation.status, AttestationStatus::Challenged);
    }

    #[test]
    fn test_resolve_challenge_valid() {
        let mut state = ProtocolState::new();
        let ctx = test_context();

        let (creator_addr, creator_kp) = create_identity(&mut state, &ctx);
        let project_id = create_project_with_challenge_bond(&mut state, &ctx, &creator_addr, &creator_kp, 0);

        let (contributor_addr, contributor_kp) = create_identity(&mut state, &ctx);
        let attestation_id = submit_attestation(
            &mut state,
            &ctx,
            &project_id,
            &contributor_addr,
            &contributor_kp,
            100,
        );

        let (challenger_addr, challenger_kp) = create_identity(&mut state, &ctx);
        let signed_tx = create_challenge_tx(&challenger_kp, attestation_id, None, U256::zero());

        let challenge_id = execute_challenge_attestation(
            &mut state,
            &ctx,
            &challenger_addr,
            &signed_tx,
            &attestation_id,
            &[0xCD; 32],
            None,
            U256::zero(),
        )
        .unwrap();

        // Resolve as Valid (attestation stands)
        execute_resolve_challenge(
            &mut state,
            &ctx,
            &creator_addr, // adjudicator
            &challenge_id,
            &ChallengeRuling::Valid,
        )
        .unwrap();

        let challenge = state.get_challenge(&challenge_id).unwrap();
        assert_eq!(challenge.status, ChallengeStatus::ResolvedValid);

        let attestation = state.get_attestation(&attestation_id).unwrap();
        assert_eq!(attestation.status, AttestationStatus::Finalized);

        // Bond should be returned in full
        assert_eq!(
            state.get_claimable(&contributor_addr, &test_denom()),
            U256::from(1000u64)
        );
    }

    #[test]
    fn test_resolve_challenge_invalid() {
        let mut state = ProtocolState::new();
        let ctx = test_context();

        let (creator_addr, creator_kp) = create_identity(&mut state, &ctx);
        let project_id = create_project_with_challenge_bond(&mut state, &ctx, &creator_addr, &creator_kp, 0);

        let (contributor_addr, contributor_kp) = create_identity(&mut state, &ctx);
        let attestation_id = submit_attestation(
            &mut state,
            &ctx,
            &project_id,
            &contributor_addr,
            &contributor_kp,
            100,
        );

        let (challenger_addr, challenger_kp) = create_identity(&mut state, &ctx);
        let signed_tx = create_challenge_tx(&challenger_kp, attestation_id, None, U256::zero());

        let challenge_id = execute_challenge_attestation(
            &mut state,
            &ctx,
            &challenger_addr,
            &signed_tx,
            &attestation_id,
            &[0xCD; 32],
            None,
            U256::zero(),
        )
        .unwrap();

        // Resolve as Invalid (challenge succeeds)
        execute_resolve_challenge(
            &mut state,
            &ctx,
            &creator_addr, // adjudicator
            &challenge_id,
            &ChallengeRuling::Invalid,
        )
        .unwrap();

        let challenge = state.get_challenge(&challenge_id).unwrap();
        assert_eq!(challenge.status, ChallengeStatus::ResolvedInvalid);

        let attestation = state.get_attestation(&attestation_id).unwrap();
        assert_eq!(attestation.status, AttestationStatus::Rejected);

        // Bond should be forfeited to challenger (full amount)
        assert_eq!(
            state.get_claimable(&challenger_addr, &test_denom()),
            U256::from(1000u64)
        );
    }

    #[test]
    fn test_challenger_cannot_be_contributor() {
        let mut state = ProtocolState::new();
        let ctx = test_context();

        let (creator_addr, creator_kp) = create_identity(&mut state, &ctx);
        let project_id = create_project_with_challenge_bond(&mut state, &ctx, &creator_addr, &creator_kp, 0);

        let (contributor_addr, contributor_kp) = create_identity(&mut state, &ctx);
        let attestation_id = submit_attestation(
            &mut state,
            &ctx,
            &project_id,
            &contributor_addr,
            &contributor_kp,
            100,
        );

        // Contributor tries to challenge their own attestation
        let signed_tx = create_challenge_tx(&contributor_kp, attestation_id, None, U256::zero());

        let result = execute_challenge_attestation(
            &mut state,
            &ctx,
            &contributor_addr,
            &signed_tx,
            &attestation_id,
            &[0xCD; 32],
            None,
            U256::zero(),
        );

        assert!(matches!(result, Err(StateError::ChallengerIsContributor { .. })));
    }
}
