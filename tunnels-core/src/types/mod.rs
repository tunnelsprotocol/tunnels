//! Core protocol data types.
//!
//! This module contains all the fundamental types used throughout
//! the Tunnels protocol:
//!
//! - Identity and IdentityType (Human/Agent)
//! - Project and RecipientType
//! - Attestation and AttestationStatus
//! - Challenge and ChallengeStatus/ChallengeRuling
//! - Router state types (AccumulatorState, ContributorBalance, etc.)
//! - Denomination type

mod attestation;
mod challenge;
mod denomination;
mod identity;
mod project;
mod router;

pub use attestation::{Attestation, AttestationStatus};
pub use challenge::{Challenge, ChallengeRuling, ChallengeStatus};
pub use denomination::{common as denomination_common, Denomination, denomination_from_str, denomination_to_string};
pub use identity::{Identity, IdentityType};
pub use project::{Project, RecipientType};
pub use router::{AccumulatorState, ContributorBalance, TransactionDifficultyState};
