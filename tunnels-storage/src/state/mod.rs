//! Persistent state implementation.
//!
//! This module provides `PersistentState`, which implements the `StateReader`
//! and `StateWriter` traits from `tunnels-state` with disk-backed storage
//! and Merkle Patricia Trie commitments.

mod buffer;
mod reader;
mod writer;

use std::collections::HashMap;
use std::sync::Arc;

use tunnels_core::{
    AccumulatorState, Attestation, Challenge, ContributorBalance,
    Denomination, Identity, Project, TransactionDifficultyState, U256,
    serialization::{serialize, deserialize},
};

use crate::error::StorageError;
use crate::keys::StateKey;
use crate::kv::KvBackend;
use crate::trie::MerkleTrie;

pub use buffer::WriteBuffer;

/// Persistent state backed by a key-value store with Merkle Patricia Trie commitments.
///
/// This type implements `StateReader` and `StateWriter` from `tunnels-state`,
/// allowing it to be used as a drop-in replacement for the in-memory `ProtocolState`.
///
/// ## Usage
///
/// ```ignore
/// let backend = Arc::new(RocksBackend::open("state.db")?);
/// let mut state = PersistentState::new(backend);
///
/// // Use like ProtocolState
/// state.insert_identity(identity);
///
/// // Commit changes and get state root
/// let root = state.commit()?;
/// ```
pub struct PersistentState<B: KvBackend> {
    /// The underlying KV backend (shared with trie).
    backend: Arc<B>,
    /// The Merkle Patricia Trie.
    trie: MerkleTrie<B>,

    // === Caches for mutable reference returns ===

    /// Identity cache.
    pub(crate) identity_cache: HashMap<[u8; 20], Identity>,
    /// Project cache.
    pub(crate) project_cache: HashMap<[u8; 20], Project>,
    /// Attestation cache.
    pub(crate) attestation_cache: HashMap<[u8; 20], Attestation>,
    /// Challenge cache.
    pub(crate) challenge_cache: HashMap<[u8; 20], Challenge>,
    /// Attestation to challenge mapping cache.
    pub(crate) attestation_challenge_cache: HashMap<[u8; 20], [u8; 20]>,
    /// Accumulator cache: (project_id, denom) -> AccumulatorState.
    pub(crate) accumulator_cache: HashMap<([u8; 20], Denomination), AccumulatorState>,
    /// Balance cache: (project_id, contributor, denom) -> ContributorBalance.
    pub(crate) balance_cache: HashMap<([u8; 20], [u8; 20], Denomination), ContributorBalance>,
    /// Claimable balance cache: (identity, denom) -> U256.
    pub(crate) claimable_cache: HashMap<([u8; 20], Denomination), U256>,
    /// Locked attestation bonds.
    pub(crate) locked_bond_cache: HashMap<[u8; 20], U256>,
    /// Locked challenge bonds.
    pub(crate) challenge_bond_cache: HashMap<[u8; 20], U256>,

    // === Protocol state (always loaded) ===

    /// Protocol transaction difficulty state.
    pub(crate) tx_difficulty_state: TransactionDifficultyState,

    // === Tracking ===

    /// Dirty entries tracker.
    pub(crate) dirty: WriteBuffer,
}

impl<B: KvBackend> PersistentState<B> {
    /// Create a new empty persistent state.
    pub fn new(backend: Arc<B>) -> Self {
        Self {
            trie: MerkleTrie::new(Arc::clone(&backend)),
            backend,
            identity_cache: HashMap::new(),
            project_cache: HashMap::new(),
            attestation_cache: HashMap::new(),
            challenge_cache: HashMap::new(),
            attestation_challenge_cache: HashMap::new(),
            accumulator_cache: HashMap::new(),
            balance_cache: HashMap::new(),
            claimable_cache: HashMap::new(),
            locked_bond_cache: HashMap::new(),
            challenge_bond_cache: HashMap::new(),
            tx_difficulty_state: TransactionDifficultyState::new(8),
            dirty: WriteBuffer::new(),
        }
    }

    /// Create a persistent state from an existing root.
    pub fn from_root(backend: Arc<B>, root: [u8; 32]) -> Result<Self, StorageError> {
        let trie = MerkleTrie::from_root(Arc::clone(&backend), root);

        // Load protocol state from trie
        let tx_difficulty_state = if let Some(bytes) = trie.get(&StateKey::TxDifficultyState.to_bytes())? {
            deserialize(&bytes)?
        } else {
            TransactionDifficultyState::new(8)
        };

        Ok(Self {
            trie,
            backend,
            identity_cache: HashMap::new(),
            project_cache: HashMap::new(),
            attestation_cache: HashMap::new(),
            challenge_cache: HashMap::new(),
            attestation_challenge_cache: HashMap::new(),
            accumulator_cache: HashMap::new(),
            balance_cache: HashMap::new(),
            claimable_cache: HashMap::new(),
            locked_bond_cache: HashMap::new(),
            challenge_bond_cache: HashMap::new(),
            tx_difficulty_state,
            dirty: WriteBuffer::new(),
        })
    }

