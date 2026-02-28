//! Project registration transaction handler.

use tunnels_core::crypto::sha256;
use tunnels_core::serialization::serialize;
use tunnels_core::{Project, RecipientType, SignedTransaction, Transaction};

use crate::error::{StateError, StateResult};
use crate::state::{StateReader, StateWriter};
use super::context::ExecutionContext;

/// Maximum depth for predecessor and gamma chains.
pub const MAX_CHAIN_DEPTH: u8 = 8;

/// Minimum verification window in seconds (24 hours).
/// Used in tests - the actual minimum is taken from ExecutionContext.
#[allow(dead_code)]
pub const MIN_VERIFICATION_WINDOW: u64 = 86_400;

/// Maximum basis points (100%).
pub const MAX_BASIS_POINTS: u32 = 10_000;

/// Execute a RegisterProject transaction.
///
/// # Validation
/// - Signer must be an existing, active identity
/// - rho + phi + gamma + delta <= 10000
/// - If phi > 0, phi_recipient must exist
/// - If gamma > 0, gamma_recipient must exist (identity or project)
/// - If gamma_recipient is project, check for cycles and depth <= 8
/// - If predecessor is set, delta > 0 and predecessor chain depth <= 8
/// - verification_window >= ctx.min_verification_window (24 hours default, 10s devnet)
/// - challenge_bond <= attestation_bond
pub fn execute_register_project<S: StateWriter>(
    state: &mut S,
    ctx: &ExecutionContext,
    signer_address: &[u8; 20],
    signed_tx: &SignedTransaction,
    tx: &Transaction,
) -> StateResult<[u8; 20]> {
    // Extract project fields from transaction
    let (
        creator,
        rho,
        phi,
        gamma,
        delta,
        phi_recipient,
        gamma_recipient,
        gamma_recipient_type,
        reserve_authority,
        deposit_authority,
        adjudicator,
        predecessor,
        verification_window,
        attestation_bond,
        challenge_bond,
        evidence_hash,
    ) = match tx {
        Transaction::RegisterProject {
            creator,
            rho,
            phi,
            gamma,
            delta,
            phi_recipient,
            gamma_recipient,
            gamma_recipient_type,
            reserve_authority,
            deposit_authority,
            adjudicator,
            predecessor,
            verification_window,
            attestation_bond,
            challenge_bond,
            evidence_hash,
        } => (
            creator,
            *rho,
            *phi,
            *gamma,
            *delta,
            phi_recipient,
            gamma_recipient,
            gamma_recipient_type,
            reserve_authority,
            deposit_authority,
            adjudicator,
            predecessor,
            *verification_window,
            *attestation_bond,
            *challenge_bond,
            evidence_hash,
        ),
        _ => unreachable!("execute_register_project called with non-RegisterProject transaction"),
    };

    // Verify signer is the creator
    if signer_address != creator {
        return Err(StateError::UnauthorizedSigner {
            expected: *creator,
            actual: *signer_address,
        });
    }

    // Verify creator exists and is active
    let creator_identity = state
        .get_identity(creator)
        .ok_or(StateError::IdentityNotFound { address: *creator })?;
    if !creator_identity.active {
        return Err(StateError::IdentityDeactivated { address: *creator });
    }

    // Validate basis points sum
    let sum = rho + phi + gamma + delta;
    if sum > MAX_BASIS_POINTS {
        return Err(StateError::InvalidBasisPointsSum { sum });
    }

    // Validate verification window
    if verification_window < ctx.min_verification_window {
        return Err(StateError::VerificationWindowTooShort {
            window: verification_window,
            minimum: ctx.min_verification_window,
        });
    }

    // Validate challenge bond <= attestation bond
    if challenge_bond > attestation_bond {
        return Err(StateError::ChallengeBondExceedsAttestationBond {
            challenge_bond,
            attestation_bond,
        });
    }

    // Validate phi_recipient if phi > 0
    if phi > 0 {
        let recipient = state
            .get_identity(phi_recipient)
            .ok_or(StateError::PhiRecipientNotFound {
                recipient: *phi_recipient,
            })?;
        if !recipient.active {
            return Err(StateError::IdentityDeactivated {
                address: *phi_recipient,
            });
        }
    }

    // Validate gamma_recipient if gamma > 0
    if gamma > 0 {
        match gamma_recipient_type {
            RecipientType::Identity => {
                let recipient = state
                    .get_identity(gamma_recipient)
                    .ok_or(StateError::GammaRecipientIdentityNotFound {
                        recipient: *gamma_recipient,
                    })?;
                if !recipient.active {
                    return Err(StateError::IdentityDeactivated {
                        address: *gamma_recipient,
                    });
                }
            }
            RecipientType::Project => {
                // Verify gamma recipient project exists
                if !state.project_exists(gamma_recipient) {
                    return Err(StateError::GammaRecipientProjectNotFound {
                        recipient: *gamma_recipient,
                    });
                }

                // Check gamma chain depth
                let depth = compute_gamma_chain_depth(state, gamma_recipient)?;
                if depth >= MAX_CHAIN_DEPTH {
                    return Err(StateError::GammaChainTooDeep {
                        depth,
                        limit: MAX_CHAIN_DEPTH,
                    });
                }
            }
        }
    }

    // Validate reserve_authority if rho > 0
    if rho > 0 {
        let authority = state
            .get_identity(reserve_authority)
            .ok_or(StateError::ReserveAuthorityNotFound {
                authority: *reserve_authority,
            })?;
        if !authority.active {
            return Err(StateError::IdentityDeactivated {
                address: *reserve_authority,
            });
        }
    }

    // Validate deposit_authority
    let authority = state
        .get_identity(deposit_authority)
        .ok_or(StateError::DepositAuthorityNotFound {
            authority: *deposit_authority,
        })?;
    if !authority.active {
        return Err(StateError::IdentityDeactivated {
            address: *deposit_authority,
        });
    }

    // Validate adjudicator
    let adj = state
        .get_identity(adjudicator)
        .ok_or(StateError::AdjudicatorNotFound {
            adjudicator: *adjudicator,
        })?;
    if !adj.active {
        return Err(StateError::IdentityDeactivated {
            address: *adjudicator,
        });
    }

    // Validate predecessor relationship
    match (predecessor, delta) {
        (Some(_), 0) => {
            return Err(StateError::PredecessorWithoutDelta);
        }
        (None, d) if d > 0 => {
            return Err(StateError::DeltaWithoutPredecessor);
        }
        (Some(pred_id), _) => {
            // Verify predecessor exists
            if !state.project_exists(pred_id) {
                return Err(StateError::PredecessorNotFound {
                    predecessor: *pred_id,
                });
            }

            // Check predecessor chain depth
            let depth = compute_predecessor_chain_depth(state, pred_id)?;
            if depth >= MAX_CHAIN_DEPTH {
                return Err(StateError::PredecessorChainTooDeep {
                    depth,
                    limit: MAX_CHAIN_DEPTH,
                });
            }
        }
        (None, _) => {
            // No predecessor, delta is 0 - valid
        }
    }

    // Derive project_id from hash of the signed transaction
    let tx_bytes = serialize(signed_tx).expect("SignedTransaction serialization should not fail");
    let hash = sha256(&tx_bytes);
    let mut project_id = [0u8; 20];
    project_id.copy_from_slice(&hash[..20]);

    // Check project doesn't already exist
    if state.project_exists(&project_id) {
        return Err(StateError::ProjectAlreadyExists { project_id });
    }

    // Create project
    let project = Project {
        project_id,
        creator: *creator,
        rho,
        phi,
        gamma,
        delta,
        phi_recipient: *phi_recipient,
        gamma_recipient: *gamma_recipient,
        gamma_recipient_type: *gamma_recipient_type,
        reserve_authority: *reserve_authority,
        deposit_authority: *deposit_authority,
        adjudicator: *adjudicator,
        predecessor: *predecessor,
        verification_window,
        attestation_bond,
        challenge_bond,
        evidence_hash: *evidence_hash,
    };

    state.insert_project(project);

    Ok(project_id)
}

