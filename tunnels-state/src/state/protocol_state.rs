//! In-memory protocol state container.

use std::collections::HashMap;

use tunnels_core::{
    AccumulatorState, Attestation, Challenge, ContributorBalance,
    Denomination, Identity, Project, TransactionDifficultyState, U256,
};

use super::store::{StateReader, StateWriter};

/// In-memory protocol state backed by HashMaps.
///
/// This is the testing and development implementation. Phase 4 replaces
/// this with persistent storage implementing the same traits.
#[derive(Clone, Debug)]
pub struct ProtocolState {
    /// All registered identities.
    pub identities: HashMap<[u8; 20], Identity>,

    /// All registered projects.
    pub projects: HashMap<[u8; 20], Project>,

    /// All attestations.
    pub attestations: HashMap<[u8; 20], Attestation>,

    /// All challenges.
    pub challenges: HashMap<[u8; 20], Challenge>,

    /// Mapping from attestation_id to challenge_id for quick lookup.
    pub attestation_challenges: HashMap<[u8; 20], [u8; 20]>,

    /// Accumulator state per (project_id, denomination).
    pub accumulators: HashMap<([u8; 20], Denomination), AccumulatorState>,

    /// Contributor balances per (project_id, contributor, denomination).
    pub balances: HashMap<([u8; 20], [u8; 20], Denomination), ContributorBalance>,

    /// Direct claimable balances per (identity, denomination).
    /// This tracks credits from phi, gamma (identity), and bond returns.
    pub claimable: HashMap<([u8; 20], Denomination), U256>,

    /// Locked attestation bonds by attestation_id.
    pub locked_bonds: HashMap<[u8; 20], U256>,

    /// Locked challenge bonds by challenge_id.
    pub challenge_bonds: HashMap<[u8; 20], U256>,

    /// Protocol-level transaction difficulty state.
    pub tx_difficulty_state: TransactionDifficultyState,
}

impl ProtocolState {
    /// Create a new empty protocol state.
    pub fn new() -> Self {
        Self {
            identities: HashMap::new(),
            projects: HashMap::new(),
            attestations: HashMap::new(),
            challenges: HashMap::new(),
            attestation_challenges: HashMap::new(),
            accumulators: HashMap::new(),
            balances: HashMap::new(),
            claimable: HashMap::new(),
            locked_bonds: HashMap::new(),
            challenge_bonds: HashMap::new(),
            tx_difficulty_state: TransactionDifficultyState::new(8), // Initial difficulty of 8
        }
    }

    /// Create a new protocol state with custom initial difficulty.
    pub fn with_initial_difficulty(initial_difficulty: u64) -> Self {
        Self {
            identities: HashMap::new(),
            projects: HashMap::new(),
            attestations: HashMap::new(),
            challenges: HashMap::new(),
            attestation_challenges: HashMap::new(),
            accumulators: HashMap::new(),
            balances: HashMap::new(),
            claimable: HashMap::new(),
            locked_bonds: HashMap::new(),
            challenge_bonds: HashMap::new(),
            tx_difficulty_state: TransactionDifficultyState::new(initial_difficulty),
        }
    }

    /// Get the number of registered identities.
    pub fn identity_count(&self) -> usize {
        self.identities.len()
    }

    /// Get the number of registered projects.
    pub fn project_count(&self) -> usize {
        self.projects.len()
    }

    /// Get the number of attestations.
    pub fn attestation_count(&self) -> usize {
        self.attestations.len()
    }
}

impl Default for ProtocolState {
    fn default() -> Self {
        Self::new()
    }
}

impl StateReader for ProtocolState {
    fn get_identity(&mut self, addr: &[u8; 20]) -> Option<&Identity> {
        self.identities.get(addr)
    }

    fn get_project(&mut self, id: &[u8; 20]) -> Option<&Project> {
        self.projects.get(id)
    }

    fn get_attestation(&mut self, id: &[u8; 20]) -> Option<&Attestation> {
        self.attestations.get(id)
    }

    fn get_challenge(&mut self, id: &[u8; 20]) -> Option<&Challenge> {
        self.challenges.get(id)
    }

    fn get_challenge_for_attestation(&mut self, attestation_id: &[u8; 20]) -> Option<&Challenge> {
        self.attestation_challenges
            .get(attestation_id)
            .and_then(|challenge_id| self.challenges.get(challenge_id))
    }

