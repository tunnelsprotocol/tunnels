//! Accumulator math operations.
//!
//! Implements O(1) revenue distribution using a reward-per-unit index
//! with 128.128 fixed-point arithmetic.
//!
//! The key insight is that we track a global "reward per unit" index that
//! increments with each deposit. Contributors snapshot this index when they
//! join. Their claimable amount is: units × (current_index - entry_index).

use tunnels_core::{AttestationStatus, Denomination, U256};

use crate::error::{StateError, StateResult};
use crate::state::StateWriter;

/// Fixed-point scale factor: 2^128.
/// We use 128.128 fixed-point to maintain precision across divisions.
const FIXED_POINT_SCALE: U256 = U256([0, 0, 1, 0]); // 2^128

/// Finalize attestation units and update the accumulator.
///
/// Called when an attestation is successfully finalized (either by
/// FinalizeAttestation after the verification window, or by
/// ResolveChallenge with Valid ruling).
///
/// # Algorithm
///
/// 1. If this is the first finalization (total_units was 0):
///    - Distribute any unallocated balance to these initial units
///    - reward_per_unit_global += unallocated / new_units (in fixed-point)
///    - unallocated = 0
///
/// 2. Snapshot any pending rewards for the contributor before updating:
///    - pending = units × (global - entry)
///    - This is implicitly handled by keeping entry at global when adding units
///
/// 3. Update contributor balance:
///    - contributor.units += new_units
///    - contributor.entry = global (snapshot current index)
///
/// 4. Update accumulator:
///    - total_finalized_units += new_units
///    - Update attestation status to Finalized
pub fn finalize_attestation_units<S: StateWriter>(
    state: &mut S,
    attestation_id: &[u8; 20],
    project_id: &[u8; 20],
    contributor: &[u8; 20],
    units: u64,
    denom: &Denomination,
    timestamp: u64,
) -> StateResult<()> {
    // Get or create the accumulator
    let acc = state.get_or_create_accumulator(project_id, denom);

    // Check if this is the first finalization with pending unallocated funds
    let is_first_finalization = acc.total_finalized_units == 0;

    // Capture the entry point for new contributors BEFORE distributing unallocated.
    // This ensures first finalization contributors can claim their share.
    let entry_point_for_new_contributors = acc.reward_per_unit_global;

    if is_first_finalization && !acc.unallocated.is_zero() {
        // Distribute unallocated to the new units
        // reward_per_unit_global += (unallocated << 128) / new_units
        let unallocated_scaled = acc.unallocated * FIXED_POINT_SCALE;
        let reward_delta = unallocated_scaled / U256::from(units);
        acc.reward_per_unit_global = acc.reward_per_unit_global + reward_delta;
        acc.unallocated = U256::zero();
    }

    // Current global after any unallocated distribution
    let current_global = acc.reward_per_unit_global;

    // Update total units in accumulator
    acc.total_finalized_units += units;

    // Get or create contributor balance
    let balance = state.get_or_create_balance(project_id, contributor, denom);
    let had_units = balance.units > 0;

    // If contributor already has units, snapshot pending rewards before update
    // This is handled implicitly: when we set entry = global, we "claim" the
    // pending rewards into the calculation.
    //
    // For existing contributors with units > 0:
    //   pending = units × (global - entry)
    //   We credit this to their claimable balance.
    if had_units {
        let pending = compute_pending_reward(balance.units, current_global, balance.reward_per_unit_at_entry);
        if !pending.is_zero() {
            state.credit_claimable(contributor, denom, pending);
        }
    }

    // Now get the balance again to update it (needed because we moved state)
    let balance = state.get_or_create_balance(project_id, contributor, denom);
    balance.units += units;

    // New contributors get entry at the point BEFORE unallocated distribution.
    // Existing contributors get entry at current global (after snapshotting pending).
    balance.reward_per_unit_at_entry = if had_units {
        current_global
    } else {
        entry_point_for_new_contributors
    };

    // Update attestation status
    state.update_attestation(attestation_id, |att| {
        att.status = AttestationStatus::Finalized;
        att.finalized_at = Some(timestamp);
    });

    Ok(())
}

