//! Revenue transaction handlers.
//!
//! Handles DepositRevenue, ClaimRevenue, and ClaimReserve.

use tunnels_core::{Denomination, IdentityType, U256};

use crate::accumulator::claim_labor_pool_revenue;
use crate::error::{StateError, StateResult};
use crate::state::StateWriter;
use crate::waterfall::execute_waterfall;

use super::context::ExecutionContext;

/// Execute a DepositRevenue transaction.
///
/// Routes the deposit through the waterfall algorithm, distributing to:
/// - Predecessor projects (delta%)
/// - Reserve pool (rho%)
/// - Fee recipient (phi%)
/// - Genesis recipient (gamma%)
/// - Labor pool (remainder)
///
/// # Validation
/// - Project must exist
/// - Amount must be > 0
/// - Signer must be the deposit_authority
pub fn execute_deposit_revenue<S: StateWriter>(
    state: &mut S,
    _ctx: &ExecutionContext,
    signer_address: &[u8; 20],
    project_id: &[u8; 20],
    amount: U256,
    denom: &Denomination,
) -> StateResult<()> {
    // Validate amount > 0
    if amount.is_zero() {
        return Err(StateError::ZeroDeposit);
    }

    // Get project and validate authority
    let project = state
        .get_project(project_id)
        .ok_or(StateError::ProjectNotFound {
            project_id: *project_id,
        })?;

    // Verify signer is deposit_authority
    if *signer_address != project.deposit_authority {
        return Err(StateError::NotDepositAuthority {
            authority: project.deposit_authority,
            signer: *signer_address,
        });
    }

    // Execute waterfall starting at depth 0
    execute_waterfall(state, project_id, amount, denom, 0)?;

    Ok(())
}

/// Execute a ClaimRevenue transaction.
///
/// Claims accumulated revenue from the labor pool for a contributor.
/// For agents, the parent Human must sign.
///
/// # Validation
/// - Contributor must exist and be active
/// - If agent, parent must sign
/// - Must have claimable balance
pub fn execute_claim_revenue<S: StateWriter>(
    state: &mut S,
    _ctx: &ExecutionContext,
    signer_address: &[u8; 20],
    project_id: &[u8; 20],
    contributor: &[u8; 20],
    denom: &Denomination,
) -> StateResult<U256> {
    // Validate project exists
    let _project = state
        .get_project(project_id)
        .ok_or(StateError::ProjectNotFound {
            project_id: *project_id,
        })?;

    // Validate contributor exists and is active, extract needed fields
    let (identity_type, parent_opt) = {
        let identity = state
            .get_identity(contributor)
            .ok_or(StateError::IdentityNotFound {
                address: *contributor,
            })?;

        if !identity.active {
            return Err(StateError::IdentityDeactivated {
                address: *contributor,
            });
        }

        (identity.identity_type, identity.parent)
    };

    // Validate signer authorization
    match identity_type {
        IdentityType::Human => {
            // Human must sign for themselves
            if signer_address != contributor {
                return Err(StateError::UnauthorizedSigner {
                    expected: *contributor,
                    actual: *signer_address,
                });
            }
        }
        IdentityType::Agent => {
            // Parent must sign for agent
            let parent = parent_opt.ok_or(StateError::ParentNotFound {
                parent: *contributor,
            })?;
            if *signer_address != parent {
                return Err(StateError::UnauthorizedSigner {
                    expected: parent,
                    actual: *signer_address,
                });
            }
        }
    }

    // Claim from labor pool
    let claimed = claim_labor_pool_revenue(state, project_id, contributor, denom)?;

    // Credit to the actual recipient (for agents, this should go to parent)
    // Per protocol: agent revenue routes to parent Human
    let recipient = match identity_type {
        IdentityType::Human => *contributor,
        IdentityType::Agent => parent_opt.unwrap_or(*contributor),
    };

    state.credit_claimable(&recipient, denom, claimed);

    Ok(claimed)
}

