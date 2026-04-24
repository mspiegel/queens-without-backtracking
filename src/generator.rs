//! Random generator for LinkedIn Queens boards with exactly one valid
//! solution. See `QUEENS.md` for the game rules and file format. The
//! `queens-generator` binary at `src/bin/generator.rs` is a thin CLI over
//! the functions in this module.
//!
//! Pipeline per board:
//!
//! 1. Sample a valid queen placement: random column permutation,
//!    rejecting any whose consecutive rows are king-adjacent.
//! 2. Grow N connected regions from the N queens by randomized
//!    multi-source flood fill.
//! 3. Verify uniqueness with a u16-bitmask row-by-row backtracker that
//!    early-exits at the second solution. If not unique, discard and
//!    retry from step 1.

use std::fmt;

use rand::seq::SliceRandom;
use rand::Rng;

/// Hard cap on outer attempts per board (each attempt = one fresh
/// sample of queens and initial regions, followed by surgery). If this
/// is exhausted, generation fails. In practice algorithm A succeeds
/// within a handful of outer attempts for N ≤ 11.
const MAX_ATTEMPTS_PER_BOARD: usize = 1_000;

/// Hard cap on surgery steps per outer attempt. If surgery can't drive
/// the solution count down to 1 within this budget, the outer loop
/// restarts with fresh queens and regions.
const MAX_SURGERY_STEPS: usize = 500;

/// Maximum N the backtracker supports. Limited by the u16 column and
/// region bitmasks.
pub const MAX_N: usize = 16;

#[derive(Debug)]
pub enum GenError {
    ExhaustedAttempts,
    TooSmall(usize),
    TooLarge(usize),
}

impl fmt::Display for GenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GenError::ExhaustedAttempts => write!(
                f,
                "failed to produce a uniquely solvable board within {MAX_ATTEMPTS_PER_BOARD} attempts"
            ),
            GenError::TooSmall(n) => {
                write!(f, "N={n} is too small to admit a valid queen placement")
            }
            GenError::TooLarge(n) => {
                write!(f, "N={n} exceeds the backtracker limit of {MAX_N}")
            }
        }
    }
}

impl std::error::Error for GenError {}

/// Generate a random N×N region grid with exactly one valid queens
/// solution. Returns regions as a row-major grid of region letters
/// (`A`, `B`, ...).
///
/// Algorithm (A):
/// 1. Sample a valid queen placement `P` (random permutation with
///    consecutive rows king-non-adjacent).
/// 2. Grow initial regions by multi-source flood fill from `P`. The
///    result always admits `P` as at least one solution.
/// 3. Enumerate valid solutions up to cap 2. If exactly one, done.
///    Otherwise pick an alternative solution and apply a single-cell
///    surgery that invalidates it while preserving `P` and region
///    connectivity. Repeat.
/// 4. If surgery cannot make progress within a bounded budget, throw
///    the board away and restart from step 1.
pub fn generate_board(n: usize, rng: &mut impl Rng) -> Result<Vec<Vec<u8>>, GenError> {
    if n > MAX_N {
        return Err(GenError::TooLarge(n));
    }
    for _ in 0..MAX_ATTEMPTS_PER_BOARD {
        let Some(seed) = sample_queens(n, rng) else {
            return Err(GenError::TooSmall(n));
        };
        let mut regions = grow_regions(n, &seed, rng);

        let mut made_progress = true;
        for _ in 0..MAX_SURGERY_STEPS {
            let sols = enumerate_solutions(n, &regions, 2);
            if sols.len() <= 1 {
                break;
            }
            let alt = sols.into_iter().find(|s| s != &seed).expect("alt exists");
            if !block_alternative(&mut regions, &seed, &alt, rng) {
                made_progress = false;
                break;
            }
        }
        if made_progress && count_solutions(n, &regions, 2) == 1 {
            return Ok(regions);
        }
    }
    Err(GenError::ExhaustedAttempts)
}

