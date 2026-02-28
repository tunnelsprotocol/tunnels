//! Chain reorganization utilities.

use crate::error::{ChainError, ChainResult};

use super::state::ChainState;

/// Find the common ancestor (fork point) between two block hashes.
///
/// Returns the hash of the most recent block that is an ancestor of both.
pub fn find_fork_point(
    chain: &ChainState,
    hash_a: &[u8; 32],
    hash_b: &[u8; 32],
) -> ChainResult<[u8; 32]> {
    // Get heights of both blocks
    let block_a = chain.get_block(hash_a).ok_or(ChainError::UnknownPrevBlock { hash: *hash_a })?;
    let block_b = chain.get_block(hash_b).ok_or(ChainError::UnknownPrevBlock { hash: *hash_b })?;

    let mut height_a = block_a.height();
    let mut height_b = block_b.height();
    let mut current_a = *hash_a;
    let mut current_b = *hash_b;

    // Bring both to the same height
    while height_a > height_b {
        let block = chain.get_block(&current_a).ok_or(ChainError::UnknownPrevBlock { hash: current_a })?;
        current_a = block.prev_hash();
        height_a -= 1;
    }

    while height_b > height_a {
        let block = chain.get_block(&current_b).ok_or(ChainError::UnknownPrevBlock { hash: current_b })?;
        current_b = block.prev_hash();
        height_b -= 1;
    }

    // Walk back until we find common ancestor
    while current_a != current_b {
        let block_a = chain.get_block(&current_a).ok_or(ChainError::UnknownPrevBlock { hash: current_a })?;
        let block_b = chain.get_block(&current_b).ok_or(ChainError::UnknownPrevBlock { hash: current_b })?;

        if block_a.block.is_genesis() || block_b.block.is_genesis() {
            // Both should reach genesis
            if current_a == current_b {
                return Ok(current_a);
            }
            // This shouldn't happen in a valid chain
            return Err(ChainError::OrphanBlock { hash: current_a });
        }

        current_a = block_a.prev_hash();
        current_b = block_b.prev_hash();
    }

    Ok(current_a)
}

/// Calculate the blocks to revert and apply for a reorg.
///
/// Returns (blocks_to_revert, blocks_to_apply) where:
/// - blocks_to_revert: blocks from old tip to fork point (in reverse order)
/// - blocks_to_apply: blocks from fork point to new tip (in apply order)
#[allow(dead_code)]
#[allow(clippy::type_complexity)]
pub fn calculate_reorg_path(
    chain: &ChainState,
    old_tip: &[u8; 32],
    new_tip: &[u8; 32],
) -> ChainResult<(Vec<[u8; 32]>, Vec<[u8; 32]>)> {
    let fork_point = find_fork_point(chain, old_tip, new_tip)?;

    // Collect blocks to revert (old_tip to fork_point, exclusive)
    let mut to_revert = Vec::new();
    let mut current = *old_tip;
    while current != fork_point {
        to_revert.push(current);
        let block = chain.get_block(&current).ok_or(ChainError::UnknownPrevBlock { hash: current })?;
        current = block.prev_hash();
    }

    // Collect blocks to apply (fork_point to new_tip, exclusive)
    let mut to_apply = Vec::new();
    let mut current = *new_tip;
    while current != fork_point {
        to_apply.push(current);
        let block = chain.get_block(&current).ok_or(ChainError::UnknownPrevBlock { hash: current })?;
        current = block.prev_hash();
    }

    // Reverse to_apply so blocks are in application order
    to_apply.reverse();

    Ok((to_revert, to_apply))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_fork_point_same_block() {
        let chain = ChainState::new();
        let genesis_hash = chain.genesis_hash();

        let fork_point = find_fork_point(&chain, &genesis_hash, &genesis_hash).unwrap();
        assert_eq!(fork_point, genesis_hash);
    }

    #[test]
    fn test_find_fork_point_genesis() {
        let chain = ChainState::new();
        let genesis_hash = chain.genesis_hash();

        // For a chain with only genesis, fork point is always genesis
        let result = find_fork_point(&chain, &genesis_hash, &genesis_hash);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), genesis_hash);
    }
}