    fn get_accumulator(
        &mut self,
        project_id: &[u8; 20],
        denom: &Denomination,
    ) -> Option<&AccumulatorState> {
        self.accumulators.get(&(*project_id, *denom))
    }

    fn get_balance(
        &mut self,
        project_id: &[u8; 20],
        contributor: &[u8; 20],
        denom: &Denomination,
    ) -> Option<&ContributorBalance> {
        self.balances.get(&(*project_id, *contributor, *denom))
    }

    fn get_claimable(&mut self, identity: &[u8; 20], denom: &Denomination) -> U256 {
        self.claimable
            .get(&(*identity, *denom))
            .copied()
            .unwrap_or_else(U256::zero)
    }

    fn get_locked_bond(&mut self, attestation_id: &[u8; 20]) -> Option<U256> {
        self.locked_bonds.get(attestation_id).copied()
    }

    fn get_challenge_bond(&mut self, challenge_id: &[u8; 20]) -> Option<U256> {
        self.challenge_bonds.get(challenge_id).copied()
    }

    fn get_tx_difficulty_state(&mut self) -> &TransactionDifficultyState {
        &self.tx_difficulty_state
    }
}

impl StateWriter for ProtocolState {
    fn insert_identity(&mut self, identity: Identity) {
        self.identities.insert(identity.address, identity);
    }

    fn update_identity<F>(&mut self, addr: &[u8; 20], f: F)
    where
        F: FnOnce(&mut Identity),
    {
        if let Some(identity) = self.identities.get_mut(addr) {
            f(identity);
        }
    }

    fn insert_project(&mut self, project: Project) {
        self.projects.insert(project.project_id, project);
    }

    fn insert_attestation(&mut self, attestation: Attestation) {
        self.attestations
            .insert(attestation.attestation_id, attestation);
    }

    fn update_attestation<F>(&mut self, id: &[u8; 20], f: F)
    where
        F: FnOnce(&mut Attestation),
    {
        if let Some(attestation) = self.attestations.get_mut(id) {
            f(attestation);
        }
    }

    fn insert_challenge(&mut self, challenge: Challenge) {
        // Track the mapping from attestation to challenge
        self.attestation_challenges
            .insert(challenge.attestation_id, challenge.challenge_id);
        self.challenges.insert(challenge.challenge_id, challenge);
    }

    fn update_challenge<F>(&mut self, id: &[u8; 20], f: F)
    where
        F: FnOnce(&mut Challenge),
    {
        if let Some(challenge) = self.challenges.get_mut(id) {
            f(challenge);
        }
    }

    fn get_or_create_accumulator(
        &mut self,
        project_id: &[u8; 20],
        denom: &Denomination,
    ) -> &mut AccumulatorState {
        self.accumulators
            .entry((*project_id, *denom))
            .or_default()
    }

    fn update_accumulator<F>(&mut self, project_id: &[u8; 20], denom: &Denomination, f: F)
    where
        F: FnOnce(&mut AccumulatorState),
    {
        if let Some(acc) = self.accumulators.get_mut(&(*project_id, *denom)) {
            f(acc);
        }
    }

    fn get_or_create_balance(
        &mut self,
        project_id: &[u8; 20],
        contributor: &[u8; 20],
        denom: &Denomination,
    ) -> &mut ContributorBalance {
        self.balances
            .entry((*project_id, *contributor, *denom))
            .or_default()
    }

    fn update_balance<F>(
        &mut self,
        project_id: &[u8; 20],
        contributor: &[u8; 20],
        denom: &Denomination,
        f: F,
    ) where
        F: FnOnce(&mut ContributorBalance),
    {
        if let Some(balance) = self.balances.get_mut(&(*project_id, *contributor, *denom)) {
            f(balance);
        }
    }

    fn credit_claimable(&mut self, identity: &[u8; 20], denom: &Denomination, amount: U256) {
        let entry = self.claimable.entry((*identity, *denom)).or_insert(U256::zero());
        *entry = *entry + amount;
    }

    fn debit_claimable(&mut self, identity: &[u8; 20], denom: &Denomination, amount: U256) {
        if let Some(entry) = self.claimable.get_mut(&(*identity, *denom)) {
            if *entry >= amount {
                *entry = *entry - amount;
            }
        }
    }

    fn lock_bond(&mut self, attestation_id: &[u8; 20], amount: U256) {
        self.locked_bonds.insert(*attestation_id, amount);
    }

    fn unlock_bond(&mut self, attestation_id: &[u8; 20]) -> Option<U256> {
        self.locked_bonds.remove(attestation_id)
    }

