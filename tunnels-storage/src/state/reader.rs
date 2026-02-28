//! StateReader implementation for PersistentState.

use tunnels_core::{
    AccumulatorState, Attestation, Challenge, ContributorBalance,
    Denomination, Identity, Project, TransactionDifficultyState, U256,
    serialization::deserialize,
};
use tunnels_state::StateReader;

use super::PersistentState;
use crate::keys::StateKey;
use crate::kv::KvBackend;

impl<B: KvBackend> StateReader for PersistentState<B> {
    fn get_identity(&mut self, addr: &[u8; 20]) -> Option<&Identity> {
        // Check cache first
        if self.identity_cache.contains_key(addr) {
            return self.identity_cache.get(addr);
        }

        // Load from trie
        let key = StateKey::Identity(*addr).to_bytes();
        if let Ok(Some(bytes)) = self.trie.get(&key) {
            if let Ok(identity) = deserialize::<Identity>(&bytes) {
                self.identity_cache.insert(*addr, identity);
                return self.identity_cache.get(addr);
            }
        }

        None
    }

    fn get_project(&mut self, id: &[u8; 20]) -> Option<&Project> {
        // Check cache first
        if self.project_cache.contains_key(id) {
            return self.project_cache.get(id);
        }

        // Load from trie
        let key = StateKey::Project(*id).to_bytes();
        if let Ok(Some(bytes)) = self.trie.get(&key) {
            if let Ok(project) = deserialize::<Project>(&bytes) {
                self.project_cache.insert(*id, project);
                return self.project_cache.get(id);
            }
        }

        None
    }

    fn get_attestation(&mut self, id: &[u8; 20]) -> Option<&Attestation> {
        // Check cache first
        if self.attestation_cache.contains_key(id) {
            return self.attestation_cache.get(id);
        }

        // Load from trie
        let key = StateKey::Attestation(*id).to_bytes();
        if let Ok(Some(bytes)) = self.trie.get(&key) {
            if let Ok(attestation) = deserialize::<Attestation>(&bytes) {
                self.attestation_cache.insert(*id, attestation);
                return self.attestation_cache.get(id);
            }
        }

        None
    }

    fn get_challenge(&mut self, id: &[u8; 20]) -> Option<&Challenge> {
        // Check cache first
        if self.challenge_cache.contains_key(id) {
            return self.challenge_cache.get(id);
        }

        // Load from trie
        let key = StateKey::Challenge(*id).to_bytes();
        if let Ok(Some(bytes)) = self.trie.get(&key) {
            if let Ok(challenge) = deserialize::<Challenge>(&bytes) {
                self.challenge_cache.insert(*id, challenge);
                return self.challenge_cache.get(id);
            }
        }

        None
    }

    fn get_challenge_for_attestation(&mut self, attestation_id: &[u8; 20]) -> Option<&Challenge> {
        // First, get the challenge_id for this attestation
        let challenge_id = if let Some(&id) = self.attestation_challenge_cache.get(attestation_id) {
            id
        } else {
            // Load from trie
            let key = StateKey::AttestationChallenge(*attestation_id).to_bytes();
            if let Ok(Some(bytes)) = self.trie.get(&key) {
                if let Ok(id) = deserialize::<[u8; 20]>(&bytes) {
                    self.attestation_challenge_cache.insert(*attestation_id, id);
                    id
                } else {
                    return None;
                }
            } else {
                return None;
            }
        };

        // Now get the challenge itself
        self.get_challenge(&challenge_id)
    }

    fn get_accumulator(
        &mut self,
        project_id: &[u8; 20],
        denom: &Denomination,
    ) -> Option<&AccumulatorState> {
        let cache_key = (*project_id, *denom);

        // Check cache first
        if self.accumulator_cache.contains_key(&cache_key) {
            return self.accumulator_cache.get(&cache_key);
        }

        // Load from trie
        let key = StateKey::Accumulator(*project_id, *denom).to_bytes();
        if let Ok(Some(bytes)) = self.trie.get(&key) {
            if let Ok(acc) = deserialize::<AccumulatorState>(&bytes) {
                self.accumulator_cache.insert(cache_key, acc);
                return self.accumulator_cache.get(&cache_key);
            }
        }

        None
    }

    fn get_balance(
        &mut self,
        project_id: &[u8; 20],
        contributor: &[u8; 20],
        denom: &Denomination,
    ) -> Option<&ContributorBalance> {
        let cache_key = (*project_id, *contributor, *denom);

        // Check cache first
        if self.balance_cache.contains_key(&cache_key) {
            return self.balance_cache.get(&cache_key);
        }

        // Load from trie
        let key = StateKey::Balance(*contributor, *project_id, *denom).to_bytes();
        if let Ok(Some(bytes)) = self.trie.get(&key) {
            if let Ok(balance) = deserialize::<ContributorBalance>(&bytes) {
                self.balance_cache.insert(cache_key, balance);
                return self.balance_cache.get(&cache_key);
            }
        }

        None
    }

    fn get_claimable(&mut self, identity: &[u8; 20], denom: &Denomination) -> U256 {
        let cache_key = (*identity, *denom);

        // Check cache first
        if let Some(&amount) = self.claimable_cache.get(&cache_key) {
            return amount;
        }

        // Load from trie
        let key = StateKey::Claimable(*identity, *denom).to_bytes();
        if let Ok(Some(bytes)) = self.trie.get(&key) {
            if let Ok(amount) = deserialize::<U256>(&bytes) {
                self.claimable_cache.insert(cache_key, amount);
                return amount;
            }
        }

        U256::zero()
    }

    fn get_locked_bond(&mut self, attestation_id: &[u8; 20]) -> Option<U256> {
        // Check cache first
        if let Some(&amount) = self.locked_bond_cache.get(attestation_id) {
            return Some(amount);
        }

        // Load from trie
        let key = StateKey::AttestationBond(*attestation_id).to_bytes();
        if let Ok(Some(bytes)) = self.trie.get(&key) {
            if let Ok(amount) = deserialize::<U256>(&bytes) {
                self.locked_bond_cache.insert(*attestation_id, amount);
                return Some(amount);
            }
        }

        None
    }

    fn get_challenge_bond(&mut self, challenge_id: &[u8; 20]) -> Option<U256> {
        // Check cache first
        if let Some(&amount) = self.challenge_bond_cache.get(challenge_id) {
            return Some(amount);
        }

        // Load from trie
        let key = StateKey::ChallengeBond(*challenge_id).to_bytes();
        if let Ok(Some(bytes)) = self.trie.get(&key) {
            if let Ok(amount) = deserialize::<U256>(&bytes) {
                self.challenge_bond_cache.insert(*challenge_id, amount);
                return Some(amount);
            }
        }

        None
    }

    fn get_tx_difficulty_state(&mut self) -> &TransactionDifficultyState {
        &self.tx_difficulty_state
    }
}
