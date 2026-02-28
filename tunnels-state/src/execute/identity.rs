//! Identity transaction handlers.
//!
//! Handles CreateIdentity, CreateAgent, UpdateProfileHash, and DeactivateAgent.

use tunnels_core::crypto::derive_address;
use tunnels_core::{Identity, IdentityType, PublicKey};

use crate::error::{StateError, StateResult};
use crate::state::StateWriter;
use super::context::ExecutionContext;

/// Execute a CreateIdentity transaction.
///
/// Creates a new Human identity with the given public key.
/// The signer's address is derived from the public key.
///
/// # Validation
/// - Address must not already exist
pub fn execute_create_identity<S: StateWriter>(
    state: &mut S,
    _ctx: &ExecutionContext,
    public_key: &PublicKey,
) -> StateResult<[u8; 20]> {
    let address = derive_address(public_key);

    // Check if identity already exists
    if state.identity_exists(&address) {
        return Err(StateError::IdentityAlreadyExists { address });
    }

    // Create new Human identity
    let identity = Identity::new_human(address, public_key.clone());
    state.insert_identity(identity);

    Ok(address)
}

/// Execute a CreateAgent transaction.
///
/// Creates a new Agent identity linked to a parent Human.
///
/// # Validation
/// - Signer must be the parent identity
/// - Parent must exist and be an active Human
pub fn execute_create_agent<S: StateWriter>(
    state: &mut S,
    _ctx: &ExecutionContext,
    signer_address: &[u8; 20],
    parent: &[u8; 20],
    public_key: &PublicKey,
) -> StateResult<[u8; 20]> {
    // Verify signer is the parent
    if signer_address != parent {
        return Err(StateError::UnauthorizedSigner {
            expected: *parent,
            actual: *signer_address,
        });
    }

    // Verify parent exists
    let parent_identity = state
        .get_identity(parent)
        .ok_or(StateError::ParentNotFound { parent: *parent })?;

    // Verify parent is Human
    if parent_identity.identity_type != IdentityType::Human {
        return Err(StateError::NotHumanIdentity { address: *parent });
    }

    // Verify parent is active
    if !parent_identity.active {
        return Err(StateError::IdentityDeactivated { address: *parent });
    }

    // Derive agent's address
    let address = derive_address(public_key);

    // Check if address already exists
    if state.identity_exists(&address) {
        return Err(StateError::IdentityAlreadyExists { address });
    }

    // Create new Agent identity
    let identity = Identity::new_agent(address, public_key.clone(), *parent);
    state.insert_identity(identity);

    Ok(address)
}

/// Execute an UpdateProfileHash transaction.
///
/// Updates the profile hash for an existing identity.
///
/// # Validation
/// - Signer must be the identity owner
/// - Identity must exist and be active
pub fn execute_update_profile_hash<S: StateWriter>(
    state: &mut S,
    _ctx: &ExecutionContext,
    signer_address: &[u8; 20],
    identity_address: &[u8; 20],
    hash: &[u8; 32],
) -> StateResult<()> {
    // Verify signer is the identity
    if signer_address != identity_address {
        return Err(StateError::UnauthorizedSigner {
            expected: *identity_address,
            actual: *signer_address,
        });
    }

    // Verify identity exists
    let identity = state
        .get_identity(identity_address)
        .ok_or(StateError::IdentityNotFound {
            address: *identity_address,
        })?;

    // Verify identity is active
    if !identity.active {
        return Err(StateError::IdentityDeactivated {
            address: *identity_address,
        });
    }

    // Update profile hash
    state.update_identity(identity_address, |id| {
        id.profile_hash = Some(*hash);
    });

    Ok(())
}

