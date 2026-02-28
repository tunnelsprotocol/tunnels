//! Key schema encoding for storage.
//!
//! All state is stored with prefixed keys to enable efficient range queries
//! and logical grouping.

use tunnels_core::Denomination;

/// Key prefixes for different state types.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyPrefix {
    /// Identity: `0x01 || address`
    Identity = 0x01,
    /// Project: `0x02 || project_id`
    Project = 0x02,
    /// Attestation: `0x03 || attestation_id`
    Attestation = 0x03,
    /// Challenge: `0x04 || challenge_id`
    Challenge = 0x04,
    /// Accumulator: `0x05 || project_id || denom`
    Accumulator = 0x05,
    /// Balance: `0x06 || contributor || project_id || denom`
    Balance = 0x06,
    /// Claimable: `0x07 || identity || denom`
    Claimable = 0x07,
    /// Attestation bond: `0x08 || attestation_id`
    AttestationBond = 0x08,
    /// Challenge bond: `0x09 || challenge_id`
    ChallengeBond = 0x09,
    /// Protocol transaction difficulty state: `0x0B`
    TxDifficultyState = 0x0B,
    /// Attestation to challenge mapping: `0x0C || attestation_id`
    AttestationChallenge = 0x0C,
    /// Trie node: `0x10 || node_hash`
    TrieNode = 0x10,
    /// Block by hash: `0x20 || block_hash`
    BlockByHash = 0x20,
    /// Block hash by height: `0x21 || height`
    BlockHashByHeight = 0x21,
    /// Latest block height: `0x22`
    LatestBlockHeight = 0x22,
    /// State root by height: `0x23 || height`
    StateRootByHeight = 0x23,
}

/// State key for addressing state entries.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum StateKey {
    /// Identity by address.
    Identity([u8; 20]),
    /// Project by ID.
    Project([u8; 20]),
    /// Attestation by ID.
    Attestation([u8; 20]),
    /// Challenge by ID.
    Challenge([u8; 20]),
    /// Accumulator by (project_id, denomination).
    Accumulator([u8; 20], Denomination),
    /// Balance by (contributor, project_id, denomination).
    Balance([u8; 20], [u8; 20], Denomination),
    /// Claimable by (identity, denomination).
    Claimable([u8; 20], Denomination),
    /// Attestation bond by attestation_id.
    AttestationBond([u8; 20]),
    /// Challenge bond by challenge_id.
    ChallengeBond([u8; 20]),
    /// Protocol transaction difficulty state.
    TxDifficultyState,
    /// Attestation to challenge mapping.
    AttestationChallenge([u8; 20]),
}