    fn lock_challenge_bond(&mut self, challenge_id: &[u8; 20], amount: U256) {
        self.challenge_bonds.insert(*challenge_id, amount);
    }

    fn unlock_challenge_bond(&mut self, challenge_id: &[u8; 20]) -> Option<U256> {
        self.challenge_bonds.remove(challenge_id)
    }

    fn update_tx_difficulty_state<F>(&mut self, f: F)
    where
        F: FnOnce(&mut TransactionDifficultyState),
    {
        f(&mut self.tx_difficulty_state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tunnels_core::crypto::derive_address;
    use tunnels_core::KeyPair;

    fn create_test_identity() -> Identity {
        let kp = KeyPair::generate();
        let address = derive_address(&kp.public_key());
        Identity::new_human(address, kp.public_key())
    }

    #[test]
    fn test_new_state() {
        let state = ProtocolState::new();
        assert_eq!(state.identity_count(), 0);
        assert_eq!(state.project_count(), 0);
        assert_eq!(state.attestation_count(), 0);
    }

    #[test]
    fn test_insert_and_get_identity() {
        let mut state = ProtocolState::new();
        let identity = create_test_identity();
        let address = identity.address;

        assert!(!state.identity_exists(&address));
        state.insert_identity(identity.clone());
        assert!(state.identity_exists(&address));

        let retrieved = state.get_identity(&address).unwrap();
        assert_eq!(retrieved.address, address);
    }

    #[test]
    fn test_update_identity() {
        let mut state = ProtocolState::new();
        let identity = create_test_identity();
        let address = identity.address;

        state.insert_identity(identity);
        assert!(state.get_identity(&address).unwrap().active);

        state.update_identity(&address, |id| {
            id.active = false;
        });

        assert!(!state.get_identity(&address).unwrap().active);
    }

    #[test]
    fn test_claimable_balance() {
        let mut state = ProtocolState::new();
        let identity = [1u8; 20];
        let denom = *b"USD\0\0\0\0\0";

        assert_eq!(state.get_claimable(&identity, &denom), U256::zero());

        state.credit_claimable(&identity, &denom, U256::from(1000u64));
        assert_eq!(state.get_claimable(&identity, &denom), U256::from(1000u64));

        state.credit_claimable(&identity, &denom, U256::from(500u64));
        assert_eq!(state.get_claimable(&identity, &denom), U256::from(1500u64));

        state.debit_claimable(&identity, &denom, U256::from(300u64));
        assert_eq!(state.get_claimable(&identity, &denom), U256::from(1200u64));
    }

    #[test]
    fn test_bond_locking() {
        let mut state = ProtocolState::new();
        let attestation_id = [1u8; 20];
        let amount = U256::from(1000u64);

        assert!(state.get_locked_bond(&attestation_id).is_none());

        state.lock_bond(&attestation_id, amount);
        assert_eq!(state.get_locked_bond(&attestation_id), Some(amount));

        let unlocked = state.unlock_bond(&attestation_id);
        assert_eq!(unlocked, Some(amount));
        assert!(state.get_locked_bond(&attestation_id).is_none());
    }

    #[test]
    fn test_accumulator_get_or_create() {
        let mut state = ProtocolState::new();
        let project_id = [1u8; 20];
        let denom = *b"USD\0\0\0\0\0";

        assert!(state.get_accumulator(&project_id, &denom).is_none());

        let acc = state.get_or_create_accumulator(&project_id, &denom);
        assert_eq!(acc.total_finalized_units, 0);
        assert!(acc.reward_per_unit_global.is_zero());

        // Modify it
        acc.total_finalized_units = 100;

        // Get again and verify modification persisted
        let acc2 = state.get_accumulator(&project_id, &denom).unwrap();
        assert_eq!(acc2.total_finalized_units, 100);
    }

    #[test]
    fn test_balance_get_or_create() {
        let mut state = ProtocolState::new();
        let project_id = [1u8; 20];
        let contributor = [2u8; 20];
        let denom = *b"USD\0\0\0\0\0";

        assert!(state.get_balance(&project_id, &contributor, &denom).is_none());

        let balance = state.get_or_create_balance(&project_id, &contributor, &denom);
        assert_eq!(balance.units, 0);

        balance.units = 50;

        let balance2 = state.get_balance(&project_id, &contributor, &denom).unwrap();
        assert_eq!(balance2.units, 50);
    }
}
