//! StateWriter implementation for PersistentState.

// U256 doesn't implement AddAssign/SubAssign
#![allow(clippy::assign_op_pattern)]

use tunnels_core::{
    AccumulatorState, Attestation, Challenge, ContributorBalance,
    Denomination, Identity, Project, TransactionDifficultyState, U256,
};
use tunnels_state::StateWriter;

use super::PersistentState;
use crate::keys::StateKey;
use crate::kv::KvBackend;

impl<B: KvBackend> StateWriter for PersistentState<B> {
    fn insert_identity(&mut self, identity: Identity) {
        let addr = identity.address;
        self.identity_cache.insert(addr, identity);
        self.dirty.mark_dirty(StateKey::Identity(addr));
    }

    fn update_identity<F>(&mut self, addr: &[u8; 20], f: F)
    where
        F: FnOnce(&mut Identity),
    {
        if let Some(identity) = self.identity_cache.get_mut(addr) {
            f(identity);
            self.dirty.mark_dirty(StateKey::Identity(*addr));
        }
    }

    fn insert_project(&mut self, project: Project) {
        let id = project.project_id;
        self.project_cache.insert(id, project);
        self.dirty.mark_dirty(StateKey::Project(id));
    }

    fn insert_attestation(&mut self, attestation: Attestation) {
        let id = attestation.attestation_id;
        self.attestation_cache.insert(id, attestation);
        self.dirty.mark_dirty(StateKey::Attestation(id));
    }

    fn update_attestation<F>(&mut self, id: &[u8; 20], f: F)
    where
        F: FnOnce(&mut Attestation),
    {
        if let Some(attestation) = self.attestation_cache.get_mut(id) {
            f(attestation);
            self.dirty.mark_dirty(StateKey::Attestation(*id));
        }
    }

    fn insert_challenge(&mut self, challenge: Challenge) {
        let challenge_id = challenge.challenge_id;
        let attestation_id = challenge.attestation_id;

        self.attestation_challenge_cache.insert(attestation_id, challenge_id);
        self.dirty.mark_dirty(StateKey::AttestationChallenge(attestation_id));

        self.challenge_cache.insert(challenge_id, challenge);
        self.dirty.mark_dirty(StateKey::Challenge(challenge_id));
    }

    fn update_challenge<F>(&mut self, id: &[u8; 20], f: F)
    where
        F: FnOnce(&mut Challenge),
    {
        if let Some(challenge) = self.challenge_cache.get_mut(id) {
            f(challenge);
            self.dirty.mark_dirty(StateKey::Challenge(*id));
        }
    }

    fn get_or_create_accumulator(
        &mut self,
        project_id: &[u8; 20],
        denom: &Denomination,
    ) -> &mut AccumulatorState {
        let key = (*project_id, *denom);
        self.dirty.mark_dirty(StateKey::Accumulator(*project_id, *denom));
        self.accumulator_cache.entry(key).or_default()
    }

    fn update_accumulator<F>(&mut self, project_id: &[u8; 20], denom: &Denomination, f: F)
    where
        F: FnOnce(&mut AccumulatorState),
    {
        if let Some(acc) = self.accumulator_cache.get_mut(&(*project_id, *denom)) {
            f(acc);
            self.dirty.mark_dirty(StateKey::Accumulator(*project_id, *denom));
        }
    }

    fn get_or_create_balance(
        &mut self,
        project_id: &[u8; 20],
        contributor: &[u8; 20],
        denom: &Denomination,
    ) -> &mut ContributorBalance {
        let key = (*project_id, *contributor, *denom);
        self.dirty.mark_dirty(StateKey::Balance(*contributor, *project_id, *denom));
        self.balance_cache.entry(key).or_default()
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
        if let Some(balance) = self.balance_cache.get_mut(&(*project_id, *contributor, *denom)) {
            f(balance);
            self.dirty.mark_dirty(StateKey::Balance(*contributor, *project_id, *denom));
        }
    }

    fn credit_claimable(&mut self, identity: &[u8; 20], denom: &Denomination, amount: U256) {
        let entry = self.claimable_cache.entry((*identity, *denom)).or_insert(U256::zero());
        *entry = *entry + amount;
        self.dirty.mark_dirty(StateKey::Claimable(*identity, *denom));
    }

    fn debit_claimable(&mut self, identity: &[u8; 20], denom: &Denomination, amount: U256) {
        if let Some(entry) = self.claimable_cache.get_mut(&(*identity, *denom)) {
            if *entry >= amount {
                *entry = *entry - amount;
                self.dirty.mark_dirty(StateKey::Claimable(*identity, *denom));
            }
        }
    }

    fn lock_bond(&mut self, attestation_id: &[u8; 20], amount: U256) {
        self.locked_bond_cache.insert(*attestation_id, amount);
        self.dirty.mark_dirty(StateKey::AttestationBond(*attestation_id));
    }

    fn unlock_bond(&mut self, attestation_id: &[u8; 20]) -> Option<U256> {
        let amount = self.locked_bond_cache.remove(attestation_id);
        if amount.is_some() {
            self.dirty.mark_dirty(StateKey::AttestationBond(*attestation_id));
        }
        amount
    }

    fn lock_challenge_bond(&mut self, challenge_id: &[u8; 20], amount: U256) {
        self.challenge_bond_cache.insert(*challenge_id, amount);
        self.dirty.mark_dirty(StateKey::ChallengeBond(*challenge_id));
    }

    fn unlock_challenge_bond(&mut self, challenge_id: &[u8; 20]) -> Option<U256> {
        let amount = self.challenge_bond_cache.remove(challenge_id);
        if amount.is_some() {
            self.dirty.mark_dirty(StateKey::ChallengeBond(*challenge_id));
        }
        amount
    }

    fn update_tx_difficulty_state<F>(&mut self, f: F)
    where
        F: FnOnce(&mut TransactionDifficultyState),
    {
        f(&mut self.tx_difficulty_state);
        self.dirty.mark_dirty(StateKey::TxDifficultyState);
    }
}
