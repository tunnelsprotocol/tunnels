//! Bond management operations.
//!
//! Handles locking, returning, and forfeiting bonds for attestations
//! and challenges.

use tunnels_core::{Denomination, U256};

use crate::error::{StateError, StateResult};
use crate::state::StateWriter;

/// Lock a bond for an attestation.
///
/// Called when SubmitAttestation is executed.
pub fn lock_attestation_bond<S: StateWriter>(
    state: &mut S,
    attestation_id: &[u8; 20],
    _bond_poster: &[u8; 20],
    amount: U256,
    _denom: &Denomination,
    min_bond: U256,
) -> StateResult<()> {
    // Validate bond >= minimum
    if amount < min_bond {
        return Err(StateError::BondBelowMinimum {
            provided: amount,
            required: min_bond,
        });
    }

    // Lock the bond
    state.lock_bond(attestation_id, amount);

    Ok(())
}

/// Return an attestation bond on successful finalization.
///
/// Returns the full bond amount to the bond poster.
pub fn return_attestation_bond<S: StateWriter>(
    state: &mut S,
    attestation_id: &[u8; 20],
    bond_poster: &[u8; 20],
    denom: &Denomination,
) -> StateResult<U256> {
    // Unlock the bond
    let locked = state
        .unlock_bond(attestation_id)
        .ok_or(StateError::AttestationNotFound {
            attestation_id: *attestation_id,
        })?;

    // Credit full bond to bond poster
    if !locked.is_zero() {
        state.credit_claimable(bond_poster, denom, locked);
    }

    Ok(locked)
}

/// Forfeit an attestation bond to the challenger on invalid ruling.
///
/// Called when a challenge is resolved as Invalid.
pub fn forfeit_attestation_bond_to_challenger<S: StateWriter>(
    state: &mut S,
    attestation_id: &[u8; 20],
    challenger: &[u8; 20],
    denom: &Denomination,
) -> StateResult<U256> {
    // Unlock the bond
    let locked = state
        .unlock_bond(attestation_id)
        .ok_or(StateError::AttestationNotFound {
            attestation_id: *attestation_id,
        })?;

    // Transfer full bond to challenger (no burn on forfeit)
    state.credit_claimable(challenger, denom, locked);

    Ok(locked)
}

/// Lock a challenge bond.
///
/// Called when ChallengeAttestation is executed (if project requires a challenge bond).
pub fn lock_challenge_bond<S: StateWriter>(
    state: &mut S,
    challenge_id: &[u8; 20],
    _bond_poster: &[u8; 20],
    amount: U256,
    _denom: &Denomination,
    required: u64,
) -> StateResult<()> {
    // Validate bond >= required
    if amount < U256::from(required) {
        return Err(StateError::ChallengeBondBelowMinimum {
            provided: amount,
            required,
        });
    }

    // Lock the challenge bond
    state.lock_challenge_bond(challenge_id, amount);

    Ok(())
}

/// Return a challenge bond when challenge succeeds (ruling is Invalid).
///
/// The challenge bond is returned in full (no burn).
pub fn return_challenge_bond<S: StateWriter>(
    state: &mut S,
    challenge_id: &[u8; 20],
    challenger: &[u8; 20],
    denom: &Denomination,
) -> StateResult<U256> {
    // Unlock the challenge bond
    let locked = state
        .unlock_challenge_bond(challenge_id)
        .ok_or(StateError::ChallengeNotFound {
            challenge_id: *challenge_id,
        })?;

    // Return full bond to challenger
    state.credit_claimable(challenger, denom, locked);

    Ok(locked)
}