    /// Get the backend.
    pub fn backend(&self) -> &Arc<B> {
        &self.backend
    }

    /// Get the current state root.
    pub fn state_root(&self) -> [u8; 32] {
        self.trie.root_hash()
    }

    /// Check if the state is empty.
    pub fn is_empty(&self) -> bool {
        self.trie.is_empty()
    }

    /// Prepare for a new block by clearing caches.
    ///
    /// Call this before processing transactions in a new block.
    pub fn begin_block(&mut self) {
        // Caches are kept between blocks for efficiency.
        // Only the dirty tracker is cleared at commit time.
    }

    /// Load state entries from the trie into cache.
    ///
    /// This is used when replaying or when random access to state is needed.
    pub fn load_identity(&mut self, addr: &[u8; 20]) -> Result<Option<&Identity>, StorageError> {
        if self.identity_cache.contains_key(addr) {
            return Ok(self.identity_cache.get(addr));
        }

        let key = StateKey::Identity(*addr).to_bytes();
        if let Some(bytes) = self.trie.get(&key)? {
            let identity: Identity = deserialize(&bytes)?;
            self.identity_cache.insert(*addr, identity);
            Ok(self.identity_cache.get(addr))
        } else {
            Ok(None)
        }
    }

    /// Load a project from the trie into cache.
    pub fn load_project(&mut self, id: &[u8; 20]) -> Result<Option<&Project>, StorageError> {
        if self.project_cache.contains_key(id) {
            return Ok(self.project_cache.get(id));
        }

        let key = StateKey::Project(*id).to_bytes();
        if let Some(bytes) = self.trie.get(&key)? {
            let project: Project = deserialize(&bytes)?;
            self.project_cache.insert(*id, project);
            Ok(self.project_cache.get(id))
        } else {
            Ok(None)
        }
    }

    /// Load an attestation from the trie into cache.
    pub fn load_attestation(&mut self, id: &[u8; 20]) -> Result<Option<&Attestation>, StorageError> {
        if self.attestation_cache.contains_key(id) {
            return Ok(self.attestation_cache.get(id));
        }

        let key = StateKey::Attestation(*id).to_bytes();
        if let Some(bytes) = self.trie.get(&key)? {
            let attestation: Attestation = deserialize(&bytes)?;
            self.attestation_cache.insert(*id, attestation);
            Ok(self.attestation_cache.get(id))
        } else {
            Ok(None)
        }
    }

    /// Load a challenge from the trie into cache.
    pub fn load_challenge(&mut self, id: &[u8; 20]) -> Result<Option<&Challenge>, StorageError> {
        if self.challenge_cache.contains_key(id) {
            return Ok(self.challenge_cache.get(id));
        }

        let key = StateKey::Challenge(*id).to_bytes();
        if let Some(bytes) = self.trie.get(&key)? {
            let challenge: Challenge = deserialize(&bytes)?;
            self.challenge_cache.insert(*id, challenge);
            Ok(self.challenge_cache.get(id))
        } else {
            Ok(None)
        }
    }

    /// Load an accumulator from the trie into cache.
    pub fn load_accumulator(
        &mut self,
        project_id: &[u8; 20],
        denom: &Denomination,
    ) -> Result<Option<&AccumulatorState>, StorageError> {
        let cache_key = (*project_id, *denom);
        if self.accumulator_cache.contains_key(&cache_key) {
            return Ok(self.accumulator_cache.get(&cache_key));
        }

        let key = StateKey::Accumulator(*project_id, *denom).to_bytes();
        if let Some(bytes) = self.trie.get(&key)? {
            let acc: AccumulatorState = deserialize(&bytes)?;
            self.accumulator_cache.insert(cache_key, acc);
            Ok(self.accumulator_cache.get(&cache_key))
        } else {
            Ok(None)
        }
    }

    /// Load a balance from the trie into cache.
    pub fn load_balance(
        &mut self,
        project_id: &[u8; 20],
        contributor: &[u8; 20],
        denom: &Denomination,
    ) -> Result<Option<&ContributorBalance>, StorageError> {
        let cache_key = (*project_id, *contributor, *denom);
        if self.balance_cache.contains_key(&cache_key) {
            return Ok(self.balance_cache.get(&cache_key));
        }

        let key = StateKey::Balance(*contributor, *project_id, *denom).to_bytes();
        if let Some(bytes) = self.trie.get(&key)? {
            let balance: ContributorBalance = deserialize(&bytes)?;
            self.balance_cache.insert(cache_key, balance);
            Ok(self.balance_cache.get(&cache_key))
        } else {
            Ok(None)
        }
    }

