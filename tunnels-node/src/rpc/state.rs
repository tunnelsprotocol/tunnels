//! Protocol state query RPC methods.
//!
//! These methods provide The Garage integration surface for querying
//! identities, projects, attestations, challenges, and balances.

use std::sync::Arc;

use jsonrpsee::types::ErrorObjectOwned;
use jsonrpsee::RpcModule;
use serde::{Deserialize, Serialize};

use tunnels_core::crypto::sha256;
use tunnels_core::{
    Attestation, AttestationStatus, Challenge, ChallengeStatus, Denomination, Identity,
    IdentityType, Project, U256,
};
use tunnels_state::StateReader;

use super::RpcState;

/// Identity information returned by RPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityInfo {
    pub address: String,
    pub public_key: String,
    pub identity_type: String,
    pub parent: Option<String>,
    pub profile_hash: Option<String>,
    pub active: bool,
}

impl From<&Identity> for IdentityInfo {
    fn from(id: &Identity) -> Self {
        let identity_type = match id.identity_type {
            IdentityType::Human => "human",
            IdentityType::Agent => "agent",
        };
        Self {
            address: hex::encode(id.address),
            public_key: hex::encode(id.public_key.as_bytes()),
            identity_type: identity_type.to_string(),
            parent: id.parent.map(hex::encode),
            profile_hash: id.profile_hash.map(hex::encode),
            active: id.active,
        }
    }
}

/// Project information returned by RPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub project_id: String,
    pub creator: String,
    pub rho: u32,
    pub phi: u32,
    pub gamma: u32,
    pub delta: u32,
}

impl From<&Project> for ProjectInfo {
    fn from(p: &Project) -> Self {
        Self {
            project_id: hex::encode(p.project_id),
            creator: hex::encode(p.creator),
            rho: p.rho,
            phi: p.phi,
            gamma: p.gamma,
            delta: p.delta,
        }
    }
}

/// Attestation information returned by RPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestationInfo {
    pub attestation_id: String,
    pub project_id: String,
    pub contributor: String,
    pub units: u64,
    pub evidence_hash: String,
    pub bond_denomination: String,
    pub created_at: u64,
    pub status: String,
    pub finalized_at: Option<u64>,
}

impl From<&Attestation> for AttestationInfo {
    fn from(a: &Attestation) -> Self {
        let status = match a.status {
            AttestationStatus::Pending => "pending",
            AttestationStatus::Challenged => "challenged",
            AttestationStatus::Finalized => "finalized",
            AttestationStatus::Rejected => "rejected",
        };
        Self {
            attestation_id: hex::encode(a.attestation_id),
            project_id: hex::encode(a.project_id),
            contributor: hex::encode(a.contributor),
            units: a.units,
            evidence_hash: hex::encode(a.evidence_hash),
            bond_denomination: String::from_utf8_lossy(&a.bond_denomination).trim_end_matches('\0').to_string(),
            created_at: a.created_at,
            status: status.to_string(),
            finalized_at: a.finalized_at,
        }
    }
}

/// Challenge information returned by RPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeInfo {
    pub challenge_id: String,
    pub attestation_id: String,
    pub challenger: String,
    pub evidence_hash: String,
    pub created_at: u64,
    pub status: String,
}

impl From<&Challenge> for ChallengeInfo {
    fn from(c: &Challenge) -> Self {
        let status = match c.status {
            ChallengeStatus::Open => "open",
            ChallengeStatus::Responded => "responded",
            ChallengeStatus::ResolvedValid => "resolved_valid",
            ChallengeStatus::ResolvedInvalid => "resolved_invalid",
        };
        Self {
            challenge_id: hex::encode(c.challenge_id),
            attestation_id: hex::encode(c.attestation_id),
            challenger: hex::encode(c.challenger),
            evidence_hash: hex::encode(c.evidence_hash),
            created_at: c.created_at,
            status: status.to_string(),
        }
    }
}

/// Balance information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceInfo {
    pub claimable: String, // U256 as hex string
}

/// Accumulator state information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccumulatorInfo {
    pub total_finalized_units: u64,
    pub reward_per_unit_global: String, // U256 as hex string
}

