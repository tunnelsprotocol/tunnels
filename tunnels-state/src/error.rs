//! Error types for state machine operations.

use tunnels_core::{AttestationStatus, ChallengeStatus, U256};

/// All validation and execution errors for state transitions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StateError {
    // === Identity Errors ===
    /// Address already registered as an identity.
    IdentityAlreadyExists { address: [u8; 20] },
    /// Identity not found at address.
    IdentityNotFound { address: [u8; 20] },
    /// Expected Human identity, found Agent.
    NotHumanIdentity { address: [u8; 20] },
    /// Identity is not an Agent.
    NotAgentIdentity { address: [u8; 20] },
    /// Agent's parent identity not found.
    ParentNotFound { parent: [u8; 20] },
    /// Identity is deactivated.
    IdentityDeactivated { address: [u8; 20] },
    /// Signer is not authorized for this operation.
    UnauthorizedSigner { expected: [u8; 20], actual: [u8; 20] },

    // === Project Errors ===
    /// Project ID already exists.
    ProjectAlreadyExists { project_id: [u8; 20] },
    /// Project not found.
    ProjectNotFound { project_id: [u8; 20] },
    /// Sum of rho + phi + gamma + delta exceeds 10000 basis points.
    InvalidBasisPointsSum { sum: u32 },
    /// Verification window below minimum (86400 seconds / 24 hours).
    VerificationWindowTooShort { window: u64, minimum: u64 },
    /// Challenge bond exceeds attestation bond.
    ChallengeBondExceedsAttestationBond { challenge_bond: u64, attestation_bond: u64 },
    /// Delta specified without predecessor.
    DeltaWithoutPredecessor,
    /// Predecessor specified without delta.
    PredecessorWithoutDelta,
    /// Predecessor project not found.
    PredecessorNotFound { predecessor: [u8; 20] },
    /// Predecessor chain depth exceeds limit (8).
    PredecessorChainTooDeep { depth: u8, limit: u8 },
    /// Gamma recipient chain depth exceeds limit (8).
    GammaChainTooDeep { depth: u8, limit: u8 },
    /// Combined predecessor + gamma depth exceeds limit.
    CombinedChainTooDeep { depth: u8, limit: u8 },
    /// Gamma recipient (project) not found.
    GammaRecipientProjectNotFound { recipient: [u8; 20] },
    /// Gamma recipient (identity) not found.
    GammaRecipientIdentityNotFound { recipient: [u8; 20] },
    /// Phi recipient (identity) not found.
    PhiRecipientNotFound { recipient: [u8; 20] },
    /// Reserve authority not found.
    ReserveAuthorityNotFound { authority: [u8; 20] },
    /// Deposit authority not found.
    DepositAuthorityNotFound { authority: [u8; 20] },
    /// Adjudicator not found.
    AdjudicatorNotFound { adjudicator: [u8; 20] },
    /// Cycle detected in gamma recipient chain.
    GammaCycleDetected { project_id: [u8; 20] },

    // === Attestation Errors ===
    /// Attestation already exists.
    AttestationAlreadyExists { attestation_id: [u8; 20] },
    /// Attestation not found.
    AttestationNotFound { attestation_id: [u8; 20] },
    /// Bond amount below required minimum.
    BondBelowMinimum { provided: U256, required: U256 },
    /// Units must be greater than zero.
    ZeroUnits,
    /// Bond poster not found.
    BondPosterNotFound { bond_poster: [u8; 20] },
    /// Bond poster is deactivated.
    BondPosterDeactivated { bond_poster: [u8; 20] },
    /// Attestation not in Pending status.
    AttestationNotPending {
        attestation_id: [u8; 20],
        status: AttestationStatus,
    },
    /// Verification window not yet expired.
    VerificationWindowNotExpired {
        attestation_id: [u8; 20],
        expires_at: u64,
        current: u64,
    },
    /// Attestation is in terminal state (finalized or rejected).
    AttestationTerminal { attestation_id: [u8; 20] },

    // === Challenge Errors ===
    /// Challenge already exists for this attestation.
    ChallengeAlreadyExists { attestation_id: [u8; 20] },
    /// Challenge not found.
    ChallengeNotFound { challenge_id: [u8; 20] },
    /// Attestation is not in a challengeable state.
    AttestationNotChallengeable { attestation_id: [u8; 20] },
    /// Challenge bond below required minimum.
    ChallengeBondBelowMinimum { provided: U256, required: u64 },
    /// Challenge already resolved.
    ChallengeAlreadyResolved {
        challenge_id: [u8; 20],
        status: ChallengeStatus,
    },
    /// Challenge cannot receive response (not Open).
    ChallengeCannotRespond {
        challenge_id: [u8; 20],
        status: ChallengeStatus,
    },
    /// Signer is not the adjudicator.
    NotAdjudicator {
        adjudicator: [u8; 20],
        signer: [u8; 20],
    },
    /// Verification window expired (cannot challenge).
    VerificationWindowExpired {
        attestation_id: [u8; 20],
        expired_at: u64,
        current: u64,
    },
    /// Challenger cannot be the same as contributor.
    ChallengerIsContributor { identity: [u8; 20] },
    /// Challenge bond poster required but not provided.
    ChallengeBondPosterRequired,

    // === Revenue Errors ===
    /// Not the deposit authority.
    NotDepositAuthority {
        authority: [u8; 20],
        signer: [u8; 20],
    },
    /// Not the reserve authority.
    NotReserveAuthority {
        authority: [u8; 20],
        signer: [u8; 20],
    },
    /// Reserve balance insufficient.
    InsufficientReserve { available: U256, requested: U256 },
    /// No claimable balance.
    NothingToClaim {
        project_id: [u8; 20],
        contributor: [u8; 20],
    },
    /// Deposit amount is zero.
    ZeroDeposit,
    /// Claim amount is zero.
    ZeroClaim,

    // === Signature/PoW Errors ===
    /// Signature verification failed.
    InvalidSignature,
    /// PoW does not meet difficulty.
    InvalidProofOfWork,

    // === General Errors ===
    /// Arithmetic overflow in calculation.
    ArithmeticOverflow,
}

