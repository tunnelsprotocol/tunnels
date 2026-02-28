//! Challenge types for the Tunnels protocol.
//!
//! Challenges are the protocol's verification mechanism, allowing
//! participants to dispute attestations during the verification window.

use serde::{Deserialize, Serialize};

use crate::u256::U256;

/// The status of a challenge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChallengeStatus {
    /// Challenge filed, awaiting response or resolution.
    Open,
    /// Contributor has responded with counter-evidence.
    Responded,
    /// Adjudicator ruled the attestation is valid.
    ResolvedValid,
    /// Adjudicator ruled the attestation is invalid.
    ResolvedInvalid,
}

/// The ruling from challenge adjudication.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChallengeRuling {
    /// The attestation is valid (challenge fails).
    Valid,
    /// The attestation is invalid (challenge succeeds).
    Invalid,
}

/// A challenge against an attestation.
///
/// Challenges freeze attestations and require adjudication before
/// the attestation can finalize or be rejected.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Challenge {
    /// Unique challenge identifier (derived from hash of creation tx).
    pub challenge_id: [u8; 20],

    /// The attestation being challenged.
    pub attestation_id: [u8; 20],

    /// The identity filing the challenge.
    pub challenger: [u8; 20],

    /// SHA-256 hash of evidence supporting the challenge.
    pub evidence_hash: [u8; 32],

    /// SHA-256 hash of the contributor's response (if responded).
    pub response_hash: Option<[u8; 32]>,

    /// Identity that posted the challenge bond (None if no bond required).
    pub bond_poster: Option<[u8; 20]>,

    /// Challenge bond amount (0 if no bond required).
    pub bond_amount: U256,

    /// Current status of the challenge.
    pub status: ChallengeStatus,

    /// Block timestamp when the challenge was created.
    pub created_at: u64,
}

impl Challenge {
    /// Check if this challenge is in a terminal state (resolved).
    #[inline]
    pub fn is_resolved(&self) -> bool {
        matches!(
            self.status,
            ChallengeStatus::ResolvedValid | ChallengeStatus::ResolvedInvalid
        )
    }

    /// Check if the challenge can receive a response.
    #[inline]
    pub fn can_respond(&self) -> bool {
        matches!(self.status, ChallengeStatus::Open)
    }

    /// Check if the challenge can be resolved by the adjudicator.
    #[inline]
    pub fn can_resolve(&self) -> bool {
        matches!(
            self.status,
            ChallengeStatus::Open | ChallengeStatus::Responded
        )
    }

    /// Check if a challenge bond was required/posted.
    #[inline]
    pub fn has_bond(&self) -> bool {
        self.bond_poster.is_some() && !self.bond_amount.is_zero()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_challenge() -> Challenge {
        Challenge {
            challenge_id: [1u8; 20],
            attestation_id: [2u8; 20],
            challenger: [3u8; 20],
            evidence_hash: [4u8; 32],
            response_hash: None,
            bond_poster: Some([5u8; 20]),
            bond_amount: U256::from(100u64),
            status: ChallengeStatus::Open,
            created_at: 1000,
        }
    }

    #[test]
    fn test_open_challenge() {
        let challenge = test_challenge();
        assert!(!challenge.is_resolved());
        assert!(challenge.can_respond());
        assert!(challenge.can_resolve());
        assert!(challenge.has_bond());
    }

    #[test]
    fn test_responded_challenge() {
        let mut challenge = test_challenge();
        challenge.status = ChallengeStatus::Responded;
        challenge.response_hash = Some([6u8; 32]);
        assert!(!challenge.is_resolved());
        assert!(!challenge.can_respond());
        assert!(challenge.can_resolve());
    }

    #[test]
    fn test_resolved_valid() {
        let mut challenge = test_challenge();
        challenge.status = ChallengeStatus::ResolvedValid;
        assert!(challenge.is_resolved());
        assert!(!challenge.can_respond());
        assert!(!challenge.can_resolve());
    }

    #[test]
    fn test_resolved_invalid() {
        let mut challenge = test_challenge();
        challenge.status = ChallengeStatus::ResolvedInvalid;
        assert!(challenge.is_resolved());
        assert!(!challenge.can_respond());
        assert!(!challenge.can_resolve());
    }

    #[test]
    fn test_no_bond() {
        let mut challenge = test_challenge();
        challenge.bond_poster = None;
        challenge.bond_amount = U256::zero();
        assert!(!challenge.has_bond());
    }

    #[test]
    fn test_serialization() {
        let challenge = test_challenge();
        let bytes = crate::serialization::serialize(&challenge).unwrap();
        let recovered: Challenge = crate::serialization::deserialize(&bytes).unwrap();
        assert_eq!(challenge, recovered);
    }

    #[test]
    fn test_status_serialization() {
        for status in [
            ChallengeStatus::Open,
            ChallengeStatus::Responded,
            ChallengeStatus::ResolvedValid,
            ChallengeStatus::ResolvedInvalid,
        ] {
            let bytes = crate::serialization::serialize(&status).unwrap();
            let recovered: ChallengeStatus = crate::serialization::deserialize(&bytes).unwrap();
            assert_eq!(status, recovered);
        }
    }

    #[test]
    fn test_ruling_serialization() {
        for ruling in [ChallengeRuling::Valid, ChallengeRuling::Invalid] {
            let bytes = crate::serialization::serialize(&ruling).unwrap();
            let recovered: ChallengeRuling = crate::serialization::deserialize(&bytes).unwrap();
            assert_eq!(ruling, recovered);
        }
    }
}