impl StateKey {
    /// Convert the state key to bytes for storage.
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            StateKey::Identity(addr) => {
                let mut key = Vec::with_capacity(21);
                key.push(KeyPrefix::Identity as u8);
                key.extend_from_slice(addr);
                key
            }
            StateKey::Project(id) => {
                let mut key = Vec::with_capacity(21);
                key.push(KeyPrefix::Project as u8);
                key.extend_from_slice(id);
                key
            }
            StateKey::Attestation(id) => {
                let mut key = Vec::with_capacity(21);
                key.push(KeyPrefix::Attestation as u8);
                key.extend_from_slice(id);
                key
            }
            StateKey::Challenge(id) => {
                let mut key = Vec::with_capacity(21);
                key.push(KeyPrefix::Challenge as u8);
                key.extend_from_slice(id);
                key
            }
            StateKey::Accumulator(project_id, denom) => {
                let mut key = Vec::with_capacity(29);
                key.push(KeyPrefix::Accumulator as u8);
                key.extend_from_slice(project_id);
                key.extend_from_slice(denom);
                key
            }
            StateKey::Balance(contributor, project_id, denom) => {
                let mut key = Vec::with_capacity(49);
                key.push(KeyPrefix::Balance as u8);
                key.extend_from_slice(contributor);
                key.extend_from_slice(project_id);
                key.extend_from_slice(denom);
                key
            }
            StateKey::Claimable(identity, denom) => {
                let mut key = Vec::with_capacity(29);
                key.push(KeyPrefix::Claimable as u8);
                key.extend_from_slice(identity);
                key.extend_from_slice(denom);
                key
            }
            StateKey::AttestationBond(id) => {
                let mut key = Vec::with_capacity(21);
                key.push(KeyPrefix::AttestationBond as u8);
                key.extend_from_slice(id);
                key
            }
            StateKey::ChallengeBond(id) => {
                let mut key = Vec::with_capacity(21);
                key.push(KeyPrefix::ChallengeBond as u8);
                key.extend_from_slice(id);
                key
            }
            StateKey::TxDifficultyState => {
                vec![KeyPrefix::TxDifficultyState as u8]
            }
            StateKey::AttestationChallenge(id) => {
                let mut key = Vec::with_capacity(21);
                key.push(KeyPrefix::AttestationChallenge as u8);
                key.extend_from_slice(id);
                key
            }
        }
    }

    /// Parse a state key from bytes.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.is_empty() {
            return None;
        }

        let prefix = bytes[0];
        let data = &bytes[1..];

        match prefix {
            x if x == KeyPrefix::Identity as u8 => {
                if data.len() != 20 {
                    return None;
                }
                let mut addr = [0u8; 20];
                addr.copy_from_slice(data);
                Some(StateKey::Identity(addr))
            }
            x if x == KeyPrefix::Project as u8 => {
                if data.len() != 20 {
                    return None;
                }
                let mut id = [0u8; 20];
                id.copy_from_slice(data);
                Some(StateKey::Project(id))
            }
            x if x == KeyPrefix::Attestation as u8 => {
                if data.len() != 20 {
                    return None;
                }
                let mut id = [0u8; 20];
                id.copy_from_slice(data);
                Some(StateKey::Attestation(id))
            }
            x if x == KeyPrefix::Challenge as u8 => {
                if data.len() != 20 {
                    return None;
                }
                let mut id = [0u8; 20];
                id.copy_from_slice(data);
                Some(StateKey::Challenge(id))
            }
            x if x == KeyPrefix::Accumulator as u8 => {
                if data.len() != 28 {
                    return None;
                }
                let mut project_id = [0u8; 20];
                let mut denom = [0u8; 8];
                project_id.copy_from_slice(&data[..20]);
                denom.copy_from_slice(&data[20..]);
                Some(StateKey::Accumulator(project_id, denom))
            }
            x if x == KeyPrefix::Balance as u8 => {
                if data.len() != 48 {
                    return None;
                }
                let mut contributor = [0u8; 20];
                let mut project_id = [0u8; 20];
                let mut denom = [0u8; 8];
                contributor.copy_from_slice(&data[..20]);
                project_id.copy_from_slice(&data[20..40]);
                denom.copy_from_slice(&data[40..]);
                Some(StateKey::Balance(contributor, project_id, denom))
            }
            x if x == KeyPrefix::Claimable as u8 => {
                if data.len() != 28 {
                    return None;
                }
                let mut identity = [0u8; 20];
                let mut denom = [0u8; 8];
                identity.copy_from_slice(&data[..20]);
                denom.copy_from_slice(&data[20..]);
                Some(StateKey::Claimable(identity, denom))
            }
            x if x == KeyPrefix::AttestationBond as u8 => {
                if data.len() != 20 {
                    return None;
                }
                let mut id = [0u8; 20];
                id.copy_from_slice(data);
                Some(StateKey::AttestationBond(id))
            }
            x if x == KeyPrefix::ChallengeBond as u8 => {
                if data.len() != 20 {
                    return None;
                }
                let mut id = [0u8; 20];
                id.copy_from_slice(data);
                Some(StateKey::ChallengeBond(id))
            }
            x if x == KeyPrefix::TxDifficultyState as u8 => {
                if !data.is_empty() {
                    return None;
                }
                Some(StateKey::TxDifficultyState)
            }
            x if x == KeyPrefix::AttestationChallenge as u8 => {
                if data.len() != 20 {
                    return None;
                }
                let mut id = [0u8; 20];
                id.copy_from_slice(data);
                Some(StateKey::AttestationChallenge(id))
            }
            _ => None,
        }
    }
}