/// Forfeit a challenge bond to the attestation bond poster when challenge fails.
///
/// Called when a challenge is resolved as Valid.
pub fn forfeit_challenge_bond_to_attestor<S: StateWriter>(
    state: &mut S,
    challenge_id: &[u8; 20],
    attestor_bond_poster: &[u8; 20],
    denom: &Denomination,
) -> StateResult<U256> {
    // Unlock the challenge bond
    let locked = state.unlock_challenge_bond(challenge_id);

    // If no challenge bond was required, this is fine
    let locked = match locked {
        Some(amount) => amount,
        None => return Ok(U256::zero()),
    };

    // Transfer full bond to attestor's bond poster
    state.credit_claimable(attestor_bond_poster, denom, locked);

    Ok(locked)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{ProtocolState, StateReader, StateWriter};

    fn test_denom() -> Denomination {
        *b"USD\0\0\0\0\0"
    }

    #[test]
    fn test_lock_attestation_bond() {
        let mut state = ProtocolState::new();
        let attestation_id = [1u8; 20];
        let bond_poster = [2u8; 20];
        let amount = U256::from(1000u64);
        let min_bond = U256::from(100u64);

        lock_attestation_bond(
            &mut state,
            &attestation_id,
            &bond_poster,
            amount,
            &test_denom(),
            min_bond,
        )
        .unwrap();

        assert_eq!(state.get_locked_bond(&attestation_id), Some(amount));
    }

    #[test]
    fn test_lock_attestation_bond_below_minimum() {
        let mut state = ProtocolState::new();
        let attestation_id = [1u8; 20];
        let bond_poster = [2u8; 20];
        let amount = U256::from(50u64);
        let min_bond = U256::from(100u64);

        let result = lock_attestation_bond(
            &mut state,
            &attestation_id,
            &bond_poster,
            amount,
            &test_denom(),
            min_bond,
        );

        assert!(matches!(result, Err(StateError::BondBelowMinimum { .. })));
    }

    #[test]
    fn test_return_attestation_bond() {
        let mut state = ProtocolState::new();
        let attestation_id = [1u8; 20];
        let bond_poster = [2u8; 20];
        let amount = U256::from(1000u64);
        let denom = test_denom();

        // Lock the bond first
        state.lock_bond(&attestation_id, amount);

        // Return it (full return)
        let returned = return_attestation_bond(
            &mut state,
            &attestation_id,
            &bond_poster,
            &denom,
        )
        .unwrap();

        assert_eq!(returned, U256::from(1000u64));
        assert_eq!(state.get_claimable(&bond_poster, &denom), U256::from(1000u64));
        assert!(state.get_locked_bond(&attestation_id).is_none());
    }

    #[test]
    fn test_forfeit_attestation_bond() {
        let mut state = ProtocolState::new();
        let attestation_id = [1u8; 20];
        let challenger = [3u8; 20];
        let amount = U256::from(1000u64);
        let denom = test_denom();

        // Lock the bond first
        state.lock_bond(&attestation_id, amount);

        // Forfeit it
        let forfeited = forfeit_attestation_bond_to_challenger(
            &mut state,
            &attestation_id,
            &challenger,
            &denom,
        )
        .unwrap();

        assert_eq!(forfeited, amount);
        assert_eq!(state.get_claimable(&challenger, &denom), amount);
        assert!(state.get_locked_bond(&attestation_id).is_none());
    }

    #[test]
    fn test_challenge_bond_lifecycle() {
        let mut state = ProtocolState::new();
        let challenge_id = [1u8; 20];
        let challenger = [2u8; 20];
        let amount = U256::from(500u64);
        let denom = test_denom();

        // Lock challenge bond
        lock_challenge_bond(
            &mut state,
            &challenge_id,
            &challenger,
            amount,
            &denom,
            100,
        )
        .unwrap();

        assert_eq!(state.get_challenge_bond(&challenge_id), Some(amount));

        // Return it (challenge succeeded)
        let returned = return_challenge_bond(
            &mut state,
            &challenge_id,
            &challenger,
            &denom,
        )
        .unwrap();

        assert_eq!(returned, amount);
        assert_eq!(state.get_claimable(&challenger, &denom), amount);
    }

    #[test]
    fn test_forfeit_challenge_bond() {
        let mut state = ProtocolState::new();
        let challenge_id = [1u8; 20];
        let attestor = [2u8; 20];
        let amount = U256::from(500u64);
        let denom = test_denom();

        // Lock challenge bond
        state.lock_challenge_bond(&challenge_id, amount);

        // Forfeit it (challenge failed)
        let forfeited = forfeit_challenge_bond_to_attestor(
            &mut state,
            &challenge_id,
            &attestor,
            &denom,
        )
        .unwrap();

        assert_eq!(forfeited, amount);
        assert_eq!(state.get_claimable(&attestor, &denom), amount);
    }
}