/// Execute a DeactivateAgent transaction.
///
/// Deactivates an agent identity.
///
/// # Validation
/// - Signer must be the agent's parent
/// - Target must be an Agent (not Human)
pub fn execute_deactivate_agent<S: StateWriter>(
    state: &mut S,
    _ctx: &ExecutionContext,
    signer_address: &[u8; 20],
    agent_address: &[u8; 20],
) -> StateResult<()> {
    // Verify agent exists
    let agent = state
        .get_identity(agent_address)
        .ok_or(StateError::IdentityNotFound {
            address: *agent_address,
        })?;

    // Verify target is an Agent
    if agent.identity_type != IdentityType::Agent {
        return Err(StateError::NotAgentIdentity {
            address: *agent_address,
        });
    }

    // Verify signer is the parent
    let parent = agent.parent.ok_or(StateError::ParentNotFound {
        parent: *agent_address,
    })?;

    if signer_address != &parent {
        return Err(StateError::UnauthorizedSigner {
            expected: parent,
            actual: *signer_address,
        });
    }

    // Deactivate the agent
    state.update_identity(agent_address, |id| {
        id.active = false;
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{ProtocolState, StateReader};
    use tunnels_core::KeyPair;

    fn test_context() -> ExecutionContext {
        ExecutionContext::test_context()
    }

    #[test]
    fn test_create_identity() {
        let mut state = ProtocolState::new();
        let ctx = test_context();
        let kp = KeyPair::generate();

        let address = execute_create_identity(&mut state, &ctx, &kp.public_key()).unwrap();

        let identity = state.get_identity(&address).unwrap();
        assert_eq!(identity.identity_type, IdentityType::Human);
        assert!(identity.active);
        assert!(identity.parent.is_none());
    }

    #[test]
    fn test_create_identity_duplicate_fails() {
        let mut state = ProtocolState::new();
        let ctx = test_context();
        let kp = KeyPair::generate();

        execute_create_identity(&mut state, &ctx, &kp.public_key()).unwrap();
        let result = execute_create_identity(&mut state, &ctx, &kp.public_key());

        assert!(matches!(result, Err(StateError::IdentityAlreadyExists { .. })));
    }

    #[test]
    fn test_create_agent() {
        let mut state = ProtocolState::new();
        let ctx = test_context();

        // Create parent Human
        let parent_kp = KeyPair::generate();
        let parent_address = execute_create_identity(&mut state, &ctx, &parent_kp.public_key()).unwrap();

        // Create Agent
        let agent_kp = KeyPair::generate();
        let agent_address = execute_create_agent(
            &mut state,
            &ctx,
            &parent_address,
            &parent_address,
            &agent_kp.public_key(),
        )
        .unwrap();

        let agent = state.get_identity(&agent_address).unwrap();
        assert_eq!(agent.identity_type, IdentityType::Agent);
        assert_eq!(agent.parent, Some(parent_address));
        assert!(agent.active);
    }

    #[test]
    fn test_create_agent_parent_not_human_fails() {
        let mut state = ProtocolState::new();
        let ctx = test_context();

        // Create parent Human
        let parent_kp = KeyPair::generate();
        let parent_address = execute_create_identity(&mut state, &ctx, &parent_kp.public_key()).unwrap();

        // Create first Agent
        let agent1_kp = KeyPair::generate();
        let agent1_address = execute_create_agent(
            &mut state,
            &ctx,
            &parent_address,
            &parent_address,
            &agent1_kp.public_key(),
        )
        .unwrap();

        // Try to create Agent with Agent as parent
        let agent2_kp = KeyPair::generate();
        let result = execute_create_agent(
            &mut state,
            &ctx,
            &agent1_address,
            &agent1_address,
            &agent2_kp.public_key(),
        );

        assert!(matches!(result, Err(StateError::NotHumanIdentity { .. })));
    }

    #[test]
    fn test_create_agent_wrong_signer_fails() {
        let mut state = ProtocolState::new();
        let ctx = test_context();

        // Create parent Human
        let parent_kp = KeyPair::generate();
        let parent_address = execute_create_identity(&mut state, &ctx, &parent_kp.public_key()).unwrap();

        // Create another Human
        let other_kp = KeyPair::generate();
        let other_address = execute_create_identity(&mut state, &ctx, &other_kp.public_key()).unwrap();

        // Try to create Agent with wrong signer
        let agent_kp = KeyPair::generate();
        let result = execute_create_agent(
            &mut state,
            &ctx,
            &other_address, // Wrong signer
            &parent_address,
            &agent_kp.public_key(),
        );

        assert!(matches!(result, Err(StateError::UnauthorizedSigner { .. })));
    }

    #[test]
    fn test_update_profile_hash() {
        let mut state = ProtocolState::new();
        let ctx = test_context();
        let kp = KeyPair::generate();

        let address = execute_create_identity(&mut state, &ctx, &kp.public_key()).unwrap();
        let hash = [0xAB; 32];

        execute_update_profile_hash(&mut state, &ctx, &address, &address, &hash).unwrap();

        let identity = state.get_identity(&address).unwrap();
        assert_eq!(identity.profile_hash, Some(hash));
    }

    #[test]
    fn test_deactivate_agent() {
        let mut state = ProtocolState::new();
        let ctx = test_context();

        // Create parent and agent
        let parent_kp = KeyPair::generate();
        let parent_address = execute_create_identity(&mut state, &ctx, &parent_kp.public_key()).unwrap();

        let agent_kp = KeyPair::generate();
        let agent_address = execute_create_agent(
            &mut state,
            &ctx,
            &parent_address,
            &parent_address,
            &agent_kp.public_key(),
        )
        .unwrap();

        // Verify agent is active
        assert!(state.get_identity(&agent_address).unwrap().active);

        // Deactivate agent
        execute_deactivate_agent(&mut state, &ctx, &parent_address, &agent_address).unwrap();

        // Verify agent is deactivated
        assert!(!state.get_identity(&agent_address).unwrap().active);
    }

    #[test]
    fn test_deactivate_human_fails() {
        let mut state = ProtocolState::new();
        let ctx = test_context();

        let kp = KeyPair::generate();
        let address = execute_create_identity(&mut state, &ctx, &kp.public_key()).unwrap();

        let result = execute_deactivate_agent(&mut state, &ctx, &address, &address);

        assert!(matches!(result, Err(StateError::NotAgentIdentity { .. })));
    }
}