/// Compute pending reward for a contributor.
///
/// pending = units × (global - entry) / SCALE
fn compute_pending_reward(units: u64, global: U256, entry: U256) -> U256 {
    if global <= entry {
        return U256::zero();
    }
    let delta = global - entry;
    let reward_scaled = U256::from(units) * delta;
    // Convert from fixed-point back to regular amount
    reward_scaled / FIXED_POINT_SCALE
}

/// Compute claimable revenue for a contributor in a project.
///
/// Returns the total amount claimable from both:
/// 1. Pending rewards: units × (global - entry)
/// 2. Previously snapshotted rewards (in claimable balance)
///
/// Note: This does NOT include direct claimable from phi/gamma/bonds,
/// which are tracked separately.
pub fn compute_claimable_revenue<S: StateWriter>(
    state: &mut S,
    project_id: &[u8; 20],
    contributor: &[u8; 20],
    denom: &Denomination,
) -> StateResult<U256> {
    // Get accumulator
    let acc = match state.get_accumulator(project_id, denom) {
        Some(acc) => acc,
        None => return Ok(U256::zero()),
    };

    let current_global = acc.reward_per_unit_global;

    // Get contributor balance
    let balance = match state.get_balance(project_id, contributor, denom) {
        Some(b) => b,
        None => return Ok(U256::zero()),
    };

    if balance.units == 0 {
        return Ok(U256::zero());
    }

    // Compute pending
    let pending = compute_pending_reward(balance.units, current_global, balance.reward_per_unit_at_entry);

    // Total claimable = pending - already_claimed
    // But claimed is tracked separately, so we just return pending
    // The caller should subtract `balance.claimed` if needed
    if pending <= balance.claimed {
        Ok(U256::zero())
    } else {
        Ok(pending - balance.claimed)
    }
}

/// Distribute revenue to the labor pool.
///
/// Called by the waterfall router for the remainder after all slices.
///
/// If there are finalized units, update the global index.
/// If no finalized units yet, add to unallocated.
pub fn distribute_to_labor_pool<S: StateWriter>(
    state: &mut S,
    project_id: &[u8; 20],
    amount: U256,
    denom: &Denomination,
) -> StateResult<()> {
    if amount.is_zero() {
        return Ok(());
    }

    let acc = state.get_or_create_accumulator(project_id, denom);

    if acc.total_finalized_units == 0 {
        // No contributors yet - hold as unallocated
        acc.unallocated = acc.unallocated + amount;
    } else {
        // Distribute to existing contributors via index update
        // reward_per_unit_global += (amount << 128) / total_units
        let amount_scaled = amount * FIXED_POINT_SCALE;
        let reward_delta = amount_scaled / U256::from(acc.total_finalized_units);
        acc.reward_per_unit_global = acc.reward_per_unit_global + reward_delta;
    }

    Ok(())
}

