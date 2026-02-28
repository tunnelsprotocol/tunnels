//! Waterfall revenue distribution router.
//!
//! The waterfall distributes incoming revenue according to project parameters:
//! 1. Predecessor slice (delta%) - recursive deposit to predecessor
//! 2. Reserve slice (rho%) - credited to reserve_balance
//! 3. Fee slice (phi%) - credited to phi_recipient
//! 4. Genesis slice (gamma%) - credited to identity or recursive deposit to project
//! 5. Labor pool (remainder) - distributed via accumulator or held as unallocated

use tunnels_core::{Denomination, RecipientType, U256};

use crate::accumulator::distribute_to_labor_pool;
use crate::error::{StateError, StateResult};
use crate::state::StateWriter;

/// Maximum recursion depth for predecessor and gamma chains.
pub const MAX_CHAIN_DEPTH: u8 = 8;

/// Execute the waterfall distribution for a deposit.
///
/// This is the core revenue routing algorithm. It distributes `amount` according
/// to the project's parameters, potentially recursing into predecessor and gamma
/// projects up to a maximum depth.
///
/// # Arguments
/// - `state`: Protocol state
/// - `project_id`: Target project
/// - `amount`: Amount to distribute
/// - `denom`: Denomination of the deposit
/// - `depth`: Current recursion depth (starts at 0)
pub fn execute_waterfall<S: StateWriter>(
    state: &mut S,
    project_id: &[u8; 20],
    amount: U256,
    denom: &Denomination,
    depth: u8,
) -> StateResult<()> {
    // Nothing to distribute
    if amount.is_zero() {
        return Ok(());
    }

    // At max depth, treat entire amount as labor pool
    if depth > MAX_CHAIN_DEPTH {
        let acc = state.get_or_create_accumulator(project_id, denom);
        acc.unallocated = acc.unallocated + amount;
        return Ok(());
    }

    // Get project
    let project = state
        .get_project(project_id)
        .ok_or(StateError::ProjectNotFound {
            project_id: *project_id,
        })?
        .clone();

    let mut remaining = amount;

    // 1. Predecessor slice (delta% of remaining)
    if let Some(predecessor_id) = &project.predecessor {
        let delta_amount = compute_basis_points(remaining, project.delta);
        if !delta_amount.is_zero() {
            // Recursive deposit to predecessor
            execute_waterfall(state, predecessor_id, delta_amount, denom, depth + 1)?;
            remaining = remaining - delta_amount;
        }
    }

    // 2. Reserve slice (rho% of remaining)
    let reserve_amount = compute_basis_points(remaining, project.rho);
    if !reserve_amount.is_zero() {
        let acc = state.get_or_create_accumulator(project_id, denom);
        acc.reserve_balance = acc.reserve_balance + reserve_amount;
        remaining = remaining - reserve_amount;
    }

    // 3. Fee slice (phi% of remaining)
    let phi_amount = compute_basis_points(remaining, project.phi);
    if !phi_amount.is_zero() {
        state.credit_claimable(&project.phi_recipient, denom, phi_amount);
        remaining = remaining - phi_amount;
    }

    // 4. Genesis slice (gamma% of remaining)
    let gamma_amount = compute_basis_points(remaining, project.gamma);
    if !gamma_amount.is_zero() {
        match project.gamma_recipient_type {
            RecipientType::Identity => {
                state.credit_claimable(&project.gamma_recipient, denom, gamma_amount);
            }
            RecipientType::Project => {
                // Recursive deposit to gamma project (shares depth counter)
                execute_waterfall(state, &project.gamma_recipient, gamma_amount, denom, depth + 1)?;
            }
        }
        remaining = remaining - gamma_amount;
    }

    // 5. Labor pool (remainder)
    if !remaining.is_zero() {
        distribute_to_labor_pool(state, project_id, remaining, denom)?;
    }

    Ok(())
}