    /// Load a claimable balance from the trie into cache.
    pub fn load_claimable(
        &mut self,
        identity: &[u8; 20],
        denom: &Denomination,
    ) -> Result<U256, StorageError> {
        let cache_key = (*identity, *denom);
        if let Some(&amount) = self.claimable_cache.get(&cache_key) {
            return Ok(amount);
        }

        let key = StateKey::Claimable(*identity, *denom).to_bytes();
        if let Some(bytes) = self.trie.get(&key)? {
            let amount: U256 = deserialize(&bytes)?;
            self.claimable_cache.insert(cache_key, amount);
            Ok(amount)
        } else {
            Ok(U256::zero())
        }
    }

    /// Load a locked bond from the trie into cache.
    pub fn load_locked_bond(&mut self, attestation_id: &[u8; 20]) -> Result<Option<U256>, StorageError> {
        if let Some(&amount) = self.locked_bond_cache.get(attestation_id) {
            return Ok(Some(amount));
        }

        let key = StateKey::AttestationBond(*attestation_id).to_bytes();
        if let Some(bytes) = self.trie.get(&key)? {
            let amount: U256 = deserialize(&bytes)?;
            self.locked_bond_cache.insert(*attestation_id, amount);
            Ok(Some(amount))
        } else {
            Ok(None)
        }
    }

    /// Load a challenge bond from the trie into cache.
    pub fn load_challenge_bond(&mut self, challenge_id: &[u8; 20]) -> Result<Option<U256>, StorageError> {
        if let Some(&amount) = self.challenge_bond_cache.get(challenge_id) {
            return Ok(Some(amount));
        }

        let key = StateKey::ChallengeBond(*challenge_id).to_bytes();
        if let Some(bytes) = self.trie.get(&key)? {
            let amount: U256 = deserialize(&bytes)?;
            self.challenge_bond_cache.insert(*challenge_id, amount);
            Ok(Some(amount))
        } else {
            Ok(None)
        }
    }

    /// Commit all dirty entries to the trie and return the new state root.
    pub fn commit(&mut self) -> Result<[u8; 32], StorageError> {
        // Collect dirty keys first to avoid borrowing issues
        let dirty_keys: Vec<StateKey> = self.dirty.drain().collect();

        // Flush dirty entries to trie
        for key in dirty_keys {
            let key_bytes = key.to_bytes();
            let value = self.serialize_state_entry(&key)?;

            if let Some(bytes) = value {
                self.trie.insert(&key_bytes, &bytes)?;
            } else {
                self.trie.delete(&key_bytes)?;
            }
        }

        // Commit trie nodes to backend
        self.trie.commit()?;

        Ok(self.trie.root_hash())
    }

    /// Rollback uncommitted changes.
    pub fn rollback(&mut self) {
        self.trie.rollback();
        self.dirty.clear();

        // Clear caches to avoid stale data
        self.identity_cache.clear();
        self.project_cache.clear();
        self.attestation_cache.clear();
        self.challenge_cache.clear();
        self.attestation_challenge_cache.clear();
        self.accumulator_cache.clear();
        self.balance_cache.clear();
        self.claimable_cache.clear();
        self.locked_bond_cache.clear();
        self.challenge_bond_cache.clear();
    }

    /// Serialize a state entry from cache.
    fn serialize_state_entry(&self, key: &StateKey) -> Result<Option<Vec<u8>>, StorageError> {
        match key {
            StateKey::Identity(addr) => {
                self.identity_cache.get(addr)
                    .map(|v| serialize(v).map_err(StorageError::from))
                    .transpose()
            }
            StateKey::Project(id) => {
                self.project_cache.get(id)
                    .map(|v| serialize(v).map_err(StorageError::from))
                    .transpose()
            }
            StateKey::Attestation(id) => {
                self.attestation_cache.get(id)
                    .map(|v| serialize(v).map_err(StorageError::from))
                    .transpose()
            }
            StateKey::Challenge(id) => {
                self.challenge_cache.get(id)
                    .map(|v| serialize(v).map_err(StorageError::from))
                    .transpose()
            }
            StateKey::AttestationChallenge(attestation_id) => {
                self.attestation_challenge_cache.get(attestation_id)
                    .map(|v| serialize(v).map_err(StorageError::from))
                    .transpose()
            }
            StateKey::Accumulator(project_id, denom) => {
                self.accumulator_cache.get(&(*project_id, *denom))
                    .map(|v| serialize(v).map_err(StorageError::from))
                    .transpose()
            }
            StateKey::Balance(contributor, project_id, denom) => {
                self.balance_cache.get(&(*project_id, *contributor, *denom))
                    .map(|v| serialize(v).map_err(StorageError::from))
                    .transpose()
            }
            StateKey::Claimable(identity, denom) => {
                self.claimable_cache.get(&(*identity, *denom))
                    .map(|v| serialize(v).map_err(StorageError::from))
                    .transpose()
            }
            StateKey::AttestationBond(id) => {
                self.locked_bond_cache.get(id)
                    .map(|v| serialize(v).map_err(StorageError::from))
                    .transpose()
            }
            StateKey::ChallengeBond(id) => {
                self.challenge_bond_cache.get(id)
                    .map(|v| serialize(v).map_err(StorageError::from))
                    .transpose()
            }
            StateKey::TxDifficultyState => {
                Ok(Some(serialize(&self.tx_difficulty_state)?))
            }
        }
    }