impl std::fmt::Display for StateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // Identity errors
            StateError::IdentityAlreadyExists { address } => {
                write!(f, "identity already exists: {:?}", &address[..4])
            }
            StateError::IdentityNotFound { address } => {
                write!(f, "identity not found: {:?}", &address[..4])
            }
            StateError::NotHumanIdentity { address } => {
                write!(f, "not a human identity: {:?}", &address[..4])
            }
            StateError::NotAgentIdentity { address } => {
                write!(f, "not an agent identity: {:?}", &address[..4])
            }
            StateError::ParentNotFound { parent } => {
                write!(f, "parent identity not found: {:?}", &parent[..4])
            }
            StateError::IdentityDeactivated { address } => {
                write!(f, "identity is deactivated: {:?}", &address[..4])
            }
            StateError::UnauthorizedSigner { expected, actual } => {
                write!(
                    f,
                    "unauthorized signer: expected {:?}, got {:?}",
                    &expected[..4],
                    &actual[..4]
                )
            }

            // Project errors
            StateError::ProjectAlreadyExists { project_id } => {
                write!(f, "project already exists: {:?}", &project_id[..4])
            }
            StateError::ProjectNotFound { project_id } => {
                write!(f, "project not found: {:?}", &project_id[..4])
            }
            StateError::InvalidBasisPointsSum { sum } => {
                write!(f, "basis points sum {} exceeds 10000", sum)
            }
            StateError::VerificationWindowTooShort { window, minimum } => {
                write!(
                    f,
                    "verification window {} is below minimum {}",
                    window, minimum
                )
            }
            StateError::ChallengeBondExceedsAttestationBond { challenge_bond, attestation_bond } => {
                write!(f, "challenge_bond {} cannot exceed attestation_bond {}", challenge_bond, attestation_bond)
            }
            StateError::DeltaWithoutPredecessor => {
                write!(f, "delta specified without predecessor")
            }
            StateError::PredecessorWithoutDelta => {
                write!(f, "predecessor specified without delta")
            }
            StateError::PredecessorNotFound { predecessor } => {
                write!(f, "predecessor not found: {:?}", &predecessor[..4])
            }
            StateError::PredecessorChainTooDeep { depth, limit } => {
                write!(f, "predecessor chain depth {} exceeds limit {}", depth, limit)
            }
            StateError::GammaChainTooDeep { depth, limit } => {
                write!(f, "gamma chain depth {} exceeds limit {}", depth, limit)
            }
            StateError::CombinedChainTooDeep { depth, limit } => {
                write!(f, "combined chain depth {} exceeds limit {}", depth, limit)
            }
            StateError::GammaRecipientProjectNotFound { recipient } => {
                write!(f, "gamma recipient project not found: {:?}", &recipient[..4])
            }
            StateError::GammaRecipientIdentityNotFound { recipient } => {
                write!(f, "gamma recipient identity not found: {:?}", &recipient[..4])
            }
            StateError::PhiRecipientNotFound { recipient } => {
                write!(f, "phi recipient not found: {:?}", &recipient[..4])
            }
            StateError::ReserveAuthorityNotFound { authority } => {
                write!(f, "reserve authority not found: {:?}", &authority[..4])
            }
            StateError::DepositAuthorityNotFound { authority } => {
                write!(f, "deposit authority not found: {:?}", &authority[..4])
            }
            StateError::AdjudicatorNotFound { adjudicator } => {
                write!(f, "adjudicator not found: {:?}", &adjudicator[..4])
            }
            StateError::GammaCycleDetected { project_id } => {
                write!(f, "gamma cycle detected at project: {:?}", &project_id[..4])
            }

            // Attestation errors
            StateError::AttestationAlreadyExists { attestation_id } => {
                write!(f, "attestation already exists: {:?}", &attestation_id[..4])
            }
            StateError::AttestationNotFound { attestation_id } => {
                write!(f, "attestation not found: {:?}", &attestation_id[..4])
            }
            StateError::BondBelowMinimum { provided, required } => {
                write!(f, "bond {:?} below minimum {:?}", provided, required)
            }
            StateError::ZeroUnits => write!(f, "units must be greater than zero"),
            StateError::BondPosterNotFound { bond_poster } => {
                write!(f, "bond poster not found: {:?}", &bond_poster[..4])
            }
            StateError::BondPosterDeactivated { bond_poster } => {
                write!(f, "bond poster deactivated: {:?}", &bond_poster[..4])
            }
            StateError::AttestationNotPending {
                attestation_id,
                status,
            } => {
                write!(
                    f,
                    "attestation {:?} not pending, status: {:?}",
                    &attestation_id[..4],
                    status
                )
            }
            StateError::VerificationWindowNotExpired {
                attestation_id,
                expires_at,
                current,
            } => {
                write!(
                    f,
                    "verification window for {:?} not expired (expires {}, now {})",
                    &attestation_id[..4],
                    expires_at,
                    current
                )
            }
            StateError::AttestationTerminal { attestation_id } => {
                write!(f, "attestation {:?} is in terminal state", &attestation_id[..4])
            }

            // Challenge errors
            StateError::ChallengeAlreadyExists { attestation_id } => {
                write!(
                    f,
                    "challenge already exists for attestation {:?}",
                    &attestation_id[..4]
                )
            }
            StateError::ChallengeNotFound { challenge_id } => {
                write!(f, "challenge not found: {:?}", &challenge_id[..4])
            }
            StateError::AttestationNotChallengeable { attestation_id } => {
                write!(
                    f,
                    "attestation {:?} not challengeable",
                    &attestation_id[..4]
                )
            }
            StateError::ChallengeBondBelowMinimum { provided, required } => {
                write!(
                    f,
                    "challenge bond {:?} below minimum {}",
                    provided, required
                )
            }
            StateError::ChallengeAlreadyResolved {
                challenge_id,
                status,
            } => {
                write!(
                    f,
                    "challenge {:?} already resolved: {:?}",
                    &challenge_id[..4],
                    status
                )
            }
            StateError::ChallengeCannotRespond {
                challenge_id,
                status,
            } => {
                write!(
                    f,
                    "challenge {:?} cannot receive response, status: {:?}",
                    &challenge_id[..4],
                    status
                )
            }
            StateError::NotAdjudicator { adjudicator, signer } => {
                write!(
                    f,
                    "not adjudicator: expected {:?}, got {:?}",
                    &adjudicator[..4],
                    &signer[..4]
                )
            }
            StateError::VerificationWindowExpired {
                attestation_id,
                expired_at,
                current,
            } => {
                write!(
                    f,
                    "verification window for {:?} expired at {} (now {})",
                    &attestation_id[..4],
                    expired_at,
                    current
                )
            }
            StateError::ChallengerIsContributor { identity } => {
                write!(f, "challenger is contributor: {:?}", &identity[..4])
            }
            StateError::ChallengeBondPosterRequired => {
                write!(f, "challenge bond poster required")
            }

            // Revenue errors
            StateError::NotDepositAuthority { authority, signer } => {
                write!(
                    f,
                    "not deposit authority: expected {:?}, got {:?}",
                    &authority[..4],
                    &signer[..4]
                )
            }
            StateError::NotReserveAuthority { authority, signer } => {
                write!(
                    f,
                    "not reserve authority: expected {:?}, got {:?}",
                    &authority[..4],
                    &signer[..4]
                )
            }
            StateError::InsufficientReserve { available, requested } => {
                write!(
                    f,
                    "insufficient reserve: available {:?}, requested {:?}",
                    available, requested
                )
            }
            StateError::NothingToClaim {
                project_id,
                contributor,
            } => {
                write!(
                    f,
                    "nothing to claim: project {:?}, contributor {:?}",
                    &project_id[..4],
                    &contributor[..4]
                )
            }
            StateError::ZeroDeposit => write!(f, "deposit amount is zero"),
            StateError::ZeroClaim => write!(f, "claim amount is zero"),

            // Signature/PoW errors
            StateError::InvalidSignature => write!(f, "invalid signature"),
            StateError::InvalidProofOfWork => write!(f, "invalid proof of work"),

            // General errors
            StateError::ArithmeticOverflow => write!(f, "arithmetic overflow"),
        }
    }
}

impl std::error::Error for StateError {}

/// Result type for state operations.
pub type StateResult<T> = Result<T, StateError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = StateError::IdentityNotFound {
            address: [0u8; 20],
        };
        assert!(err.to_string().contains("identity not found"));
    }

    #[test]
    fn test_error_clone() {
        let err = StateError::ZeroUnits;
        let cloned = err.clone();
        assert_eq!(err, cloned);
    }
}