/// Claim revenue from a project's labor pool.
///
/// Updates the contributor's claimed amount and returns the net claimable.
pub fn claim_labor_pool_revenue<S: StateWriter>(
    state: &mut S,
    project_id: &[u8; 20],
    contributor: &[u8; 20],
    denom: &Denomination,
) -> StateResult<U256> {
    // Compute current claimable
    let claimable = compute_claimable_revenue(state, project_id, contributor, denom)?;

    if claimable.is_zero() {
        return Err(StateError::NothingToClaim {
            project_id: *project_id,
            contributor: *contributor,
        });
    }

    // Update contributor's claimed amount
    state.update_balance(project_id, contributor, denom, |balance| {
        balance.claimed = balance.claimed + claimable;
    });

    Ok(claimable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{ProtocolState, StateReader, StateWriter};
    use tunnels_core::{Attestation, AttestationStatus};

    fn test_denom() -> Denomination {
        *b"USD\0\0\0\0\0"
    }

    fn create_pending_attestation(
        state: &mut ProtocolState,
        attestation_id: [u8; 20],
        project_id: [u8; 20],
        contributor: [u8; 20],
        units: u64,
    ) {
        let attestation = Attestation {
            attestation_id,
            project_id,
            contributor,
            units,
            evidence_hash: [0u8; 32],
            bond_poster: contributor,
            bond_amount: U256::from(100u64),
            bond_denomination: test_denom(),
            status: AttestationStatus::Pending,
            created_at: 1000,
            finalized_at: None,
        };
        state.insert_attestation(attestation);
    }

    #[test]
    fn test_finalize_first_attestation() {
        let mut state = ProtocolState::new();
        let project_id = [1u8; 20];
        let contributor = [2u8; 20];
        let attestation_id = [3u8; 20];
        let denom = test_denom();

        create_pending_attestation(&mut state, attestation_id, project_id, contributor, 100);

        finalize_attestation_units(
            &mut state,
            &attestation_id,
            &project_id,
            &contributor,
            100,
            &denom,
            2000,
        )
        .unwrap();

        // Check accumulator
        let acc = state.get_accumulator(&project_id, &denom).unwrap();
        assert_eq!(acc.total_finalized_units, 100);

        // Check contributor balance
        let balance = state.get_balance(&project_id, &contributor, &denom).unwrap();
        assert_eq!(balance.units, 100);

        // Check attestation status
        let att = state.get_attestation(&attestation_id).unwrap();
        assert_eq!(att.status, AttestationStatus::Finalized);
        assert_eq!(att.finalized_at, Some(2000));
    }

    #[test]
    fn test_distribute_before_finalization() {
        let mut state = ProtocolState::new();
        let project_id = [1u8; 20];
        let denom = test_denom();

        // Deposit before any attestations finalize
        distribute_to_labor_pool(&mut state, &project_id, U256::from(1000u64), &denom).unwrap();

        // Check unallocated
        let acc = state.get_accumulator(&project_id, &denom).unwrap();
        assert_eq!(acc.unallocated, U256::from(1000u64));
        assert_eq!(acc.total_finalized_units, 0);
    }

    #[test]
    fn test_unallocated_distributed_on_first_finalization() {
        let mut state = ProtocolState::new();
        let project_id = [1u8; 20];
        let contributor = [2u8; 20];
        let attestation_id = [3u8; 20];
        let denom = test_denom();

        // Deposit 1000 before any attestations
        distribute_to_labor_pool(&mut state, &project_id, U256::from(1000u64), &denom).unwrap();

        // Now finalize 100 units
        create_pending_attestation(&mut state, attestation_id, project_id, contributor, 100);
        finalize_attestation_units(
            &mut state,
            &attestation_id,
            &project_id,
            &contributor,
            100,
            &denom,
            2000,
        )
        .unwrap();

        // Unallocated should be cleared
        let acc = state.get_accumulator(&project_id, &denom).unwrap();
        assert_eq!(acc.unallocated, U256::zero());

        // Contributor should be able to claim the 1000
        let claimable = compute_claimable_revenue(&mut state, &project_id, &contributor, &denom).unwrap();
        assert_eq!(claimable, U256::from(1000u64));
    }

    #[test]
    fn test_distribute_after_finalization() {
        let mut state = ProtocolState::new();
        let project_id = [1u8; 20];
        let contributor = [2u8; 20];
        let attestation_id = [3u8; 20];
        let denom = test_denom();

        // Finalize 100 units first
        create_pending_attestation(&mut state, attestation_id, project_id, contributor, 100);
        finalize_attestation_units(
            &mut state,
            &attestation_id,
            &project_id,
            &contributor,
            100,
            &denom,
            2000,
        )
        .unwrap();

        // Now deposit 1000
        distribute_to_labor_pool(&mut state, &project_id, U256::from(1000u64), &denom).unwrap();

        // Check that index increased
        let acc = state.get_accumulator(&project_id, &denom).unwrap();
        assert_eq!(acc.unallocated, U256::zero());
        assert!(acc.reward_per_unit_global > U256::zero());

        // Contributor should be able to claim 1000
        let claimable = compute_claimable_revenue(&mut state, &project_id, &contributor, &denom).unwrap();
        assert_eq!(claimable, U256::from(1000u64));
    }

    #[test]
    fn test_two_contributors_equal_units() {
        let mut state = ProtocolState::new();
        let project_id = [1u8; 20];
        let contributor_a = [2u8; 20];
        let contributor_b = [3u8; 20];
        let attestation_a = [4u8; 20];
        let attestation_b = [5u8; 20];
        let denom = test_denom();

        // A finalizes 100 units
        create_pending_attestation(&mut state, attestation_a, project_id, contributor_a, 100);
        finalize_attestation_units(
            &mut state,
            &attestation_a,
            &project_id,
            &contributor_a,
            100,
            &denom,
            1000,
        )
        .unwrap();

        // B finalizes 100 units
        create_pending_attestation(&mut state, attestation_b, project_id, contributor_b, 100);
        finalize_attestation_units(
            &mut state,
            &attestation_b,
            &project_id,
            &contributor_b,
            100,
            &denom,
            2000,
        )
        .unwrap();

        // Deposit 2000
        distribute_to_labor_pool(&mut state, &project_id, U256::from(2000u64), &denom).unwrap();

        // Each should get 1000 (equal share)
        let claimable_a = compute_claimable_revenue(&mut state, &project_id, &contributor_a, &denom).unwrap();
        let claimable_b = compute_claimable_revenue(&mut state, &project_id, &contributor_b, &denom).unwrap();

        assert_eq!(claimable_a, U256::from(1000u64));
        assert_eq!(claimable_b, U256::from(1000u64));
    }

    #[test]
    fn test_two_contributors_unequal_units() {
        let mut state = ProtocolState::new();
        let project_id = [1u8; 20];
        let contributor_a = [2u8; 20];
        let contributor_b = [3u8; 20];
        let attestation_a = [4u8; 20];
        let attestation_b = [5u8; 20];
        let denom = test_denom();

        // A finalizes 100 units
        create_pending_attestation(&mut state, attestation_a, project_id, contributor_a, 100);
        finalize_attestation_units(
            &mut state,
            &attestation_a,
            &project_id,
            &contributor_a,
            100,
            &denom,
            1000,
        )
        .unwrap();

        // B finalizes 200 units
        create_pending_attestation(&mut state, attestation_b, project_id, contributor_b, 200);
        finalize_attestation_units(
            &mut state,
            &attestation_b,
            &project_id,
            &contributor_b,
            200,
            &denom,
            2000,
        )
        .unwrap();

        // Deposit 3000 (should split 1000/2000)
        distribute_to_labor_pool(&mut state, &project_id, U256::from(3000u64), &denom).unwrap();

        let claimable_a = compute_claimable_revenue(&mut state, &project_id, &contributor_a, &denom).unwrap();
        let claimable_b = compute_claimable_revenue(&mut state, &project_id, &contributor_b, &denom).unwrap();

        // A: 100/300 × 3000 = 1000
        // B: 200/300 × 3000 = 2000
        assert_eq!(claimable_a, U256::from(1000u64));
        assert_eq!(claimable_b, U256::from(2000u64));
    }

    #[test]
    fn test_staggered_contributions() {
        // Acceptance test #10: A joins, deposit, B joins, deposit
        let mut state = ProtocolState::new();
        let project_id = [1u8; 20];
        let contributor_a = [2u8; 20];
        let contributor_b = [3u8; 20];
        let attestation_a = [4u8; 20];
        let attestation_b = [5u8; 20];
        let denom = test_denom();

        // A finalizes 100 units
        create_pending_attestation(&mut state, attestation_a, project_id, contributor_a, 100);
        finalize_attestation_units(
            &mut state,
            &attestation_a,
            &project_id,
            &contributor_a,
            100,
            &denom,
            1000,
        )
        .unwrap();

        // First deposit: 1000 (only A exists)
        distribute_to_labor_pool(&mut state, &project_id, U256::from(1000u64), &denom).unwrap();

        // B joins with 100 units
        create_pending_attestation(&mut state, attestation_b, project_id, contributor_b, 100);
        finalize_attestation_units(
            &mut state,
            &attestation_b,
            &project_id,
            &contributor_b,
            100,
            &denom,
            2000,
        )
        .unwrap();

        // Second deposit: 1000 (A and B both exist, equal units)
        distribute_to_labor_pool(&mut state, &project_id, U256::from(1000u64), &denom).unwrap();

        let claimable_a = compute_claimable_revenue(&mut state, &project_id, &contributor_a, &denom).unwrap();
        let claimable_b = compute_claimable_revenue(&mut state, &project_id, &contributor_b, &denom).unwrap();

        // A: 1000 (from first) + 500 (from second) = 1500
        // B: 0 (from first) + 500 (from second) = 500
        assert_eq!(claimable_a, U256::from(1500u64));
        assert_eq!(claimable_b, U256::from(500u64));
    }

    #[test]
    fn test_claim_updates_balance() {
        let mut state = ProtocolState::new();
        let project_id = [1u8; 20];
        let contributor = [2u8; 20];
        let attestation_id = [3u8; 20];
        let denom = test_denom();

        // Setup: finalize and deposit
        create_pending_attestation(&mut state, attestation_id, project_id, contributor, 100);
        finalize_attestation_units(
            &mut state,
            &attestation_id,
            &project_id,
            &contributor,
            100,
            &denom,
            1000,
        )
        .unwrap();
        distribute_to_labor_pool(&mut state, &project_id, U256::from(1000u64), &denom).unwrap();

        // Claim
        let claimed = claim_labor_pool_revenue(&mut state, &project_id, &contributor, &denom).unwrap();
        assert_eq!(claimed, U256::from(1000u64));

        // Check balance updated
        let balance = state.get_balance(&project_id, &contributor, &denom).unwrap();
        assert_eq!(balance.claimed, U256::from(1000u64));

        // Second claim should fail (nothing left)
        let result = claim_labor_pool_revenue(&mut state, &project_id, &contributor, &denom);
        assert!(matches!(result, Err(StateError::NothingToClaim { .. })));
    }

    #[test]
    fn test_multi_denomination() {
        // Acceptance test #12: USD and EUR independent
        let mut state = ProtocolState::new();
        let project_id = [1u8; 20];
        let contributor = [2u8; 20];
        let attestation_id = [3u8; 20];
        let denom_usd = *b"USD\0\0\0\0\0";
        let denom_eur = *b"EUR\0\0\0\0\0";

        // Finalize in both denominations
        create_pending_attestation(&mut state, attestation_id, project_id, contributor, 100);

        // Finalize for USD
        finalize_attestation_units(
            &mut state,
            &attestation_id,
            &project_id,
            &contributor,
            100,
            &denom_usd,
            1000,
        )
        .unwrap();

        // Finalize for EUR (same attestation can finalize in multiple denoms)
        // Actually, attestation is already finalized, but we can still update the accumulator
        // In practice, units are per-attestation, but accumulators are per-denom
        // Let's create another attestation for EUR
        let attestation_eur = [4u8; 20];
        create_pending_attestation(&mut state, attestation_eur, project_id, contributor, 100);
        finalize_attestation_units(
            &mut state,
            &attestation_eur,
            &project_id,
            &contributor,
            100,
            &denom_eur,
            1000,
        )
        .unwrap();

        // Deposit to each
        distribute_to_labor_pool(&mut state, &project_id, U256::from(1000u64), &denom_usd).unwrap();
        distribute_to_labor_pool(&mut state, &project_id, U256::from(500u64), &denom_eur).unwrap();

        // Check independent
        let claimable_usd = compute_claimable_revenue(&mut state, &project_id, &contributor, &denom_usd).unwrap();
        let claimable_eur = compute_claimable_revenue(&mut state, &project_id, &contributor, &denom_eur).unwrap();

        assert_eq!(claimable_usd, U256::from(1000u64));
        assert_eq!(claimable_eur, U256::from(500u64));
    }
}
