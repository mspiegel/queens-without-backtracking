//! Independent ground-truth enumerator.
//!
//! Walks every set-partition of the N² grid cells by recursive block
//! assignment, pruning once more than N blocks have been created, and accepts
//! a partition iff every block is 4-connected in the grid. Θ(Bell(N²)) — only
//! practical up to N=3 (Bell(9) ≈ 21k). Used solely as a test oracle.

use num_bigint::BigUint;

pub fn count(n: usize) -> BigUint {
    assert!(n >= 1);
    let total = n * n;
    let mut block_of = vec![0u8; total];
    let mut num_blocks: u8 = 0;
    let mut acc = BigUint::from(0u32);
    recurse(n, 0, &mut block_of, &mut num_blocks, &mut acc);
    acc
}

fn recurse(
    n: usize,
    k: usize,
    block_of: &mut [u8],
    num_blocks: &mut u8,
    acc: &mut BigUint,
) {
    let total = n * n;
    if k == total {
        if *num_blocks as usize == n && all_blocks_connected(n, block_of, *num_blocks) {
            *acc += 1u32;
        }
        return;
    }

    // Prune: number of blocks can never decrease; abort if already > n.
    if *num_blocks as usize > n {
        return;
    }

    // Place cell k into one of the existing blocks.
    for b in 0..*num_blocks {
        block_of[k] = b;
        recurse(n, k + 1, block_of, num_blocks, acc);
    }

    // Or open a new block, if that still leaves room to reach exactly n.
    if (*num_blocks as usize) < n {
        block_of[k] = *num_blocks;
        *num_blocks += 1;
        recurse(n, k + 1, block_of, num_blocks, acc);
        *num_blocks -= 1;
    }
}

fn all_blocks_connected(n: usize, block_of: &[u8], num_blocks: u8) -> bool {
    let total = n * n;
    let mut seen_root = vec![usize::MAX; num_blocks as usize];
    // First cell of each block is its BFS seed.
    for (i, &b) in block_of.iter().enumerate() {
        let bi = b as usize;
        if seen_root[bi] == usize::MAX {
            seen_root[bi] = i;
        }
    }
    for b in 0..num_blocks as usize {
        let root = seen_root[b];
        // BFS from root inside cells labeled b.
        let mut visited = vec![false; total];
        let mut stack = vec![root];
        visited[root] = true;
        let mut count = 1usize;
        while let Some(c) = stack.pop() {
            let r = c / n;
            let col = c % n;
            let neighbors = [
                (r > 0).then(|| c - n),
                (r + 1 < n).then(|| c + n),
                (col > 0).then(|| c - 1),
                (col + 1 < n).then(|| c + 1),
            ];
            for nb in neighbors.into_iter().flatten() {
                if !visited[nb] && block_of[nb] as usize == b {
                    visited[nb] = true;
                    stack.push(nb);
                    count += 1;
                }
            }
        }
        let block_size = block_of.iter().filter(|&&x| x as usize == b).count();
        if count != block_size {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn n1_is_one() {
        assert_eq!(count(1), BigUint::from(1u32));
    }

    #[test]
    fn n2_is_six() {
        assert_eq!(count(2), BigUint::from(6u32));
    }

    #[test]
    fn n3_is_258() {
        assert_eq!(count(3), BigUint::from(258u32));
    }
}