/// Sample a random column assignment row→col with no two consecutive
/// rows king-adjacent. Returns `None` if no placement can exist (N ≤ 3).
fn sample_queens(n: usize, rng: &mut impl Rng) -> Option<Vec<usize>> {
    if n == 0 {
        return Some(Vec::new());
    }
    if n == 1 {
        return Some(vec![0]);
    }
    if n < 4 {
        return None;
    }
    const INNER_ATTEMPTS: usize = 10_000;
    for _ in 0..INNER_ATTEMPTS {
        let mut cols: Vec<usize> = (0..n).collect();
        cols.shuffle(rng);
        let ok = (0..n - 1).all(|r| {
            let a = cols[r] as isize;
            let b = cols[r + 1] as isize;
            (a - b).abs() > 1
        });
        if ok {
            return Some(cols);
        }
    }
    None
}

/// Grow N connected regions from the N queen cells by randomized
/// multi-source flood fill. Each step picks a random frontier cell
/// (unassigned, adjacent to ≥1 assigned cell), then assigns it the
/// region of a random already-assigned 4-neighbor. Each region stays
/// 4-connected because cells only join by touching that region.
fn grow_regions(n: usize, queens: &[usize], rng: &mut impl Rng) -> Vec<Vec<u8>> {
    let mut regions = vec![vec![0u8; n]; n];
    let mut assigned = vec![vec![false; n]; n];
    let mut in_frontier = vec![vec![false; n]; n];
    let mut frontier: Vec<(usize, usize)> = Vec::new();

    for r in 0..n {
        let c = queens[r];
        regions[r][c] = b'A' + r as u8;
        assigned[r][c] = true;
        push_neighbors(n, r, c, &assigned, &mut in_frontier, &mut frontier);
    }

    let total = n * n;
    let mut placed = n;
    let mut picks: Vec<u8> = Vec::with_capacity(4);
    while placed < total {
        let idx = rng.gen_range(0..frontier.len());
        let (r, c) = frontier.swap_remove(idx);
        in_frontier[r][c] = false;

        picks.clear();
        for (dr, dc) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
            let nr = r as i32 + dr;
            let nc = c as i32 + dc;
            if nr < 0 || nc < 0 || nr >= n as i32 || nc >= n as i32 {
                continue;
            }
            let (nr, nc) = (nr as usize, nc as usize);
            if assigned[nr][nc] {
                picks.push(regions[nr][nc]);
            }
        }
        let pick = picks[rng.gen_range(0..picks.len())];
        regions[r][c] = pick;
        assigned[r][c] = true;
        placed += 1;
        push_neighbors(n, r, c, &assigned, &mut in_frontier, &mut frontier);
    }
    regions
}

fn push_neighbors(
    n: usize,
    r: usize,
    c: usize,
    assigned: &[Vec<bool>],
    in_frontier: &mut [Vec<bool>],
    frontier: &mut Vec<(usize, usize)>,
) {
    for (dr, dc) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
        let nr = r as i32 + dr;
        let nc = c as i32 + dc;
        if nr < 0 || nc < 0 || nr >= n as i32 || nc >= n as i32 {
            continue;
        }
        let (nr, nc) = (nr as usize, nc as usize);
        if !assigned[nr][nc] && !in_frontier[nr][nc] {
            in_frontier[nr][nc] = true;
            frontier.push((nr, nc));
        }
    }
}

/// Count valid solutions for `regions`, early-exiting at `cap`. Uses
/// u16 bitmasks for column and region occupancy (so `n ≤ MAX_N = 16`).
pub fn count_solutions(n: usize, regions: &[Vec<u8>], cap: usize) -> usize {
    enumerate_solutions(n, regions, cap).len()
}