/// Create a trie node key from a hash.
pub fn trie_node_key(hash: &[u8; 32]) -> Vec<u8> {
    let mut key = Vec::with_capacity(33);
    key.push(KeyPrefix::TrieNode as u8);
    key.extend_from_slice(hash);
    key
}

/// Create a block-by-hash key.
pub fn block_by_hash_key(hash: &[u8; 32]) -> Vec<u8> {
    let mut key = Vec::with_capacity(33);
    key.push(KeyPrefix::BlockByHash as u8);
    key.extend_from_slice(hash);
    key
}

/// Create a block-hash-by-height key.
pub fn block_hash_by_height_key(height: u64) -> Vec<u8> {
    let mut key = Vec::with_capacity(9);
    key.push(KeyPrefix::BlockHashByHeight as u8);
    key.extend_from_slice(&height.to_be_bytes());
    key
}

/// Create a latest block height key.
pub fn latest_block_height_key() -> Vec<u8> {
    vec![KeyPrefix::LatestBlockHeight as u8]
}

/// Create a state root by height key.
pub fn state_root_by_height_key(height: u64) -> Vec<u8> {
    let mut key = Vec::with_capacity(9);
    key.push(KeyPrefix::StateRootByHeight as u8);
    key.extend_from_slice(&height.to_be_bytes());
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identity_key_roundtrip() {
        let addr = [1u8; 20];
        let key = StateKey::Identity(addr);
        let bytes = key.to_bytes();
        let parsed = StateKey::from_bytes(&bytes).unwrap();
        assert_eq!(key, parsed);
    }

    #[test]
    fn test_accumulator_key_roundtrip() {
        let project_id = [2u8; 20];
        let denom = *b"USD\0\0\0\0\0";
        let key = StateKey::Accumulator(project_id, denom);
        let bytes = key.to_bytes();
        let parsed = StateKey::from_bytes(&bytes).unwrap();
        assert_eq!(key, parsed);
    }

    #[test]
    fn test_balance_key_roundtrip() {
        let contributor = [3u8; 20];
        let project_id = [4u8; 20];
        let denom = *b"BTC\0\0\0\0\0";
        let key = StateKey::Balance(contributor, project_id, denom);
        let bytes = key.to_bytes();
        let parsed = StateKey::from_bytes(&bytes).unwrap();
        assert_eq!(key, parsed);
    }

    #[test]
    fn test_protocol_state_keys() {
        let diff_key = StateKey::TxDifficultyState;
        let bytes = diff_key.to_bytes();
        assert_eq!(bytes.len(), 1);
        let parsed = StateKey::from_bytes(&bytes).unwrap();
        assert_eq!(diff_key, parsed);
    }

    #[test]
    fn test_key_prefixes_unique() {
        // Verify all prefixes are unique
        let prefixes = [
            KeyPrefix::Identity,
            KeyPrefix::Project,
            KeyPrefix::Attestation,
            KeyPrefix::Challenge,
            KeyPrefix::Accumulator,
            KeyPrefix::Balance,
            KeyPrefix::Claimable,
            KeyPrefix::AttestationBond,
            KeyPrefix::ChallengeBond,
            KeyPrefix::TxDifficultyState,
            KeyPrefix::AttestationChallenge,
            KeyPrefix::TrieNode,
            KeyPrefix::BlockByHash,
            KeyPrefix::BlockHashByHeight,
            KeyPrefix::LatestBlockHeight,
            KeyPrefix::StateRootByHeight,
        ];

        let values: Vec<u8> = prefixes.iter().map(|p| *p as u8).collect();
        let unique: std::collections::HashSet<u8> = values.iter().copied().collect();
        assert_eq!(values.len(), unique.len(), "Duplicate prefix values found");
    }

    #[test]
    fn test_block_keys() {
        let hash = [5u8; 32];
        let key = block_by_hash_key(&hash);
        assert_eq!(key[0], KeyPrefix::BlockByHash as u8);
        assert_eq!(&key[1..], &hash);

        let height = 12345u64;
        let key = block_hash_by_height_key(height);
        assert_eq!(key[0], KeyPrefix::BlockHashByHeight as u8);
        assert_eq!(&key[1..], &height.to_be_bytes());
    }
}