/// Execute a ClaimReserve transaction.
///
/// Claims funds from the project's reserve pool.
///
/// # Validation
/// - Project must exist
/// - Signer must be reserve_authority
/// - Amount must be <= reserve_balance
pub fn execute_claim_reserve<S: StateWriter>(
    state: &mut S,
    _ctx: &ExecutionContext,
    signer_address: &[u8; 20],
    project_id: &[u8; 20],
    recipient: &[u8; 20],
    amount: U256,
    denom: &Denomination,
) -> StateResult<()> {
    // Validate amount > 0
    if amount.is_zero() {
        return Err(StateError::ZeroClaim);
    }

    // Get project and validate authority
    let project = state
        .get_project(project_id)
        .ok_or(StateError::ProjectNotFound {
            project_id: *project_id,
        })?;

    // Verify signer is reserve_authority
    if *signer_address != project.reserve_authority {
        return Err(StateError::NotReserveAuthority {
            authority: project.reserve_authority,
            signer: *signer_address,
        });
    }

    // Check recipient exists
    let recipient_identity = state
        .get_identity(recipient)
        .ok_or(StateError::IdentityNotFound {
            address: *recipient,
        })?;

    if !recipient_identity.active {
        return Err(StateError::IdentityDeactivated {
            address: *recipient,
        });
    }

    // Get accumulator and check reserve balance
    let acc = state
        .get_accumulator(project_id, denom)
        .ok_or(StateError::InsufficientReserve {
            available: U256::zero(),
            requested: amount,
        })?;

    if acc.reserve_balance < amount {
        return Err(StateError::InsufficientReserve {
            available: acc.reserve_balance,
            requested: amount,
        });
    }

    // Deduct from reserve
    state.update_accumulator(project_id, denom, |acc| {
        acc.reserve_balance = acc.reserve_balance - amount;
    });

    // Credit to recipient
    state.credit_claimable(recipient, denom, amount);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accumulator::distribute_to_labor_pool;
    use crate::execute::identity::execute_create_identity;
    use crate::execute::project::{execute_register_project, MIN_VERIFICATION_WINDOW};
    use crate::state::{ProtocolState, StateReader, StateWriter};
    use tunnels_core::crypto::sign;
    use tunnels_core::serialization::serialize;
    use tunnels_core::transaction::mine_pow;
    use tunnels_core::{KeyPair, RecipientType, SignedTransaction, Transaction};

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
            rho: 1000,  // 10% reserve
            phi: 500,   // 5% fee
            gamma: 1500, // 15% genesis
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
            challenge_bond: 0,
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

    #[test]
    fn test_deposit_revenue() {
        let mut state = ProtocolState::new();
        let ctx = test_context();

        let (creator_addr, creator_kp) = create_identity(&mut state, &ctx);
        let project_id = create_project(&mut state, &ctx, &creator_addr, &creator_kp);

        let amount = U256::from(10000u64);
        execute_deposit_revenue(
            &mut state,
            &ctx,
            &creator_addr,
            &project_id,
            amount,
            &test_denom(),
        )
        .unwrap();

        // Check cascading waterfall distribution:
        // D = 10000
        // reserve = 10000 * 10% = 1000, remaining = 9000
        // fee = 9000 * 5% = 450, remaining = 8550
        // genesis = 8550 * 15% = 1282, remaining = 7268
        // labor = 7268
        let acc = state.get_accumulator(&project_id, &test_denom()).unwrap();
        // Reserve: 10% of 10000 = 1000
        assert_eq!(acc.reserve_balance, U256::from(1000u64));

        // Labor pool (unallocated since no contributors): 7268
        assert_eq!(acc.unallocated, U256::from(7268u64));

        // Phi + gamma to creator: 450 + 1282 = 1732
        assert_eq!(
            state.get_claimable(&creator_addr, &test_denom()),
            U256::from(1732u64)
        );
    }

    #[test]
    fn test_deposit_revenue_zero_fails() {
        let mut state = ProtocolState::new();
        let ctx = test_context();

        let (creator_addr, creator_kp) = create_identity(&mut state, &ctx);
        let project_id = create_project(&mut state, &ctx, &creator_addr, &creator_kp);

        let result = execute_deposit_revenue(
            &mut state,
            &ctx,
            &creator_addr,
            &project_id,
            U256::zero(),
            &test_denom(),
        );

        assert!(matches!(result, Err(StateError::ZeroDeposit)));
    }

    #[test]
    fn test_deposit_revenue_wrong_authority_fails() {
        let mut state = ProtocolState::new();
        let ctx = test_context();

        let (creator_addr, creator_kp) = create_identity(&mut state, &ctx);
        let project_id = create_project(&mut state, &ctx, &creator_addr, &creator_kp);

        let (other_addr, _) = create_identity(&mut state, &ctx);

        let result = execute_deposit_revenue(
            &mut state,
            &ctx,
            &other_addr, // Wrong authority
            &project_id,
            U256::from(1000u64),
            &test_denom(),
        );

        assert!(matches!(result, Err(StateError::NotDepositAuthority { .. })));
    }

    #[test]
    fn test_claim_reserve() {
        let mut state = ProtocolState::new();
        let ctx = test_context();

        let (creator_addr, creator_kp) = create_identity(&mut state, &ctx);
        let project_id = create_project(&mut state, &ctx, &creator_addr, &creator_kp);

        // Deposit to create reserve
        execute_deposit_revenue(
            &mut state,
            &ctx,
            &creator_addr,
            &project_id,
            U256::from(10000u64),
            &test_denom(),
        )
        .unwrap();

        // Reserve should have 1000 (10%)
        let acc = state.get_accumulator(&project_id, &test_denom()).unwrap();
        assert_eq!(acc.reserve_balance, U256::from(1000u64));

        // Claim 500 from reserve
        execute_claim_reserve(
            &mut state,
            &ctx,
            &creator_addr,
            &project_id,
            &creator_addr,
            U256::from(500u64),
            &test_denom(),
        )
        .unwrap();

        // Check reserve decreased
        let acc = state.get_accumulator(&project_id, &test_denom()).unwrap();
        assert_eq!(acc.reserve_balance, U256::from(500u64));

        // Check claimable increased:
        // From waterfall: phi (450) + gamma (1282) = 1732
        // From reserve claim: + 500
        // Total: 2232
        assert_eq!(
            state.get_claimable(&creator_addr, &test_denom()),
            U256::from(2232u64)
        );
    }

    #[test]
    fn test_claim_reserve_insufficient_fails() {
        let mut state = ProtocolState::new();
        let ctx = test_context();

        let (creator_addr, creator_kp) = create_identity(&mut state, &ctx);
        let project_id = create_project(&mut state, &ctx, &creator_addr, &creator_kp);

        // Deposit to create reserve (1000)
        execute_deposit_revenue(
            &mut state,
            &ctx,
            &creator_addr,
            &project_id,
            U256::from(10000u64),
            &test_denom(),
        )
        .unwrap();

        // Try to claim more than available
        let result = execute_claim_reserve(
            &mut state,
            &ctx,
            &creator_addr,
            &project_id,
            &creator_addr,
            U256::from(2000u64), // Only 1000 available
            &test_denom(),
        );

        assert!(matches!(result, Err(StateError::InsufficientReserve { .. })));
    }

    #[test]
    fn test_claim_revenue_from_labor_pool() {
        let mut state = ProtocolState::new();
        let ctx = test_context();

        let (creator_addr, creator_kp) = create_identity(&mut state, &ctx);
        let project_id = create_project(&mut state, &ctx, &creator_addr, &creator_kp);

        // Create a contributor
        let (contributor_addr, _) = create_identity(&mut state, &ctx);

        // Manually set up accumulator with finalized units (simulating attestation finalization)
        {
            let acc = state.get_or_create_accumulator(&project_id, &test_denom());
            acc.total_finalized_units = 100;
        }

        {
            let balance = state.get_or_create_balance(&project_id, &contributor_addr, &test_denom());
            balance.units = 100;
        }

        // Deposit revenue (this will go to labor pool since we have finalized units)
        // Note: We're using distribute directly to test claim, not the full deposit
        distribute_to_labor_pool(&mut state, &project_id, U256::from(1000u64), &test_denom()).unwrap();

        // Claim revenue
        let claimed = execute_claim_revenue(
            &mut state,
            &ctx,
            &contributor_addr,
            &project_id,
            &contributor_addr,
            &test_denom(),
        )
        .unwrap();

        assert_eq!(claimed, U256::from(1000u64));

        // Check claimable balance updated
        assert_eq!(
            state.get_claimable(&contributor_addr, &test_denom()),
            U256::from(1000u64)
        );
    }

    #[test]
    fn test_claim_revenue_nothing_to_claim() {
        let mut state = ProtocolState::new();
        let ctx = test_context();

        let (creator_addr, creator_kp) = create_identity(&mut state, &ctx);
        let project_id = create_project(&mut state, &ctx, &creator_addr, &creator_kp);

        let (contributor_addr, _) = create_identity(&mut state, &ctx);

        // Try to claim with no units or deposits
        let result = execute_claim_revenue(
            &mut state,
            &ctx,
            &contributor_addr,
            &project_id,
            &contributor_addr,
            &test_denom(),
        );

        assert!(matches!(result, Err(StateError::NothingToClaim { .. })));
    }
}
