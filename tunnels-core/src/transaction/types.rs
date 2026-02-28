//! Transaction enum with all 13 protocol transaction types.

use serde::{Deserialize, Serialize};

use crate::crypto::PublicKey;
use crate::types::{ChallengeRuling, Denomination, RecipientType};
use crate::u256::U256;

/// All protocol transaction types.
///
/// The Tunnels protocol defines exactly 13 transaction types.
/// Each transaction must be signed by the appropriate identity
/// as specified in the protocol documentation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Transaction {
    /// Create a new Human identity.
    /// Signed by: The new identity's key.
    CreateIdentity {
        /// The public key for the new identity.
        public_key: PublicKey,
    },

    /// Create a new Agent identity linked to a parent Human.
    /// Signed by: The parent's key.
    CreateAgent {
        /// Address of the parent Human identity.
        parent: [u8; 20],
        /// The public key for the new Agent identity.
        public_key: PublicKey,
    },

    /// Update an identity's profile hash.
    /// Signed by: The identity's key.
    UpdateProfileHash {
        /// Address of the identity to update.
        identity: [u8; 20],
        /// New profile hash (SHA-256 of off-chain profile data).
        hash: [u8; 32],
    },

    /// Register a new project with immutable parameters.
    /// Signed by: The creator's key.
    RegisterProject {
        /// Address of the project creator.
        creator: [u8; 20],
        /// Reserve rate in basis points (0-10000).
        rho: u32,
        /// Application fee rate in basis points (0-10000).
        phi: u32,
        /// Genesis allocation rate in basis points (0-10000).
        gamma: u32,
        /// Predecessor allocation rate in basis points (0-10000).
        delta: u32,
        /// Identity receiving the application fee.
        phi_recipient: [u8; 20],
        /// Identity or project receiving the genesis allocation.
        gamma_recipient: [u8; 20],
        /// Whether gamma_recipient is an Identity or Project.
        gamma_recipient_type: RecipientType,
        /// Identity authorized to withdraw from reserve.
        reserve_authority: [u8; 20],
        /// Identity authorized to deposit revenue.
        deposit_authority: [u8; 20],
        /// Identity authorized to resolve challenges.
        adjudicator: [u8; 20],
        /// Optional predecessor project for provenance linking.
        predecessor: Option<[u8; 20]>,
        /// Verification window duration in seconds (min 86400).
        verification_window: u64,
        /// Minimum bond required per attestation.
        attestation_bond: u64,
        /// Bond required to file a challenge (0 = no bond required).
        challenge_bond: u64,
        /// Optional hash of project description/evidence.
        evidence_hash: Option<[u8; 32]>,
    },

    /// Submit an attestation (contribution claim).
    /// Signed by: The contributor's key.
    SubmitAttestation {
        /// Project to attest contribution to.
        project_id: [u8; 20],
        /// Contribution weight (must be > 0).
        units: u64,
        /// SHA-256 hash of evidence/proof artifact.
        evidence_hash: [u8; 32],
        /// Identity posting the bond (may be contributor or third party).
        bond_poster: [u8; 20],
        /// Bond amount (must be >= project minimum bond).
        bond_amount: U256,
        /// Denomination of the bond.
        bond_denomination: Denomination,
    },

    /// Challenge a pending attestation.
    /// Signed by: The challenger's key.
    ChallengeAttestation {
        /// ID of the attestation to challenge.
        attestation_id: [u8; 20],
        /// SHA-256 hash of challenge evidence.
        evidence_hash: [u8; 32],
        /// Identity posting the challenge bond (None if no bond required).
        bond_poster: Option<[u8; 20]>,
        /// Challenge bond amount (0 if no bond required).
        bond_amount: U256,
    },

    /// Respond to a challenge with counter-evidence.
    /// Signed by: The original contributor's key.
    RespondToChallenge {
        /// ID of the challenge to respond to.
        challenge_id: [u8; 20],
        /// SHA-256 hash of response evidence.
        evidence_hash: [u8; 32],
    },

    /// Resolve a challenge (adjudicator decision).
    /// Signed by: The adjudicator's key.
    ResolveChallenge {
        /// ID of the challenge to resolve.
        challenge_id: [u8; 20],
        /// The ruling (Valid or Invalid).
        ruling: ChallengeRuling,
    },

    /// Finalize an unchallenged attestation after verification window.
    /// Signed by: Any key (permissionless operation).
    FinalizeAttestation {
        /// ID of the attestation to finalize.
        attestation_id: [u8; 20],
    },

    /// Deposit revenue into a project's router.
    /// Signed by: The deposit authority's key.
    DepositRevenue {
        /// Project to deposit into.
        project_id: [u8; 20],
        /// Amount to deposit.
        amount: U256,
        /// Denomination of the deposit.
        denomination: Denomination,
    },

    /// Claim accumulated revenue.
    /// Signed by: The contributor's key.
    ClaimRevenue {
        /// Project to claim from.
        project_id: [u8; 20],
        /// Denomination to claim.
        denomination: Denomination,
    },

    /// Withdraw from project reserve pool.
    /// Signed by: The reserve authority's key.
    ClaimReserve {
        /// Project to withdraw from.
        project_id: [u8; 20],
        /// Denomination to withdraw.
        denomination: Denomination,
        /// Amount to withdraw.
        amount: U256,
    },

    /// Deactivate an Agent identity.
    /// Signed by: The parent's key.
    DeactivateAgent {
        /// Address of the Agent to deactivate.
        agent: [u8; 20],
    },
}