/// Enumerate up to `cap` valid solutions for `regions`. Each solution
/// is a `Vec<usize>` of length `n` whose `r`-th entry is the queen's
/// column in row `r`. Uses u16 bitmasks (so `n ≤ MAX_N = 16`).
pub fn enumerate_solutions(n: usize, regions: &[Vec<u8>], cap: usize) -> Vec<Vec<usize>> {
    assert!(n <= MAX_N, "enumerate_solutions supports n ≤ {MAX_N}");

    let mut letter_to_index = [u8::MAX; 256];
    let mut region_of = vec![vec![0u8; n]; n];
    let mut next = 0u8;
    for r in 0..n {
        for c in 0..n {
            let letter = regions[r][c];
            let slot = &mut letter_to_index[letter as usize];
            if *slot == u8::MAX {
                *slot = next;
                next += 1;
            }
            region_of[r][c] = *slot;
        }
    }

    let full_cols: u16 = if n == 16 {
        u16::MAX
    } else {
        (1u16 << n).wrapping_sub(1)
    };
    let mut out: Vec<Vec<usize>> = Vec::new();
    let mut current: Vec<usize> = Vec::with_capacity(n);
    backtrack(0, n, full_cols, &region_of, 0, 0, 0, &mut current, &mut out, cap);
    out
}

fn backtrack(
    row: usize,
    n: usize,
    full_cols: u16,
    region_of: &[Vec<u8>],
    cols_used: u16,
    regs_used: u16,
    col_ban: u16,
    current: &mut Vec<usize>,
    out: &mut Vec<Vec<usize>>,
    cap: usize,
) {
    if out.len() >= cap {
        return;
    }
    if row == n {
        out.push(current.clone());
        return;
    }
    let mut available = full_cols & !cols_used & !col_ban;
    while available != 0 {
        let c = available.trailing_zeros() as usize;
        available &= available - 1;
        let rbit = 1u16 << region_of[row][c];
        if regs_used & rbit != 0 {
            continue;
        }
        let mut new_ban = 1u16 << c;
        if c > 0 {
            new_ban |= 1u16 << (c - 1);
        }
        if c + 1 < n {
            new_ban |= 1u16 << (c + 1);
        }
        current.push(c);
        backtrack(
            row + 1,
            n,
            full_cols,
            region_of,
            cols_used | (1u16 << c),
            regs_used | rbit,
            new_ban,
            current,
            out,
            cap,
        );
        current.pop();
        if out.len() >= cap {
            return;
        }
    }
}

/// Return true iff every cell of `regions` labeled `letter` forms a
/// single 4-connected component. Used to validate a candidate surgery
/// before committing.
fn is_region_connected(regions: &[Vec<u8>], letter: u8) -> bool {
    let n = regions.len();
    let mut start: Option<(usize, usize)> = None;
    let mut total = 0usize;
    for r in 0..n {
        for c in 0..n {
            if regions[r][c] == letter {
                total += 1;
                if start.is_none() {
                    start = Some((r, c));
                }
            }
        }
    }
    let Some(origin) = start else { return true };

    let mut seen = vec![vec![false; n]; n];
    let mut stack = vec![origin];
    seen[origin.0][origin.1] = true;
    let mut reached = 0usize;
    while let Some((r, c)) = stack.pop() {
        reached += 1;
        for (dr, dc) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
            let nr = r as i32 + dr;
            let nc = c as i32 + dc;
            if nr < 0 || nc < 0 || nr >= n as i32 || nc >= n as i32 {
                continue;
            }
            let (nr, nc) = (nr as usize, nc as usize);
            if regions[nr][nc] == letter && !seen[nr][nc] {
                seen[nr][nc] = true;
                stack.push((nr, nc));
            }
        }
    }
    reached == total
}