/// Reserve balance information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReserveInfo {
    pub balance: String, // U256 as hex string
}

/// Register protocol state RPC methods.
pub fn register_methods(module: &mut RpcModule<Arc<RpcState>>) {
    // get_identity - look up an identity by public key
    module
        .register_async_method("get_identity", |params, state, _| async move {
            let pubkey_hex: String = params.one()?;
            let pubkey_bytes = parse_pubkey(&pubkey_hex)?;

            // Derive address from public key
            let address = derive_address_from_bytes(&pubkey_bytes);

            let chain = state.chain.read().await;
            let mut protocol_state = chain.state().clone();

            match protocol_state.get_identity(&address) {
                Some(identity) => Ok::<_, ErrorObjectOwned>(Some(IdentityInfo::from(identity))),
                None => Ok(None),
            }
        })
        .unwrap();

    // get_project - look up a project by ID
    module
        .register_async_method("get_project", |params, state, _| async move {
            let project_id_hex: String = params.one()?;
            let project_id = parse_id(&project_id_hex)?;

            let chain = state.chain.read().await;
            let mut protocol_state = chain.state().clone();

            match protocol_state.get_project(&project_id) {
                Some(project) => Ok::<_, ErrorObjectOwned>(Some(ProjectInfo::from(project))),
                None => Ok(None),
            }
        })
        .unwrap();

    // get_attestation - look up an attestation by ID
    module
        .register_async_method("get_attestation", |params, state, _| async move {
            let attestation_id_hex: String = params.one()?;
            let attestation_id = parse_id(&attestation_id_hex)?;

            let chain = state.chain.read().await;
            let mut protocol_state = chain.state().clone();

            match protocol_state.get_attestation(&attestation_id) {
                Some(attestation) => Ok::<_, ErrorObjectOwned>(Some(AttestationInfo::from(attestation))),
                None => Ok(None),
            }
        })
        .unwrap();

    // get_challenge - look up a challenge by ID
    module
        .register_async_method("get_challenge", |params, state, _| async move {
            let challenge_id_hex: String = params.one()?;
            let challenge_id = parse_id(&challenge_id_hex)?;

            let chain = state.chain.read().await;
            let mut protocol_state = chain.state().clone();

            match protocol_state.get_challenge(&challenge_id) {
                Some(challenge) => Ok::<_, ErrorObjectOwned>(Some(ChallengeInfo::from(challenge))),
                None => Ok(None),
            }
        })
        .unwrap();

    // get_balance - query claimable balance for an identity in a project
    module
        .register_async_method("get_balance", |params, state, _| async move {
            let (identity_hex, project_id_hex): (String, String) = params.parse()?;
            let identity_bytes = parse_pubkey(&identity_hex)?;
            let identity_addr = derive_address_from_bytes(&identity_bytes);
            let _project_id = parse_id(&project_id_hex)?;

            let chain = state.chain.read().await;
            let mut protocol_state = chain.state().clone();

            // Default denomination (could be parameterized)
            let denom = default_denomination();

            let claimable = protocol_state.get_claimable(&identity_addr, &denom);

            Ok::<_, ErrorObjectOwned>(BalanceInfo {
                claimable: format_u256(&claimable),
            })
        })
        .unwrap();

    // get_accumulator - query reward accumulator state for a project
    module
        .register_async_method("get_accumulator", |params, state, _| async move {
            let project_id_hex: String = params.one()?;
            let project_id = parse_id(&project_id_hex)?;

            let chain = state.chain.read().await;
            let mut protocol_state = chain.state().clone();

            let denom = default_denomination();

            match protocol_state.get_accumulator(&project_id, &denom) {
                Some(acc) => Ok::<_, ErrorObjectOwned>(Some(AccumulatorInfo {
                    total_finalized_units: acc.total_finalized_units,
                    reward_per_unit_global: format_u256(&acc.reward_per_unit_global),
                })),
                None => Ok(None),
            }
        })
        .unwrap();

    // get_reserve_balance - query project reserve balance
    module
        .register_async_method("get_reserve_balance", |params, state, _| async move {
            let project_id_hex: String = params.one()?;
            let project_id = parse_id(&project_id_hex)?;

            let chain = state.chain.read().await;
            let mut protocol_state = chain.state().clone();

            let denom = default_denomination();

            // Reserve balance is tracked in the accumulator
            let balance = match protocol_state.get_accumulator(&project_id, &denom) {
                Some(acc) => acc.reserve_balance,
                None => U256::zero(),
            };

            Ok::<_, ErrorObjectOwned>(ReserveInfo {
                balance: format_u256(&balance),
            })
        })
        .unwrap();

    // get_attestations_by_identity - all attestations for an identity
    module
        .register_async_method("get_attestations_by_identity", |params, state, _| async move {
            let pubkey_hex: String = params.one()?;
            let pubkey_bytes = parse_pubkey(&pubkey_hex)?;
            let identity_addr = derive_address_from_bytes(&pubkey_bytes);

            let chain = state.chain.read().await;
            let protocol_state = chain.state();

            let attestations: Vec<AttestationInfo> = protocol_state
                .attestations
                .values()
                .filter(|a| a.contributor == identity_addr)
                .map(AttestationInfo::from)
                .collect();

            Ok::<_, ErrorObjectOwned>(attestations)
        })
        .unwrap();

    // get_attestations_by_project - all attestations for a project
    module
        .register_async_method("get_attestations_by_project", |params, state, _| async move {
            let project_id_hex: String = params.one()?;
            let project_id = parse_id(&project_id_hex)?;

            let chain = state.chain.read().await;
            let protocol_state = chain.state();

            let attestations: Vec<AttestationInfo> = protocol_state
                .attestations
                .values()
                .filter(|a| a.project_id == project_id)
                .map(AttestationInfo::from)
                .collect();

            Ok::<_, ErrorObjectOwned>(attestations)
        })
        .unwrap();

    // get_projects_by_identity - all projects owned by an identity
    module
        .register_async_method("get_projects_by_identity", |params, state, _| async move {
            let pubkey_hex: String = params.one()?;
            let pubkey_bytes = parse_pubkey(&pubkey_hex)?;
            let identity_addr = derive_address_from_bytes(&pubkey_bytes);

            let chain = state.chain.read().await;
            let protocol_state = chain.state();

            let projects: Vec<ProjectInfo> = protocol_state
                .projects
                .values()
                .filter(|p| p.creator == identity_addr)
                .map(ProjectInfo::from)
                .collect();

            Ok::<_, ErrorObjectOwned>(projects)
        })
        .unwrap();
}