/// Compute the depth of a predecessor chain.
fn compute_predecessor_chain_depth<S: StateReader>(
    state: &mut S,
    project_id: &[u8; 20],
) -> StateResult<u8> {
    let mut depth = 0u8;
    let mut current = *project_id;

    while depth < MAX_CHAIN_DEPTH + 1 {
        let project = state.get_project(&current);
        match project {
            Some(p) => match p.predecessor {
                Some(pred) => {
                    depth += 1;
                    current = pred;
                }
                None => break,
            },
            None => break,
        }
    }

    Ok(depth)
}

/// Compute the depth of a gamma-to-project chain.
fn compute_gamma_chain_depth<S: StateReader>(
    state: &mut S,
    project_id: &[u8; 20],
) -> StateResult<u8> {
    let mut depth = 0u8;
    let mut current = *project_id;
    let mut visited = std::collections::HashSet::new();

    while depth < MAX_CHAIN_DEPTH + 1 {
        if !visited.insert(current) {
            // Cycle detected
            return Err(StateError::GammaCycleDetected { project_id: current });
        }

        let project = state.get_project(&current);
        match project {
            Some(p) => {
                if p.gamma > 0 && p.gamma_recipient_type == RecipientType::Project {
                    depth += 1;
                    current = p.gamma_recipient;
                } else {
                    break;
                }
            }
            None => break,
        }
    }

    Ok(depth)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{ProtocolState, StateReader};
    use crate::execute::identity::execute_create_identity;
    use tunnels_core::crypto::sign;
    use tunnels_core::transaction::mine_pow;
    use tunnels_core::{KeyPair, U256};

    fn test_context() -> ExecutionContext {
        ExecutionContext::test_context()
    }

    fn create_identity(state: &mut ProtocolState, ctx: &ExecutionContext) -> ([u8; 20], KeyPair) {
        let kp = KeyPair::generate();
        let address = execute_create_identity(state, ctx, &kp.public_key()).unwrap();
        (address, kp)
    }

    fn create_signed_register_project_tx(
        kp: &KeyPair,
        creator: [u8; 20],
        phi_recipient: [u8; 20],
        gamma_recipient: [u8; 20],
        gamma_recipient_type: RecipientType,
        reserve_authority: [u8; 20],
        deposit_authority: [u8; 20],
        adjudicator: [u8; 20],
        predecessor: Option<[u8; 20]>,
        rho: u32,
        phi: u32,
        gamma: u32,
        delta: u32,
    ) -> SignedTransaction {
        let tx = Transaction::RegisterProject {
            creator,
            rho,
            phi,
            gamma,
            delta,
            phi_recipient,
            gamma_recipient,
            gamma_recipient_type,
            reserve_authority,
            deposit_authority,
            adjudicator,
            predecessor,
            verification_window: MIN_VERIFICATION_WINDOW,
            attestation_bond: 100,
            challenge_bond: 50,
            evidence_hash: None,
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
    fn test_register_project_basic() {
        let mut state = ProtocolState::new();
        let ctx = test_context();

        let (creator_addr, creator_kp) = create_identity(&mut state, &ctx);

        let signed_tx = create_signed_register_project_tx(
            &creator_kp,
            creator_addr,
            creator_addr,
            creator_addr,
            RecipientType::Identity,
            creator_addr,
            creator_addr,
            creator_addr,
            None,
            1000,  // 10% reserve
            500,   // 5% fee
            1500,  // 15% genesis
            0,     // no predecessor
        );

        let project_id = execute_register_project(
            &mut state,
            &ctx,
            &creator_addr,
            &signed_tx,
            &signed_tx.tx,
        )
        .unwrap();

        let project = state.get_project(&project_id).unwrap();
        assert_eq!(project.creator, creator_addr);
        assert_eq!(project.rho, 1000);
        assert_eq!(project.phi, 500);
        assert_eq!(project.gamma, 1500);
        assert_eq!(project.delta, 0);
        assert!(project.predecessor.is_none());
    }

    #[test]
    fn test_register_project_basis_points_overflow() {
        let mut state = ProtocolState::new();
        let ctx = test_context();

        let (creator_addr, creator_kp) = create_identity(&mut state, &ctx);

        let signed_tx = create_signed_register_project_tx(
            &creator_kp,
            creator_addr,
            creator_addr,
            creator_addr,
            RecipientType::Identity,
            creator_addr,
            creator_addr,
            creator_addr,
            None,
            3000,
            3000,
            3000,
            2000, // Total: 11000 > 10000
        );

        let result = execute_register_project(
            &mut state,
            &ctx,
            &creator_addr,
            &signed_tx,
            &signed_tx.tx,
        );

        assert!(matches!(result, Err(StateError::InvalidBasisPointsSum { sum: 11000 })));
    }

    #[test]
    fn test_register_project_verification_window_too_short() {
        let mut state = ProtocolState::new();
        let ctx = test_context();

        let (creator_addr, creator_kp) = create_identity(&mut state, &ctx);

        let tx = Transaction::RegisterProject {
            creator: creator_addr,
            rho: 1000,
            phi: 500,
            gamma: 1500,
            delta: 0,
            phi_recipient: creator_addr,
            gamma_recipient: creator_addr,
            gamma_recipient_type: RecipientType::Identity,
            reserve_authority: creator_addr,
            deposit_authority: creator_addr,
            adjudicator: creator_addr,
            predecessor: None,
            verification_window: 1000, // Too short
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

        let result = execute_register_project(
            &mut state,
            &ctx,
            &creator_addr,
            &signed_tx,
            &tx,
        );

        assert!(matches!(result, Err(StateError::VerificationWindowTooShort { .. })));
    }

    #[test]
    fn test_register_project_delta_without_predecessor() {
        let mut state = ProtocolState::new();
        let ctx = test_context();

        let (creator_addr, creator_kp) = create_identity(&mut state, &ctx);

        let signed_tx = create_signed_register_project_tx(
            &creator_kp,
            creator_addr,
            creator_addr,
            creator_addr,
            RecipientType::Identity,
            creator_addr,
            creator_addr,
            creator_addr,
            None,   // No predecessor
            1000,
            500,
            1500,
            1000,   // But delta > 0
        );

        let result = execute_register_project(
            &mut state,
            &ctx,
            &creator_addr,
            &signed_tx,
            &signed_tx.tx,
        );

        assert!(matches!(result, Err(StateError::DeltaWithoutPredecessor)));
    }

    #[test]
    fn test_register_project_with_predecessor() {
        let mut state = ProtocolState::new();
        let ctx = test_context();

        let (creator_addr, creator_kp) = create_identity(&mut state, &ctx);

        // Create predecessor project
        let signed_tx1 = create_signed_register_project_tx(
            &creator_kp,
            creator_addr,
            creator_addr,
            creator_addr,
            RecipientType::Identity,
            creator_addr,
            creator_addr,
            creator_addr,
            None,
            1000,
            500,
            1500,
            0,
        );

        let pred_id = execute_register_project(
            &mut state,
            &ctx,
            &creator_addr,
            &signed_tx1,
            &signed_tx1.tx,
        )
        .unwrap();

        // Create project with predecessor
        let signed_tx2 = create_signed_register_project_tx(
            &creator_kp,
            creator_addr,
            creator_addr,
            creator_addr,
            RecipientType::Identity,
            creator_addr,
            creator_addr,
            creator_addr,
            Some(pred_id),
            1000,
            500,
            1500,
            500, // 5% to predecessor
        );

        let project_id = execute_register_project(
            &mut state,
            &ctx,
            &creator_addr,
            &signed_tx2,
            &signed_tx2.tx,
        )
        .unwrap();

        let project = state.get_project(&project_id).unwrap();
        assert_eq!(project.predecessor, Some(pred_id));
        assert_eq!(project.delta, 500);
    }

    fn create_register_project_tx_with_bonds(
        kp: &KeyPair,
        creator: [u8; 20],
        attestation_bond: u64,
        challenge_bond: u64,
    ) -> SignedTransaction {
        let tx = Transaction::RegisterProject {
            creator,
            rho: 1000,
            phi: 500,
            gamma: 1500,
            delta: 0,
            phi_recipient: creator,
            gamma_recipient: creator,
            gamma_recipient_type: RecipientType::Identity,
            reserve_authority: creator,
            deposit_authority: creator,
            adjudicator: creator,
            predecessor: None,
            verification_window: MIN_VERIFICATION_WINDOW,
            attestation_bond,
            challenge_bond,
            evidence_hash: None,
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
    fn test_register_project_challenge_bond_exceeds_attestation_bond() {
        let mut state = ProtocolState::new();
        let ctx = test_context();

        let (creator_addr, creator_kp) = create_identity(&mut state, &ctx);

        // challenge_bond (150) > attestation_bond (100) - should fail
        let signed_tx = create_register_project_tx_with_bonds(
            &creator_kp,
            creator_addr,
            100,  // attestation_bond
            150,  // challenge_bond > attestation_bond
        );

        let result = execute_register_project(
            &mut state,
            &ctx,
            &creator_addr,
            &signed_tx,
            &signed_tx.tx,
        );

        assert!(matches!(
            result,
            Err(StateError::ChallengeBondExceedsAttestationBond {
                challenge_bond: 150,
                attestation_bond: 100
            })
        ));
    }

    #[test]
    fn test_register_project_challenge_bond_equals_attestation_bond() {
        let mut state = ProtocolState::new();
        let ctx = test_context();

        let (creator_addr, creator_kp) = create_identity(&mut state, &ctx);

        // challenge_bond == attestation_bond - should succeed
        let signed_tx = create_register_project_tx_with_bonds(
            &creator_kp,
            creator_addr,
            100,  // attestation_bond
            100,  // challenge_bond == attestation_bond
        );

        let result = execute_register_project(
            &mut state,
            &ctx,
            &creator_addr,
            &signed_tx,
            &signed_tx.tx,
        );

        assert!(result.is_ok());
        let project_id = result.unwrap();
        let project = state.get_project(&project_id).unwrap();
        assert_eq!(project.attestation_bond, 100);
        assert_eq!(project.challenge_bond, 100);
    }

    #[test]
    fn test_register_project_challenge_bond_zero() {
        let mut state = ProtocolState::new();
        let ctx = test_context();

        let (creator_addr, creator_kp) = create_identity(&mut state, &ctx);

        // challenge_bond == 0 - should succeed
        let signed_tx = create_register_project_tx_with_bonds(
            &creator_kp,
            creator_addr,
            100,  // attestation_bond
            0,    // challenge_bond == 0
        );

        let result = execute_register_project(
            &mut state,
            &ctx,
            &creator_addr,
            &signed_tx,
            &signed_tx.tx,
        );

        assert!(result.is_ok());
        let project_id = result.unwrap();
        let project = state.get_project(&project_id).unwrap();
        assert_eq!(project.attestation_bond, 100);
        assert_eq!(project.challenge_bond, 0);
    }
}
