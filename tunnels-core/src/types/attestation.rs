//! Attestation types for the Tunnels protocol.
//!
//! An attestation is the protocol's atomic unit - a cryptographic record
//! that a contribution occurred, backed by an economic commitment (bond).

use serde::{Deserialize, Serialize};

use super::Denomination;
use crate::u256::U256;

/// The status of an attestation in its lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AttestationStatus {
    /// Within verification window, not yet challenged.
    Pending,
    /// A challenge has been filed; attestation is frozen.
    Challenged,
    /// Verification window closed without challenge, or challenge resolved in favor.
    /// The attestation is now immutable and counts toward distribution.
    Finalized,
    /// Challenge resolved against the attestation.
    /// The attestation is invalidated and its weight never enters distribution.
    Rejected,
}

/// A contribution attestation.
///
/// Attestations are the fundamental records of work in the Tunnels protocol.
/// Each attestation:
/// - Links a contributor identity to a project
/// - Specifies a unit weight for distribution
/// - References off-chain evidence via hash
/// - Is backed by an economic bond
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attestation {
    /// Unique attestation identifier (derived from hash of creation tx).
    pub attestation_id: [u8; 20],

    /// The project this attestation belongs to.
    pub project_id: [u8; 20],

    /// The contributor who did the work and signed the transaction.
    /// The contributor and signer are always the same identity.
    pub contributor: [u8; 20],

    /// Contribution weight (must be > 0).
    /// Determines the contributor's share of future distributions.
    pub units: u64,

    /// SHA-256 hash of off-chain evidence/proof artifact.
    pub evidence_hash: [u8; 32],

    /// Identity that posted the bond (may be contributor or third party).
    pub bond_poster: [u8; 20],

    /// Amount of bond locked (must be >= project minimum bond).
    pub bond_amount: U256,

    /// Denomination of the bond.
    pub bond_denomination: Denomination,

    /// Current status in the attestation lifecycle.
    pub status: AttestationStatus,

    /// Block timestamp when the attestation was created.
    pub created_at: u64,

    /// Block timestamp when the attestation was finalized (None if not yet finalized).
    pub finalized_at: Option<u64>,
}

impl Attestation {
    /// Check if this attestation is in a terminal state (Finalized or Rejected).
    #[inline]
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            AttestationStatus::Finalized | AttestationStatus::Rejected
        )
    }

    /// Check if this attestation can be challenged.
    ///
    /// Returns true if status is Pending (not already challenged or finalized).
    #[inline]
    pub fn can_be_challenged(&self) -> bool {
        matches!(self.status, AttestationStatus::Pending)
    }

    /// Check if this attestation can be finalized.
    ///
    /// Returns true if status is Pending (unchallenged attestations only).
    #[inline]
    pub fn can_be_finalized(&self) -> bool {
        matches!(self.status, AttestationStatus::Pending)
    }

    /// Check if the verification window has expired.
    ///
    /// Returns true if `current_timestamp >= created_at + verification_window`.
    pub fn verification_window_expired(
        &self,
        current_timestamp: u64,
        verification_window: u64,
    ) -> bool {
        current_timestamp >= self.created_at.saturating_add(verification_window)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::denomination::common::USD;

    fn test_attestation() -> Attestation {
        Attestation {
            attestation_id: [1u8; 20],
            project_id: [2u8; 20],
            contributor: [3u8; 20],
            units: 100,
            evidence_hash: [4u8; 32],
            bond_poster: [5u8; 20],
            bond_amount: U256::from(1000u64),
            bond_denomination: USD,
            status: AttestationStatus::Pending,
            created_at: 1000,
            finalized_at: None,
        }
    }

    #[test]
    fn test_pending_attestation() {
        let att = test_attestation();
        assert!(!att.is_terminal());
        assert!(att.can_be_challenged());
        assert!(att.can_be_finalized());
    }

    #[test]
    fn test_challenged_attestation() {
        let mut att = test_attestation();
        att.status = AttestationStatus::Challenged;
        assert!(!att.is_terminal());
        assert!(!att.can_be_challenged());
        assert!(!att.can_be_finalized());
    }

    #[test]
    fn test_finalized_attestation() {
        let mut att = test_attestation();
        att.status = AttestationStatus::Finalized;
        att.finalized_at = Some(2000);
        assert!(att.is_terminal());
        assert!(!att.can_be_challenged());
        assert!(!att.can_be_finalized());
    }

    #[test]
    fn test_rejected_attestation() {
        let mut att = test_attestation();
        att.status = AttestationStatus::Rejected;
        assert!(att.is_terminal());
        assert!(!att.can_be_challenged());
        assert!(!att.can_be_finalized());
    }

    #[test]
    fn test_verification_window() {
        let att = test_attestation(); // created_at = 1000
        let window = 604_800; // 7 days

        // Before window expires
        assert!(!att.verification_window_expired(1000 + 604_799, window));

        // Exactly at expiration
        assert!(att.verification_window_expired(1000 + 604_800, window));

        // After expiration
        assert!(att.verification_window_expired(1000 + 700_000, window));
    }

    #[test]
    fn test_serialization() {
        let att = test_attestation();
        let bytes = crate::serialization::serialize(&att).unwrap();
        let recovered: Attestation = crate::serialization::deserialize(&bytes).unwrap();
        assert_eq!(att, recovered);
    }

    #[test]
    fn test_status_serialization() {
        for status in [
            AttestationStatus::Pending,
            AttestationStatus::Challenged,
            AttestationStatus::Finalized,
            AttestationStatus::Rejected,
        ] {
            let bytes = crate::serialization::serialize(&status).unwrap();
            let recovered: AttestationStatus = crate::serialization::deserialize(&bytes).unwrap();
            assert_eq!(status, recovered);
        }
    }
}
