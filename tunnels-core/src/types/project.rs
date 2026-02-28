//! Project type for the Tunnels protocol.
//!
//! A project is the protocol-level container against which contributions
//! are recorded. It binds attestations to a distribution configuration.

use serde::{Deserialize, Serialize};

/// The type of gamma recipient: Identity or Project.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RecipientType {
    /// Gamma flows to an identity's claimable balance.
    Identity,
    /// Gamma flows to another project's router (enabling founder splits).
    Project,
}

/// A protocol project.
///
/// Projects are immutable containers for contributions. All parameters
/// are set at creation and cannot be changed afterward. This ensures
/// contributors can see the terms before participating.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    /// Unique project identifier (derived from hash of creation tx).
    pub project_id: [u8; 20],

    /// Identity address of the project creator.
    /// Recorded for provenance; confers no special protocol-level privileges.
    pub creator: [u8; 20],

    /// Reserve rate in basis points (0-10000).
    /// Percentage of deposits allocated to the project reserve.
    pub rho: u32,

    /// Application fee rate in basis points (0-10000).
    /// Percentage allocated to the phi_recipient (typically the application).
    pub phi: u32,

    /// Genesis allocation rate in basis points (0-10000).
    /// Percentage allocated to the gamma_recipient (founders).
    pub gamma: u32,

    /// Predecessor allocation rate in basis points (0-10000).
    /// Percentage allocated to the predecessor project (if any).
    pub delta: u32,

    /// Identity address receiving the application fee (phi).
    pub phi_recipient: [u8; 20],

    /// Address receiving the genesis allocation (gamma).
    /// May be an identity or a project address.
    pub gamma_recipient: [u8; 20],

    /// Whether gamma_recipient is an Identity or Project.
    pub gamma_recipient_type: RecipientType,

    /// Identity authorized to withdraw from the reserve pool.
    pub reserve_authority: [u8; 20],

    /// Identity authorized to submit deposit_revenue transactions.
    pub deposit_authority: [u8; 20],

    /// Identity authorized to resolve challenges (adjudicator).
    pub adjudicator: [u8; 20],

    /// Optional predecessor project address for provenance linking.
    /// If set, delta percentage flows to this project's router.
    pub predecessor: Option<[u8; 20]>,

    /// Verification window duration in seconds.
    /// Minimum is 86400 (24 hours).
    pub verification_window: u64,

    /// Minimum bond required per attestation.
    pub attestation_bond: u64,

    /// Bond required to file a challenge (0 = no challenge bond required).
    pub challenge_bond: u64,

    /// Optional hash of off-chain project description/evidence.
    pub evidence_hash: Option<[u8; 32]>,
}

impl Project {
    /// Minimum verification window: 24 hours in seconds.
    pub const MIN_VERIFICATION_WINDOW: u64 = 86_400;

    /// Maximum basis points (100%).
    pub const MAX_BASIS_POINTS: u32 = 10_000;

    /// Validate that the project parameters are well-formed.
    ///
    /// Returns true if:
    /// - rho + phi + gamma + delta <= 10000
    /// - verification_window >= 86400
    /// - If predecessor is None, delta must be 0
    pub fn is_valid(&self) -> bool {
        let total = self.rho as u64 + self.phi as u64 + self.gamma as u64 + self.delta as u64;
        if total > Self::MAX_BASIS_POINTS as u64 {
            return false;
        }

        if self.verification_window < Self::MIN_VERIFICATION_WINDOW {
            return false;
        }

        if self.predecessor.is_none() && self.delta > 0 {
            return false;
        }

        true
    }

    /// Calculate the labor pool percentage (what's left after all allocations).
    ///
    /// Returns the percentage in basis points (0-10000).
    pub fn labor_pool_basis_points(&self) -> u32 {
        Self::MAX_BASIS_POINTS
            .saturating_sub(self.rho)
            .saturating_sub(self.phi)
            .saturating_sub(self.gamma)
            .saturating_sub(self.delta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_project() -> Project {
        Project {
            project_id: [1u8; 20],
            creator: [2u8; 20],
            rho: 1000,  // 10%
            phi: 500,   // 5%
            gamma: 1500, // 15%
            delta: 0,
            phi_recipient: [3u8; 20],
            gamma_recipient: [4u8; 20],
            gamma_recipient_type: RecipientType::Identity,
            reserve_authority: [5u8; 20],
            deposit_authority: [6u8; 20],
            adjudicator: [7u8; 20],
            predecessor: None,
            verification_window: 86_400,
            attestation_bond: 100,
            challenge_bond: 50,
            evidence_hash: None,
        }
    }

    #[test]
    fn test_valid_project() {
        let project = test_project();
        assert!(project.is_valid());
    }

    #[test]
    fn test_invalid_total_exceeds_100_percent() {
        let mut project = test_project();
        project.rho = 5000;
        project.phi = 3000;
        project.gamma = 2000;
        project.delta = 1000; // Total = 11000 > 10000
        project.predecessor = Some([8u8; 20]);
        assert!(!project.is_valid());
    }

    #[test]
    fn test_invalid_verification_window() {
        let mut project = test_project();
        project.verification_window = 3600; // 1 hour, less than 24-hour minimum
        assert!(!project.is_valid());
    }

    #[test]
    fn test_invalid_delta_without_predecessor() {
        let mut project = test_project();
        project.delta = 1000;
        project.predecessor = None;
        assert!(!project.is_valid());
    }

    #[test]
    fn test_valid_with_predecessor() {
        let mut project = test_project();
        project.delta = 1000;
        project.predecessor = Some([8u8; 20]);
        assert!(project.is_valid());
    }

    #[test]
    fn test_labor_pool_calculation() {
        let project = test_project();
        // rho=10%, phi=5%, gamma=15%, delta=0% => labor pool = 70%
        assert_eq!(project.labor_pool_basis_points(), 7000);
    }

    #[test]
    fn test_zero_labor_pool() {
        let mut project = test_project();
        project.rho = 2500;
        project.phi = 2500;
        project.gamma = 2500;
        project.delta = 2500;
        project.predecessor = Some([8u8; 20]);
        assert!(project.is_valid());
        assert_eq!(project.labor_pool_basis_points(), 0);
    }

    #[test]
    fn test_serialization() {
        let project = test_project();
        let bytes = crate::serialization::serialize(&project).unwrap();
        let recovered: Project = crate::serialization::deserialize(&bytes).unwrap();
        assert_eq!(project, recovered);
    }

    #[test]
    fn test_recipient_type_serialization() {
        let identity = RecipientType::Identity;
        let project = RecipientType::Project;

        let id_bytes = crate::serialization::serialize(&identity).unwrap();
        let proj_bytes = crate::serialization::serialize(&project).unwrap();

        let recovered_id: RecipientType = crate::serialization::deserialize(&id_bytes).unwrap();
        let recovered_proj: RecipientType = crate::serialization::deserialize(&proj_bytes).unwrap();

        assert_eq!(identity, recovered_id);
        assert_eq!(project, recovered_proj);
    }
}