/// Parse a hex string into a 20-byte ID.
fn parse_id(hex_str: &str) -> Result<[u8; 20], ErrorObjectOwned> {
    let bytes = hex::decode(hex_str).map_err(|_| {
        ErrorObjectOwned::owned(-32602, "Invalid hex string", None::<()>)
    })?;

    if bytes.len() != 20 {
        return Err(ErrorObjectOwned::owned(
            -32602,
            "ID must be 20 bytes",
            None::<()>,
        ));
    }

    let mut id = [0u8; 20];
    id.copy_from_slice(&bytes);
    Ok(id)
}

/// Parse a hex string into a 32-byte public key.
fn parse_pubkey(hex_str: &str) -> Result<[u8; 32], ErrorObjectOwned> {
    let bytes = hex::decode(hex_str).map_err(|_| {
        ErrorObjectOwned::owned(-32602, "Invalid hex string", None::<()>)
    })?;

    if bytes.len() != 32 {
        return Err(ErrorObjectOwned::owned(
            -32602,
            "Public key must be 32 bytes",
            None::<()>,
        ));
    }

    let mut pk = [0u8; 32];
    pk.copy_from_slice(&bytes);
    Ok(pk)
}

/// Derive address from raw public key bytes.
/// Address is the first 20 bytes of SHA-256(public_key).
fn derive_address_from_bytes(pubkey: &[u8; 32]) -> [u8; 20] {
    let hash = sha256(pubkey);
    let mut address = [0u8; 20];
    address.copy_from_slice(&hash[..20]);
    address
}

/// Get default denomination.
fn default_denomination() -> Denomination {
    *b"USD\0\0\0\0\0"
}

/// Format U256 as hex string.
fn format_u256(value: &U256) -> String {
    format!("0x{:064x}", value)
}