impl Transaction {
    /// Get a human-readable name for this transaction type.
    pub fn name(&self) -> &'static str {
        match self {
            Transaction::CreateIdentity { .. } => "create_identity",
            Transaction::CreateAgent { .. } => "create_agent",
            Transaction::UpdateProfileHash { .. } => "update_profile_hash",
            Transaction::RegisterProject { .. } => "register_project",
            Transaction::SubmitAttestation { .. } => "submit_attestation",
            Transaction::ChallengeAttestation { .. } => "challenge_attestation",
            Transaction::RespondToChallenge { .. } => "respond_to_challenge",
            Transaction::ResolveChallenge { .. } => "resolve_challenge",
            Transaction::FinalizeAttestation { .. } => "finalize_attestation",
            Transaction::DepositRevenue { .. } => "deposit_revenue",
            Transaction::ClaimRevenue { .. } => "claim_revenue",
            Transaction::ClaimReserve { .. } => "claim_reserve",
            Transaction::DeactivateAgent { .. } => "deactivate_agent",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::KeyPair;
    use crate::types::denomination_common::USD;

    #[test]
    fn test_create_identity() {
        let kp = KeyPair::generate();
        let tx = Transaction::CreateIdentity {
            public_key: kp.public_key(),
        };
        assert_eq!(tx.name(), "create_identity");

        let bytes = crate::serialization::serialize(&tx).unwrap();
        let recovered: Transaction = crate::serialization::deserialize(&bytes).unwrap();
        assert_eq!(tx, recovered);
    }

    #[test]
    fn test_create_agent() {
        let kp = KeyPair::generate();
        let tx = Transaction::CreateAgent {
            parent: [1u8; 20],
            public_key: kp.public_key(),
        };
        assert_eq!(tx.name(), "create_agent");

        let bytes = crate::serialization::serialize(&tx).unwrap();
        let recovered: Transaction = crate::serialization::deserialize(&bytes).unwrap();
        assert_eq!(tx, recovered);
    }

    #[test]
    fn test_update_profile_hash() {
        let tx = Transaction::UpdateProfileHash {
            identity: [1u8; 20],
            hash: [2u8; 32],
        };
        assert_eq!(tx.name(), "update_profile_hash");

        let bytes = crate::serialization::serialize(&tx).unwrap();
        let recovered: Transaction = crate::serialization::deserialize(&bytes).unwrap();
        assert_eq!(tx, recovered);
    }

    #[test]
    fn test_register_project() {
        let tx = Transaction::RegisterProject {
            creator: [1u8; 20],
            rho: 1000,
            phi: 500,
            gamma: 1500,
            delta: 0,
            phi_recipient: [2u8; 20],
            gamma_recipient: [3u8; 20],
            gamma_recipient_type: RecipientType::Identity,
            reserve_authority: [4u8; 20],
            deposit_authority: [5u8; 20],
            adjudicator: [6u8; 20],
            predecessor: None,
            verification_window: 86_400,
            attestation_bond: 100,
            challenge_bond: 50,
            evidence_hash: Some([7u8; 32]),
        };
        assert_eq!(tx.name(), "register_project");

        let bytes = crate::serialization::serialize(&tx).unwrap();
        let recovered: Transaction = crate::serialization::deserialize(&bytes).unwrap();
        assert_eq!(tx, recovered);
    }

    #[test]
    fn test_submit_attestation() {
        let tx = Transaction::SubmitAttestation {
            project_id: [1u8; 20],
            units: 100,
            evidence_hash: [2u8; 32],
            bond_poster: [3u8; 20],
            bond_amount: U256::from(1000u64),
            bond_denomination: USD,
        };
        assert_eq!(tx.name(), "submit_attestation");

        let bytes = crate::serialization::serialize(&tx).unwrap();
        let recovered: Transaction = crate::serialization::deserialize(&bytes).unwrap();
        assert_eq!(tx, recovered);
    }

    #[test]
    fn test_challenge_attestation() {
        let tx = Transaction::ChallengeAttestation {
            attestation_id: [1u8; 20],
            evidence_hash: [2u8; 32],
            bond_poster: Some([3u8; 20]),
            bond_amount: U256::from(50u64),
        };
        assert_eq!(tx.name(), "challenge_attestation");

        let bytes = crate::serialization::serialize(&tx).unwrap();
        let recovered: Transaction = crate::serialization::deserialize(&bytes).unwrap();
        assert_eq!(tx, recovered);
    }

    #[test]
    fn test_respond_to_challenge() {
        let tx = Transaction::RespondToChallenge {
            challenge_id: [1u8; 20],
            evidence_hash: [2u8; 32],
        };
        assert_eq!(tx.name(), "respond_to_challenge");

        let bytes = crate::serialization::serialize(&tx).unwrap();
        let recovered: Transaction = crate::serialization::deserialize(&bytes).unwrap();
        assert_eq!(tx, recovered);
    }

    #[test]
    fn test_resolve_challenge() {
        for ruling in [ChallengeRuling::Valid, ChallengeRuling::Invalid] {
            let tx = Transaction::ResolveChallenge {
                challenge_id: [1u8; 20],
                ruling,
            };
            assert_eq!(tx.name(), "resolve_challenge");

            let bytes = crate::serialization::serialize(&tx).unwrap();
            let recovered: Transaction = crate::serialization::deserialize(&bytes).unwrap();
            assert_eq!(tx, recovered);
        }
    }

    #[test]
    fn test_finalize_attestation() {
        let tx = Transaction::FinalizeAttestation {
            attestation_id: [1u8; 20],
        };
        assert_eq!(tx.name(), "finalize_attestation");

        let bytes = crate::serialization::serialize(&tx).unwrap();
        let recovered: Transaction = crate::serialization::deserialize(&bytes).unwrap();
        assert_eq!(tx, recovered);
    }

    #[test]
    fn test_deposit_revenue() {
        let tx = Transaction::DepositRevenue {
            project_id: [1u8; 20],
            amount: U256::from(10000u64),
            denomination: USD,
        };
        assert_eq!(tx.name(), "deposit_revenue");

        let bytes = crate::serialization::serialize(&tx).unwrap();
        let recovered: Transaction = crate::serialization::deserialize(&bytes).unwrap();
        assert_eq!(tx, recovered);
    }

    #[test]
    fn test_claim_revenue() {
        let tx = Transaction::ClaimRevenue {
            project_id: [1u8; 20],
            denomination: USD,
        };
        assert_eq!(tx.name(), "claim_revenue");

        let bytes = crate::serialization::serialize(&tx).unwrap();
        let recovered: Transaction = crate::serialization::deserialize(&bytes).unwrap();
        assert_eq!(tx, recovered);
    }

    #[test]
    fn test_claim_reserve() {
        let tx = Transaction::ClaimReserve {
            project_id: [1u8; 20],
            denomination: USD,
            amount: U256::from(5000u64),
        };
        assert_eq!(tx.name(), "claim_reserve");

        let bytes = crate::serialization::serialize(&tx).unwrap();
        let recovered: Transaction = crate::serialization::deserialize(&bytes).unwrap();
        assert_eq!(tx, recovered);
    }

    #[test]
    fn test_deactivate_agent() {
        let tx = Transaction::DeactivateAgent { agent: [1u8; 20] };
        assert_eq!(tx.name(), "deactivate_agent");

        let bytes = crate::serialization::serialize(&tx).unwrap();
        let recovered: Transaction = crate::serialization::deserialize(&bytes).unwrap();
        assert_eq!(tx, recovered);
    }

    #[test]
    fn test_all_13_transaction_types_serialize() {
        // This test ensures acceptance criterion 2: all 13 transaction types
        // can be serialized and deserialized with equality.
        let kp = KeyPair::generate();

        let transactions = vec![
            Transaction::CreateIdentity {
                public_key: kp.public_key(),
            },
            Transaction::CreateAgent {
                parent: [1u8; 20],
                public_key: kp.public_key(),
            },
            Transaction::UpdateProfileHash {
                identity: [1u8; 20],
                hash: [2u8; 32],
            },
            Transaction::RegisterProject {
                creator: [1u8; 20],
                rho: 1000,
                phi: 500,
                gamma: 1500,
                delta: 1000,
                phi_recipient: [2u8; 20],
                gamma_recipient: [3u8; 20],
                gamma_recipient_type: RecipientType::Project,
                reserve_authority: [4u8; 20],
                deposit_authority: [5u8; 20],
                adjudicator: [6u8; 20],
                predecessor: Some([7u8; 20]),
                verification_window: 86_400,
                attestation_bond: 100,
                challenge_bond: 50,
                evidence_hash: None,
            },
            Transaction::SubmitAttestation {
                project_id: [1u8; 20],
                units: 100,
                evidence_hash: [2u8; 32],
                bond_poster: [3u8; 20],
                bond_amount: U256::from(1000u64),
                bond_denomination: USD,
            },
            Transaction::ChallengeAttestation {
                attestation_id: [1u8; 20],
                evidence_hash: [2u8; 32],
                bond_poster: None,
                bond_amount: U256::zero(),
            },
            Transaction::RespondToChallenge {
                challenge_id: [1u8; 20],
                evidence_hash: [2u8; 32],
            },
            Transaction::ResolveChallenge {
                challenge_id: [1u8; 20],
                ruling: ChallengeRuling::Valid,
            },
            Transaction::FinalizeAttestation {
                attestation_id: [1u8; 20],
            },
            Transaction::DepositRevenue {
                project_id: [1u8; 20],
                amount: U256::from(10000u64),
                denomination: USD,
            },
            Transaction::ClaimRevenue {
                project_id: [1u8; 20],
                denomination: USD,
            },
            Transaction::ClaimReserve {
                project_id: [1u8; 20],
                denomination: USD,
                amount: U256::from(5000u64),
            },
            Transaction::DeactivateAgent { agent: [1u8; 20] },
        ];

        assert_eq!(transactions.len(), 13, "Must test all 13 transaction types");

        for tx in transactions {
            let bytes = crate::serialization::serialize(&tx).unwrap();
            let recovered: Transaction = crate::serialization::deserialize(&bytes).unwrap();
            assert_eq!(tx, recovered, "Failed for transaction: {}", tx.name());
        }
    }
}
