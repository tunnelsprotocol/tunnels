//! Headers-first sync implementation.

use tunnels_chain::ChainState;
use tunnels_core::block::BlockHeader;

use crate::error::{P2pError, P2pResult};
use crate::protocol::{GetHeadersMessage, HeadersMessage};

/// Maximum number of headers per request.
pub const MAX_HEADERS_PER_REQUEST: usize = 2000;

/// Number of block locator hashes to send.
const LOCATOR_DEPTH: usize = 32;

/// Build a block locator for GetHeaders request.
///
/// Returns hashes at exponentially increasing distances from the tip:
/// - tip, tip-1, tip-2, tip-3, tip-5, tip-9, tip-17, ... genesis
pub fn build_block_locator(chain: &ChainState) -> Vec<[u8; 32]> {
    let mut locator = Vec::with_capacity(LOCATOR_DEPTH);
    let tip_height = chain.height();

    if tip_height == 0 {
        // Only genesis
        locator.push(chain.genesis_hash());
        return locator;
    }

    let mut height = tip_height;
    let mut step = 1u64;

    loop {
        if let Some(block) = chain.get_block_at_height(height) {
            locator.push(block.hash);
        }

        if height == 0 {
            break;
        }

        // Increase step exponentially after first 10
        if locator.len() >= 10 {
            step *= 2;
        }

        height = height.saturating_sub(step);

        // Don't add too many
        if locator.len() >= LOCATOR_DEPTH {
            break;
        }
    }

    // Always include genesis
    if locator.last() != Some(&chain.genesis_hash()) {
        locator.push(chain.genesis_hash());
    }

    locator
}

/// Create a GetHeaders message.
pub fn create_get_headers(chain: &ChainState, stop_hash: [u8; 32]) -> GetHeadersMessage {
    GetHeadersMessage {
        locator_hashes: build_block_locator(chain),
        stop_hash,
    }
}

/// Find the first common block from a locator.
///
/// Returns the hash and height of the first block we have from the locator.
pub fn find_fork_point(chain: &ChainState, locator: &[[u8; 32]]) -> Option<([u8; 32], u64)> {
    for hash in locator {
        if let Some(block) = chain.get_block(hash) {
            return Some((*hash, block.height()));
        }
    }
    None
}

/// Generate headers response for a GetHeaders request.
pub fn respond_to_get_headers(
    chain: &ChainState,
    request: &GetHeadersMessage,
) -> HeadersMessage {
    let mut headers = Vec::new();

    // Find fork point from locator
    let start_height = if let Some((_, height)) = find_fork_point(chain, &request.locator_hashes) {
        height + 1 // Start with the block after the fork point
    } else {
        // No common block found, start from genesis
        0
    };

    // Collect headers from start_height up to stop_hash or max
    for height in start_height..=chain.height() {
        if headers.len() >= MAX_HEADERS_PER_REQUEST {
            break;
        }

        if let Some(block) = chain.get_block_at_height(height) {
            headers.push(block.block.header.clone());

            // Stop if we've reached the stop hash
            if block.hash == request.stop_hash {
                break;
            }
        }
    }

    HeadersMessage { headers }
}

/// Validate received headers form a valid chain extension.
pub fn validate_headers(
    chain: &ChainState,
    headers: &[BlockHeader],
) -> P2pResult<Vec<[u8; 32]>> {
    if headers.is_empty() {
        return Ok(Vec::new());
    }

    let mut validated_hashes = Vec::with_capacity(headers.len());
    let mut prev_hash = headers[0].prev_block_hash;

    // Check that first header connects to a known block
    if !chain.has_block(&prev_hash) {
        return Err(P2pError::SyncError(format!(
            "Headers don't connect to known chain: {:?}",
            &prev_hash[..8]
        )));
    }

    let mut expected_height = chain
        .get_block(&prev_hash)
        .map(|b| b.height() + 1)
        .unwrap_or(0);

    for header in headers {
        // Verify connection to previous
        if header.prev_block_hash != prev_hash {
            return Err(P2pError::SyncError(format!(
                "Header at height {} doesn't connect to previous",
                header.height
            )));
        }

        // Verify height
        if header.height != expected_height {
            return Err(P2pError::SyncError(format!(
                "Header has wrong height: expected {}, got {}",
                expected_height, header.height
            )));
        }

        // Verify proof of work
        if !header.meets_difficulty() {
            return Err(P2pError::SyncError(format!(
                "Header at height {} doesn't meet difficulty",
                header.height
            )));
        }

        let hash = header.hash();
        validated_hashes.push(hash);
        prev_hash = hash;
        expected_height += 1;
    }

    Ok(validated_hashes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_block_locator_genesis_only() {
        let chain = ChainState::new();
        let locator = build_block_locator(&chain);

        assert_eq!(locator.len(), 1);
        assert_eq!(locator[0], chain.genesis_hash());
    }

    #[test]
    fn test_create_get_headers() {
        let chain = ChainState::new();
        let msg = create_get_headers(&chain, [0u8; 32]);

        assert!(!msg.locator_hashes.is_empty());
        assert_eq!(msg.stop_hash, [0u8; 32]);
    }

    #[test]
    fn test_respond_to_get_headers_empty() {
        let chain = ChainState::new();
        let request = GetHeadersMessage {
            locator_hashes: vec![chain.genesis_hash()],
            stop_hash: [0u8; 32],
        };

        let response = respond_to_get_headers(&chain, &request);

        // Should be empty since we only have genesis and locator matches it
        assert!(response.headers.is_empty());
    }

    #[test]
    fn test_validate_headers_empty() {
        let chain = ChainState::new();
        let result = validate_headers(&chain, &[]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }
}