/// Try to reassign a single non-queen cell of the regions grid so that
/// the alternative solution `alt` ceases to be valid, while keeping the
/// seed solution `seed` valid and every region still 4-connected and
/// containing its home queen.
///
/// The reassigned cell is `(r, alt[r])` for some row `r` where
/// `alt[r] != seed[r]`. It is moved from its current region into the
/// region of some other queen of `alt`, creating a duplicate-region
/// conflict that invalidates `alt`.
///
/// Returns `true` if a surgery was committed; `false` if none is
/// available for this alternative.
fn block_alternative(
    regions: &mut [Vec<u8>],
    seed: &[usize],
    alt: &[usize],
    rng: &mut impl Rng,
) -> bool {
    let n = regions.len();

    let mut row_order: Vec<usize> = (0..n).filter(|&r| alt[r] != seed[r]).collect();
    row_order.shuffle(rng);

    for r in row_order {
        let c = alt[r];
        let old_letter = regions[r][c];

        let mut neighbor_cells: Vec<(usize, usize)> = Vec::with_capacity(4);
        for (dr, dc) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
            let nr = r as i32 + dr;
            let nc = c as i32 + dc;
            if nr < 0 || nc < 0 || nr >= n as i32 || nc >= n as i32 {
                continue;
            }
            neighbor_cells.push((nr as usize, nc as usize));
        }
        neighbor_cells.shuffle(rng);

        for (nr, nc) in neighbor_cells {
            let new_letter = regions[nr][nc];
            if new_letter == old_letter {
                continue;
            }
            let hosts_other_alt_queen = (0..n)
                .any(|r2| r2 != r && regions[r2][alt[r2]] == new_letter);
            if !hosts_other_alt_queen {
                continue;
            }

            regions[r][c] = new_letter;
            if is_region_connected(regions, old_letter) {
                return true;
            }
            regions[r][c] = old_letter;
        }
    }
    false
}
pub fn format_board(regions: &[Vec<u8>]) -> String {
    let n = regions.len();
    let mut s = String::with_capacity(n * (n + 1));
    for row in regions {
        for &b in row {
            s.push(b as char);
        }
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    fn check_size(n: usize, seed: u64) {
        let mut rng = StdRng::seed_from_u64(seed);
        let regions = generate_board(n, &mut rng).expect("generate");
        let text = format_board(&regions);
        let parsed = board::parse(&text).expect("parse generated board");
        assert_eq!(parsed.n, n);
        assert_eq!(count_solutions(n, &regions, 2), 1);
    }

    #[test] fn generate_7() { check_size(7, 42); }
    #[test] fn generate_8() { check_size(8, 42); }
    #[test] fn generate_9() { check_size(9, 42); }
    #[test] fn generate_10() { check_size(10, 42); }
    #[test] fn generate_11() { check_size(11, 42); }

    #[test]
    fn sample_queens_satisfies_adjacency() {
        let mut rng = StdRng::seed_from_u64(7);
        for n in 4..=11 {
            let cols = sample_queens(n, &mut rng).expect("placement");
            assert_eq!(cols.len(), n);
            let mut sorted = cols.clone();
            sorted.sort();
            assert_eq!(sorted, (0..n).collect::<Vec<_>>());
            for r in 0..n - 1 {
                assert!((cols[r] as isize - cols[r + 1] as isize).abs() > 1);
            }
        }
    }

    #[test]
    fn small_n_rejected_by_sampler() {
        let mut rng = StdRng::seed_from_u64(0);
        assert!(sample_queens(2, &mut rng).is_none());
        assert!(sample_queens(3, &mut rng).is_none());
    }

    #[test]
    fn too_large_n_rejected_by_generator() {
        let mut rng = StdRng::seed_from_u64(0);
        assert!(matches!(
            generate_board(17, &mut rng),
            Err(GenError::TooLarge(17))
        ));
    }

    #[test]
    fn count_solutions_detects_multiple_solutions() {
        // Diagonal-striped 4x4 regions: the game admits more than one
        // valid placement, so the count should be > 1 (we cap at 4).
        //
        //   A B C D
        //   A B C D
        //   A B C D
        //   A B C D
        let regions: Vec<Vec<u8>> = (0..4)
            .map(|_| (0..4).map(|c| b'A' + c as u8).collect())
            .collect();
        let c = count_solutions(4, &regions, 4);
        assert!(c >= 2, "expected multiple solutions, got {c}");
    }
}