/// Compute basis points: (amount * bp) / 10000
///
/// Uses U256 arithmetic: multiply first, then divide to preserve precision.
fn compute_basis_points(amount: U256, bp: u32) -> U256 {
    if bp == 0 {
        return U256::zero();
    }
    (amount * U256::from(bp as u64)) / U256::from(10000u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execute::{
        execute_create_identity, execute_register_project, ExecutionContext, MIN_VERIFICATION_WINDOW,
    };
    use crate::state::{ProtocolState, StateReader};
    use tunnels_core::crypto::sign;
    use tunnels_core::serialization::serialize;
    use tunnels_core::transaction::mine_pow;
    use tunnels_core::{KeyPair, Transaction};

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
        rho: u32,
        phi: u32,
        gamma: u32,
        delta: u32,
        predecessor: Option<[u8; 20]>,
        gamma_recipient: [u8; 20],
        gamma_recipient_type: RecipientType,
    ) -> [u8; 20] {
        let tx = Transaction::RegisterProject {
            creator: *creator_addr,
            rho,
            phi,
            gamma,
            delta,
            phi_recipient: *creator_addr,
            gamma_recipient,
            gamma_recipient_type,
            reserve_authority: *creator_addr,
            deposit_authority: *creator_addr,
            adjudicator: *creator_addr,
            predecessor,
            verification_window: MIN_VERIFICATION_WINDOW,
            attestation_bond: 100,
            challenge_bond: 0,
            evidence_hash: None,
        };

        let tx_bytes = serialize(&tx).unwrap();
        let signature = sign(creator_kp.signing_key(), &tx_bytes);
        let (pow_nonce, pow_hash) = mine_pow(&tx, &creator_kp.public_key(), &signature, 1);

        let signed_tx = tunnels_core::SignedTransaction {
            tx: tx.clone(),
            signer: creator_kp.public_key(),
            signature,
            pow_nonce,
            pow_hash,
        };

        execute_register_project(state, ctx, creator_addr, &signed_tx, &tx).unwrap()
    }

    #[test]
    fn test_waterfall_simple() {
        // Test cascading waterfall distribution:
        // Project with rho=10%, phi=5%, gamma=15%, delta=0
        // Deposit 10,000
        //
        // Cascading calculation (each % from remaining after prior slices):
        //   D = 10,000
        //   predecessor = 0 (delta=0)
        //   R' = 10,000
        //   reserve = R' * 10% = 1,000
        //   R'' = 10,000 - 1,000 = 9,000
        //   fee = R'' * 5% = 450
        //   R''' = 9,000 - 450 = 8,550
        //   genesis = R''' * 15% = 1,282 (truncated)
        //   labor = 8,550 - 1,282 = 7,268

        let mut state = ProtocolState::new();
        let ctx = test_context();

        let (creator_addr, creator_kp) = create_identity(&mut state, &ctx);
        let project_id = create_project(
            &mut state,
            &ctx,
            &creator_addr,
            &creator_kp,
            1000, // 10% reserve
            500,  // 5% fee
            1500, // 15% genesis
            0,    // no predecessor
            None,
            creator_addr,
            RecipientType::Identity,
        );

        let amount = U256::from(10000u64);
        execute_waterfall(&mut state, &project_id, amount, &test_denom(), 0).unwrap();

        // Copy values from accumulator to avoid borrow conflicts
        let (reserve_balance, unallocated) = {
            let acc = state.get_accumulator(&project_id, &test_denom()).unwrap();
            (acc.reserve_balance, acc.unallocated)
        };

        // Reserve: 10% of 10,000 = 1,000
        assert_eq!(reserve_balance, U256::from(1000u64));

        // Fee + genesis credited to creator:
        // fee = 9,000 * 5% = 450
        // genesis = 8,550 * 15% = 1,282
        // total = 450 + 1,282 = 1,732
        assert_eq!(
            state.get_claimable(&creator_addr, &test_denom()),
            U256::from(450u64 + 1282u64)
        );

        // Labor pool: 8,550 - 1,282 = 7,268
        // Since no finalized units, it goes to unallocated
        assert_eq!(unallocated, U256::from(7268u64));
    }

    #[test]
    fn test_waterfall_predecessor_chain() {
        // Test acceptance criterion 6:
        // Three projects: C -> B -> A, each with delta=10%
        // Deposit 100,000 into C
        // Expected: B receives 10,000, A receives 1,000

        let mut state = ProtocolState::new();
        let ctx = test_context();

        let (creator_addr, creator_kp) = create_identity(&mut state, &ctx);

        // Project A (no predecessor)
        let project_a = create_project(
            &mut state,
            &ctx,
            &creator_addr,
            &creator_kp,
            0, 0, 0, 0,
            None,
            creator_addr,
            RecipientType::Identity,
        );

        // Project B (predecessor = A, delta = 10%)
        let project_b = create_project(
            &mut state,
            &ctx,
            &creator_addr,
            &creator_kp,
            0, 0, 0,
            1000, // 10%
            Some(project_a),
            creator_addr,
            RecipientType::Identity,
        );

        // Project C (predecessor = B, delta = 10%)
        let project_c = create_project(
            &mut state,
            &ctx,
            &creator_addr,
            &creator_kp,
            0, 0, 0,
            1000, // 10%
            Some(project_b),
            creator_addr,
            RecipientType::Identity,
        );

        let amount = U256::from(100000u64);
        execute_waterfall(&mut state, &project_c, amount, &test_denom(), 0).unwrap();

        // C's labor pool: 90,000 (100,000 - 10% to B)
        let acc_c = state.get_accumulator(&project_c, &test_denom()).unwrap();
        assert_eq!(acc_c.unallocated, U256::from(90000u64));

        // B's labor pool: 9,000 (10,000 - 10% to A)
        let acc_b = state.get_accumulator(&project_b, &test_denom()).unwrap();
        assert_eq!(acc_b.unallocated, U256::from(9000u64));

        // A's labor pool: 1,000
        let acc_a = state.get_accumulator(&project_a, &test_denom()).unwrap();
        assert_eq!(acc_a.unallocated, U256::from(1000u64));
    }

    #[test]
    fn test_waterfall_gamma_to_project() {
        // Test acceptance criterion 7:
        // Project M with gamma=20% pointing to Project F
        // F has no finalized units, so gamma slice goes to F's unallocated

        let mut state = ProtocolState::new();
        let ctx = test_context();

        let (creator_addr, creator_kp) = create_identity(&mut state, &ctx);

        // Project F (receives gamma from M)
        let project_f = create_project(
            &mut state,
            &ctx,
            &creator_addr,
            &creator_kp,
            0, 0, 0, 0,
            None,
            creator_addr,
            RecipientType::Identity,
        );

        // Project M (gamma = 20% to project F)
        let project_m = create_project(
            &mut state,
            &ctx,
            &creator_addr,
            &creator_kp,
            0, 0,
            2000, // 20% gamma
            0,
            None,
            project_f,
            RecipientType::Project,
        );

        let amount = U256::from(10000u64);
        execute_waterfall(&mut state, &project_m, amount, &test_denom(), 0).unwrap();

        // M's labor pool: 8,000 (10,000 - 20%)
        let acc_m = state.get_accumulator(&project_m, &test_denom()).unwrap();
        assert_eq!(acc_m.unallocated, U256::from(8000u64));

        // F's labor pool: 2,000 (gamma from M)
        let acc_f = state.get_accumulator(&project_f, &test_denom()).unwrap();
        assert_eq!(acc_f.unallocated, U256::from(2000u64));
    }

    #[test]
    fn test_compute_basis_points() {
        assert_eq!(compute_basis_points(U256::from(10000u64), 1000), U256::from(1000u64));
        assert_eq!(compute_basis_points(U256::from(10000u64), 500), U256::from(500u64));
        assert_eq!(compute_basis_points(U256::from(10000u64), 0), U256::zero());
        assert_eq!(compute_basis_points(U256::from(10000u64), 10000), U256::from(10000u64));
    }
}
