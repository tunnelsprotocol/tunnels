//! State storage traits.
//!
//! These traits abstract over the backing store, enabling Phase 4 to swap
//! in persistent storage without changing the state machine logic.

use tunnels_core::{
    AccumulatorState, Attestation, Challenge, ContributorBalance,
    Denomination, Identity, Project, TransactionDifficultyState, U256,
};

/// Read access to protocol state.
///
/// Methods take `&mut self` to allow implementations to lazily load data
/// from persistent storage into an internal cache on first access.
pub trait StateReader {
    // === Identity Operations ===

    /// Get an identity by address.
    fn get_identity(&mut self, addr: &[u8; 20]) -> Option<&Identity>;

    /// Check if an identity exists.
    fn identity_exists(&mut self, addr: &[u8; 20]) -> bool {
        self.get_identity(addr).is_some()
    }

    // === Project Operations ===

    /// Get a project by ID.
    fn get_project(&mut self, id: &[u8; 20]) -> Option<&Project>;

    /// Check if a project exists.
    fn project_exists(&mut self, id: &[u8; 20]) -> bool {
        self.get_project(id).is_some()
    }

    // === Attestation Operations ===

    /// Get an attestation by ID.
    fn get_attestation(&mut self, id: &[u8; 20]) -> Option<&Attestation>;

    /// Check if an attestation exists.
    fn attestation_exists(&mut self, id: &[u8; 20]) -> bool {
        self.get_attestation(id).is_some()
    }

    // === Challenge Operations ===

    /// Get a challenge by ID.
    fn get_challenge(&mut self, id: &[u8; 20]) -> Option<&Challenge>;

    /// Check if a challenge exists.
    fn challenge_exists(&mut self, id: &[u8; 20]) -> bool {
        self.get_challenge(id).is_some()
    }

    /// Get the challenge for an attestation (if any).
    fn get_challenge_for_attestation(&mut self, attestation_id: &[u8; 20]) -> Option<&Challenge>;

    // === Accumulator Operations ===

    /// Get the accumulator state for a project/denomination pair.
    fn get_accumulator(
        &mut self,
        project_id: &[u8; 20],
        denom: &Denomination,
    ) -> Option<&AccumulatorState>;

    // === Balance Operations ===

    /// Get a contributor's balance for a project/denomination.
    fn get_balance(
        &mut self,
        project_id: &[u8; 20],
        contributor: &[u8; 20],
        denom: &Denomination,
    ) -> Option<&ContributorBalance>;

    /// Get an identity's direct claimable balance (from phi, gamma, bond returns).
    fn get_claimable(&mut self, identity: &[u8; 20], denom: &Denomination) -> U256;

    // === Bond Operations ===

    /// Get the locked bond amount for an attestation.
    fn get_locked_bond(&mut self, attestation_id: &[u8; 20]) -> Option<U256>;

    /// Get the locked challenge bond amount.
    fn get_challenge_bond(&mut self, challenge_id: &[u8; 20]) -> Option<U256>;

    // === Global State ===

    /// Get the transaction difficulty state.
    fn get_tx_difficulty_state(&mut self) -> &TransactionDifficultyState;

    /// Get the current transaction difficulty.
    fn current_tx_difficulty(&mut self) -> u64 {
        self.get_tx_difficulty_state().current_tx_difficulty
    }
}

/// Mutable access to protocol state.
pub trait StateWriter: StateReader {
    // === Identity Mutations ===

    /// Insert a new identity.
    fn insert_identity(&mut self, identity: Identity);

    /// Update an existing identity.
    fn update_identity<F>(&mut self, addr: &[u8; 20], f: F)
    where
        F: FnOnce(&mut Identity);

    // === Project Mutations ===

    /// Insert a new project.
    fn insert_project(&mut self, project: Project);

    // === Attestation Mutations ===

    /// Insert a new attestation.
    fn insert_attestation(&mut self, attestation: Attestation);

    /// Update an existing attestation.
    fn update_attestation<F>(&mut self, id: &[u8; 20], f: F)
    where
        F: FnOnce(&mut Attestation);

    // === Challenge Mutations ===

    /// Insert a new challenge.
    fn insert_challenge(&mut self, challenge: Challenge);

    /// Update an existing challenge.
    fn update_challenge<F>(&mut self, id: &[u8; 20], f: F)
    where
        F: FnOnce(&mut Challenge);

    // === Accumulator Mutations ===

    /// Get or create an accumulator for a project/denomination pair.
    fn get_or_create_accumulator(
        &mut self,
        project_id: &[u8; 20],
        denom: &Denomination,
    ) -> &mut AccumulatorState;

    /// Update an accumulator.
    fn update_accumulator<F>(&mut self, project_id: &[u8; 20], denom: &Denomination, f: F)
    where
        F: FnOnce(&mut AccumulatorState);

    // === Balance Mutations ===

    /// Get or create a contributor balance.
    fn get_or_create_balance(
        &mut self,
        project_id: &[u8; 20],
        contributor: &[u8; 20],
        denom: &Denomination,
    ) -> &mut ContributorBalance;

    /// Update a contributor balance.
    fn update_balance<F>(
        &mut self,
        project_id: &[u8; 20],
        contributor: &[u8; 20],
        denom: &Denomination,
        f: F,
    ) where
        F: FnOnce(&mut ContributorBalance);

    /// Credit claimable balance to an identity.
    fn credit_claimable(&mut self, identity: &[u8; 20], denom: &Denomination, amount: U256);

    /// Debit claimable balance from an identity.
    fn debit_claimable(&mut self, identity: &[u8; 20], denom: &Denomination, amount: U256);

    // === Bond Mutations ===

    /// Lock a bond for an attestation.
    fn lock_bond(&mut self, attestation_id: &[u8; 20], amount: U256);

    /// Unlock and remove a bond for an attestation.
    fn unlock_bond(&mut self, attestation_id: &[u8; 20]) -> Option<U256>;

    /// Lock a challenge bond.
    fn lock_challenge_bond(&mut self, challenge_id: &[u8; 20], amount: U256);

    /// Unlock and remove a challenge bond.
    fn unlock_challenge_bond(&mut self, challenge_id: &[u8; 20]) -> Option<U256>;

    // === Global State Mutations ===

    /// Update the transaction difficulty state.
    fn update_tx_difficulty_state<F>(&mut self, f: F)
    where
        F: FnOnce(&mut TransactionDifficultyState);
}

/// Combined trait for full state access.
///
/// This is a convenience trait that combines both read and write access.
/// Any type implementing both `StateReader` and `StateWriter` automatically
/// implements `StateStore`.
pub trait StateStore: StateReader + StateWriter {}

// Blanket implementation
impl<T: StateReader + StateWriter> StateStore for T {}
