//! Identity types for the Tunnels protocol.
//!
//! An identity is a persistent, non-transferable record that accumulates
//! contribution history across the ecosystem.

use serde::{Deserialize, Serialize};

use crate::crypto::PublicKey;

/// The type of identity: Human or Agent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IdentityType {
    /// A human identity controlled directly by a person.
    Human,
    /// An agent identity linked to a parent Human identity.
    /// Revenue earned by Agents routes to their parent.
    Agent,
}

/// A protocol identity.
///
/// Identities are the fundamental unit of participation in the Tunnels
/// protocol. They accumulate contribution history across projects and
/// are controlled by a single cryptographic key pair.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    /// The 20-byte address derived from the public key.
    /// This is the identity's on-chain identifier.
    pub address: [u8; 20],

    /// The Ed25519 public key that controls this identity.
    /// All transactions from this identity must be signed with
    /// the corresponding private key.
    pub public_key: PublicKey,

    /// Whether this is a Human or Agent identity.
    pub identity_type: IdentityType,

    /// For Agent identities, the address of the parent Human identity.
    /// Revenue earned by Agents automatically routes to the parent.
    /// None for Human identities.
    pub parent: Option<[u8; 20]>,

    /// Optional hash of off-chain identity profile data.
    /// The protocol stores only the hash; the content lives off-chain.
    pub profile_hash: Option<[u8; 32]>,

    /// Whether this identity is active.
    /// Only applies to Agent identities - they can be deactivated
    /// by their parent. Deactivated agents cannot create new attestations.
    pub active: bool,
}

impl Identity {
    /// Create a new Human identity.
    pub fn new_human(address: [u8; 20], public_key: PublicKey) -> Self {
        Self {
            address,
            public_key,
            identity_type: IdentityType::Human,
            parent: None,
            profile_hash: None,
            active: true,
        }
    }

    /// Create a new Agent identity.
    pub fn new_agent(address: [u8; 20], public_key: PublicKey, parent: [u8; 20]) -> Self {
        Self {
            address,
            public_key,
            identity_type: IdentityType::Agent,
            parent: Some(parent),
            profile_hash: None,
            active: true,
        }
    }

    /// Check if this is a Human identity.
    #[inline]
    pub fn is_human(&self) -> bool {
        matches!(self.identity_type, IdentityType::Human)
    }

    /// Check if this is an Agent identity.
    #[inline]
    pub fn is_agent(&self) -> bool {
        matches!(self.identity_type, IdentityType::Agent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::{derive_address, KeyPair};

    #[test]
    fn test_new_human() {
        let kp = KeyPair::generate();
        let address = derive_address(&kp.public_key());
        let identity = Identity::new_human(address, kp.public_key());

        assert_eq!(identity.address, address);
        assert!(identity.is_human());
        assert!(!identity.is_agent());
        assert!(identity.parent.is_none());
        assert!(identity.active);
    }

    #[test]
    fn test_new_agent() {
        let parent_kp = KeyPair::generate();
        let agent_kp = KeyPair::generate();
        let parent_address = derive_address(&parent_kp.public_key());
        let agent_address = derive_address(&agent_kp.public_key());

        let agent = Identity::new_agent(agent_address, agent_kp.public_key(), parent_address);

        assert_eq!(agent.address, agent_address);
        assert!(agent.is_agent());
        assert!(!agent.is_human());
        assert_eq!(agent.parent, Some(parent_address));
        assert!(agent.active);
    }

    #[test]
    fn test_serialization() {
        let kp = KeyPair::generate();
        let address = derive_address(&kp.public_key());
        let identity = Identity::new_human(address, kp.public_key());

        let bytes = crate::serialization::serialize(&identity).unwrap();
        let recovered: Identity = crate::serialization::deserialize(&bytes).unwrap();

        assert_eq!(identity, recovered);
    }

    #[test]
    fn test_identity_type_serialization() {
        let human = IdentityType::Human;
        let agent = IdentityType::Agent;

        let human_bytes = crate::serialization::serialize(&human).unwrap();
        let agent_bytes = crate::serialization::serialize(&agent).unwrap();

        let recovered_human: IdentityType = crate::serialization::deserialize(&human_bytes).unwrap();
        let recovered_agent: IdentityType = crate::serialization::deserialize(&agent_bytes).unwrap();

        assert_eq!(human, recovered_human);
        assert_eq!(agent, recovered_agent);
    }
}