    /// Get count of identities in cache.
    pub fn identity_count(&self) -> usize {
        self.identity_cache.len()
    }

    /// Get count of projects in cache.
    pub fn project_count(&self) -> usize {
        self.project_cache.len()
    }

    /// Get count of attestations in cache.
    pub fn attestation_count(&self) -> usize {
        self.attestation_cache.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kv::MemoryBackend;
    use crate::trie::EMPTY_ROOT;
    use tunnels_core::{KeyPair, crypto::derive_address};

    fn create_test_identity() -> Identity {
        let kp = KeyPair::generate();
        let address = derive_address(&kp.public_key());
        Identity::new_human(address, kp.public_key())
    }

    fn create_state() -> PersistentState<MemoryBackend> {
        let backend = Arc::new(MemoryBackend::new());
        PersistentState::new(backend)
    }

    #[test]
    fn test_new_state_empty() {
        let state = create_state();
        assert!(state.is_empty());
        assert_eq!(state.state_root(), EMPTY_ROOT);
    }

    #[test]
    fn test_insert_and_commit() {
        use tunnels_state::StateWriter;

        let mut state = create_state();
        let identity = create_test_identity();

        state.insert_identity(identity);
        let root = state.commit().unwrap();

        assert_ne!(root, EMPTY_ROOT);
        assert_eq!(state.identity_count(), 1);
    }

    #[test]
    fn test_persist_and_reload() {
        use tunnels_state::{StateReader, StateWriter};

        let backend = Arc::new(MemoryBackend::new());

        let (root, addr) = {
            let mut state = PersistentState::new(Arc::clone(&backend));
            let identity = create_test_identity();
            let addr = identity.address;
            state.insert_identity(identity);
            (state.commit().unwrap(), addr)
        };

        // Reload state
        let mut state = PersistentState::from_root(backend, root).unwrap();
        state.load_identity(&addr).unwrap();

        assert!(state.get_identity(&addr).is_some());
    }

    #[test]
    fn test_rollback() {
        use tunnels_state::StateWriter;

        let backend = Arc::new(MemoryBackend::new());

        // Create initial state
        let initial_root = {
            let mut state = PersistentState::new(Arc::clone(&backend));
            let identity = create_test_identity();
            state.insert_identity(identity);
            state.commit().unwrap()
        };

        // Make changes and rollback
        {
            let mut state = PersistentState::from_root(Arc::clone(&backend), initial_root).unwrap();
            let identity2 = create_test_identity();
            state.insert_identity(identity2);
            state.rollback();
        }

        // Verify state unchanged
        let state = PersistentState::from_root(backend, initial_root).unwrap();
        assert_eq!(state.state_root(), initial_root);
    }

    #[test]
    fn test_claimable_balance() {
        use tunnels_state::{StateReader, StateWriter};

        let mut state = create_state();
        let identity = [1u8; 20];
        let denom = *b"USD\0\0\0\0\0";

        assert_eq!(state.get_claimable(&identity, &denom), U256::zero());

        state.credit_claimable(&identity, &denom, U256::from(100u64));
        assert_eq!(state.get_claimable(&identity, &denom), U256::from(100u64));

        state.credit_claimable(&identity, &denom, U256::from(50u64));
        assert_eq!(state.get_claimable(&identity, &denom), U256::from(150u64));

        state.debit_claimable(&identity, &denom, U256::from(30u64));
        assert_eq!(state.get_claimable(&identity, &denom), U256::from(120u64));
    }

    #[test]
    fn test_bond_locking() {
        use tunnels_state::{StateReader, StateWriter};

        let mut state = create_state();
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
    fn test_protocol_state_persistence() {
        use tunnels_state::StateWriter;

        let backend = Arc::new(MemoryBackend::new());

        let root = {
            let mut state = PersistentState::new(Arc::clone(&backend));
            state.update_tx_difficulty_state(|ts| {
                ts.current_tx_difficulty = 20;
            });
            state.commit().unwrap()
        };

        let state = PersistentState::from_root(backend, root).unwrap();
        assert_eq!(state.tx_difficulty_state.current_tx_difficulty, 20);
    }
}
