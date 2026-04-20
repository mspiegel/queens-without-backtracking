//! Solver state plus the incrementally-maintained structures described in
//! `HEURISTICS.md`.
//!
//! The [`State`] struct owns every piece of dynamic information the solver
//! needs while running its fixed-point loop. Two public primitives mutate
//! the state:
//!
//! * [`State::kill_cell`] marks a live cell as dead.
//! * [`State::place_queen`] places a queen at a live cell and then kills
//!   every cell on the precomputed kill list for that cell.
//!
//! Every other structure is derived from those two primitives and the
//! static `regions` grid.

use std::collections::{BTreeMap, BTreeSet};

use crate::board::{Board, Region};

/// Bitmask of cells on an N × N board. Bit `r * n + c` corresponds to cell
/// `(r, c)`. A `u128` holds up to 128 bits, enough for the Queens boards
/// shipped so far (N ≤ 11, so N² ≤ 121).
pub type CellMask = u128;

/// The single-bit mask for cell `(r, c)` on a board of side `n`.
#[inline]
pub const fn cell_bit(r: usize, c: usize, n: usize) -> CellMask {
    1u128 << (r * n + c)
}

/// Solver state.
#[derive(Debug)]
pub struct State<'a> {
    /// The board. Regions never change.
    pub board: &'a Board,
    /// Board side length. Equivalent to `board.n`.
    pub n: usize,

    /// Dead-cell grid, row-major (`[r * n + c]`).
    pub dead: Vec<bool>,
    /// Queen-cell grid, row-major.
    pub queen: Vec<bool>,
    /// Queens in the order they were placed.
    pub queen_list: Vec<(usize, usize)>,

    // ---- live-cell counts ----
    /// Number of live cells per region letter.
    pub region_live: BTreeMap<Region, usize>,
    /// Number of live cells per row, indexed 0..n.
    pub row_live: Vec<usize>,
    /// Number of live cells per column, indexed 0..n.
    pub col_live: Vec<usize>,

    // ---- live-cell sets ----
    /// Live cells per region.
    pub region_live_cells: BTreeMap<Region, BTreeSet<(usize, usize)>>,
    /// Columns with a live cell, per row.
    pub row_live_cols: Vec<BTreeSet<usize>>,
    /// Rows with a live cell, per column.
    pub col_live_rows: Vec<BTreeSet<usize>>,

    // ---- distinct-rows/cols per region ----
    /// Rows that still contain at least one live cell of the given region.
    pub region_rows: BTreeMap<Region, BTreeSet<usize>>,
    /// Columns that still contain at least one live cell of the given region.
    pub region_cols: BTreeMap<Region, BTreeSet<usize>>,
    /// Live-cell count for each `(region, row)` pair.
    pub region_row_live: BTreeMap<(Region, usize), usize>,
    /// Live-cell count for each `(region, col)` pair.
    pub region_col_live: BTreeMap<(Region, usize), usize>,

    // ---- distinct-regions per row/col ----
    /// Regions with at least one live cell in the given row.
    pub row_regions: Vec<BTreeSet<Region>>,
    /// Regions with at least one live cell in the given column.
    pub col_regions: Vec<BTreeSet<Region>>,

    // ---- polyomino-rule dirty tracking ----
    /// Regions whose live-cell set has changed since the last polyomino check.
    pub dirty_regions: BTreeSet<Region>,
    /// The subset of `dirty_regions` whose live count sits in `[2, 6]`.
    /// These are the only regions where a polyomino shape rule can fire.
    pub dirty_and_small: BTreeSet<Region>,

    // ---- N-regions rule dirty flags ----
    /// Set whenever a `(region, row)` live-cell counter transitions from 1
    /// to 0 (an edge deletion in the bipartite graph).
    pub nregions_rows_dirty: bool,
    /// Same as above but for `(region, column)`.
    pub nregions_cols_dirty: bool,

    // ---- placement-forces-contradiction rule dirty flag ----
    /// Set whenever any region's live-cell count decreases. Cleared by
    /// `apply_placement_forces_contradiction` at the start of a dirty pass.
    pub h2_dirty: bool,

    // ---- static precomputations ----
    /// Per-cell kill list. `kill_list[r * n + c]` enumerates every cell that
    /// would die if a queen landed on `(r, c)`, namely row `r`, column `c`,
    /// the region containing `(r, c)`, and the king-neighborhood, minus
    /// `(r, c)` itself.
    pub kill_list: Vec<Vec<(usize, usize)>>,

    /// Per-cell kill mask. `kill_masks[r * n + c]` is the bitmask equivalent
    /// of `kill_list[r * n + c]`. Precomputed once and then used for the
    /// `placement-forces-contradiction` rule's candidate filter.
    pub kill_masks: Vec<CellMask>,

    // ---- incrementally maintained bitmasks ----
    /// Per-region live-cell mask. Bit `r * n + c` is set iff `(r, c)` is a
    /// live cell of the region. Maintained alongside `region_live_cells` and
    /// used for fast "does this kill list swallow that region" checks by the
    /// `placement-forces-contradiction` rule.
    pub region_live_masks: BTreeMap<Region, CellMask>,
}

impl<'a> State<'a> {
    /// Construct an initial state for the given board. All cells start live,
    /// no queens are placed, no cells are dead.
    pub fn new(board: &'a Board) -> Self {
        let n = board.n;

        let dead = vec![false; n * n];
        let queen = vec![false; n * n];
        let queen_list = Vec::with_capacity(n);

        // Per-region live counts and cell sets.
        let mut region_live: BTreeMap<Region, usize> = BTreeMap::new();
        let mut region_live_cells: BTreeMap<Region, BTreeSet<(usize, usize)>> = BTreeMap::new();
        for (&letter, cells) in &board.region_cells {
            region_live.insert(letter, cells.len());
            region_live_cells.insert(letter, cells.iter().copied().collect());
        }

        // Per-row/col live counts and cell sets.
        let row_live = vec![n; n];
        let col_live = vec![n; n];
        let row_live_cols: Vec<BTreeSet<usize>> = (0..n).map(|_| (0..n).collect()).collect();
        let col_live_rows: Vec<BTreeSet<usize>> = (0..n).map(|_| (0..n).collect()).collect();

        // Distinct-rows/cols per region, plus per-pair live counts.
        let mut region_rows: BTreeMap<Region, BTreeSet<usize>> = BTreeMap::new();
        let mut region_cols: BTreeMap<Region, BTreeSet<usize>> = BTreeMap::new();
        let mut region_row_live: BTreeMap<(Region, usize), usize> = BTreeMap::new();
        let mut region_col_live: BTreeMap<(Region, usize), usize> = BTreeMap::new();
        for (&letter, cells) in &board.region_cells {
            let rows: BTreeSet<usize> = cells.iter().map(|&(r, _)| r).collect();
            let cols: BTreeSet<usize> = cells.iter().map(|&(_, c)| c).collect();
            region_rows.insert(letter, rows);
            region_cols.insert(letter, cols);
            for &(r, c) in cells {
                *region_row_live.entry((letter, r)).or_insert(0) += 1;
                *region_col_live.entry((letter, c)).or_insert(0) += 1;
            }
        }

        // Distinct-regions per row/col.
        let mut row_regions: Vec<BTreeSet<Region>> = (0..n).map(|_| BTreeSet::new()).collect();
        let mut col_regions: Vec<BTreeSet<Region>> = (0..n).map(|_| BTreeSet::new()).collect();
        for r in 0..n {
            for c in 0..n {
                let letter = board.region_at(r, c);
                row_regions[r].insert(letter);
                col_regions[c].insert(letter);
            }
        }

        // Dirty flags: the first pass of every rule must inspect every
        // region (they may fire from the initial state), so we seed all
        // dirty trackers as fully dirty.
        let mut dirty_regions: BTreeSet<Region> = board.region_cells.keys().copied().collect();
        let mut dirty_and_small: BTreeSet<Region> = BTreeSet::new();
        for (&letter, cells) in &board.region_cells {
            if (2..=6).contains(&cells.len()) {
                dirty_and_small.insert(letter);
            }
        }
        // Drop dirty tracking for regions whose live count is outside [2, 6].
        // Polyomino rules only need to see small regions.
        let small_live_bounds = 2..=6;
        dirty_regions.retain(|letter| small_live_bounds.contains(&region_live[letter]));

        // Precomputed per-cell kill list.
        let kill_list = build_kill_list(board);

        // Per-cell kill mask, derived from the kill list.
        let kill_masks: Vec<CellMask> = kill_list
            .iter()
            .map(|cells| {
                let mut mask: CellMask = 0;
                for &(r, c) in cells {
                    mask |= cell_bit(r, c, n);
                }
                mask
            })
            .collect();

        // Per-region live-cell mask, starting with every cell of the region
        // set to 1.
        let mut region_live_masks: BTreeMap<Region, CellMask> = BTreeMap::new();
        for (&letter, cells) in &board.region_cells {
            let mut mask: CellMask = 0;
            for &(r, c) in cells {
                mask |= cell_bit(r, c, n);
            }
            region_live_masks.insert(letter, mask);
        }

        State {
            board,
            n,
            dead,
            queen,
            queen_list,
            region_live,
            row_live,
            col_live,
            region_live_cells,
            row_live_cols,
            col_live_rows,
            region_rows,
            region_cols,
            region_row_live,
            region_col_live,
            row_regions,
            col_regions,
            dirty_regions,
            dirty_and_small,
            nregions_rows_dirty: true,
            nregions_cols_dirty: true,
            h2_dirty: true,
            kill_list,
            kill_masks,
            region_live_masks,
        }
    }

    /// Is the cell at `(r, c)` still live? (Neither a queen nor dead.)
    #[inline]
    pub fn is_live(&self, r: usize, c: usize) -> bool {
        let idx = r * self.n + c;
        !self.dead[idx] && !self.queen[idx]
    }

    /// Mark a live cell dead. Returns `true` if the cell transitioned from
    /// live to dead this call. Calling on a dead or queen cell is a no-op
    /// and returns `false`.
    pub fn kill_cell(&mut self, r: usize, c: usize) -> bool {
        let idx = r * self.n + c;
        if self.dead[idx] || self.queen[idx] {
            return false;
        }
        self.dead[idx] = true;
        self.drop_live_tracking(r, c);
        true
    }

    /// Place a queen at `(r, c)`. The cell must currently be live. Every
    /// cell in the precomputed kill list for `(r, c)` is then killed.
    pub fn place_queen(&mut self, r: usize, c: usize) {
        let idx = r * self.n + c;
        assert!(
            !self.dead[idx] && !self.queen[idx],
            "cannot place a queen on a dead or queen cell ({r}, {c})"
        );
        self.queen[idx] = true;
        self.queen_list.push((r, c));
        self.drop_live_tracking(r, c);

        let kills = self.kill_list[idx].clone();
        for (nr, nc) in kills {
            self.kill_cell(nr, nc);
        }
    }

    /// Update every incremental structure to reflect that `(r, c)` just
    /// left the live state. Called from both `kill_cell` and `place_queen`.
    fn drop_live_tracking(&mut self, r: usize, c: usize) {
        let letter = self.board.region_at(r, c);

        // Region totals.
        if let Some(count) = self.region_live.get_mut(&letter) {
            *count -= 1;
        }
        if let Some(set) = self.region_live_cells.get_mut(&letter) {
            set.remove(&(r, c));
        }
        if let Some(mask) = self.region_live_masks.get_mut(&letter) {
            *mask &= !cell_bit(r, c, self.n);
        }
        self.row_live[r] -= 1;
        self.col_live[c] -= 1;
        self.row_live_cols[r].remove(&c);
        self.col_live_rows[c].remove(&r);

        // Region-row and region-column ref counts.
        let rr_now = {
            let entry = self.region_row_live.get_mut(&(letter, r)).unwrap();
            *entry -= 1;
            *entry
        };
        if rr_now == 0 {
            if let Some(rows) = self.region_rows.get_mut(&letter) {
                rows.remove(&r);
            }
            self.row_regions[r].remove(&letter);
            self.nregions_rows_dirty = true;
        }

        let rc_now = {
            let entry = self.region_col_live.get_mut(&(letter, c)).unwrap();
            *entry -= 1;
            *entry
        };
        if rc_now == 0 {
            if let Some(cols) = self.region_cols.get_mut(&letter) {
                cols.remove(&c);
            }
            self.col_regions[c].remove(&letter);
            self.nregions_cols_dirty = true;
        }

        // Polyomino rule tracking.
        self.dirty_regions.insert(letter);
        self.refresh_dirty_and_small(letter);

        // Placement-forces-contradiction tracking. Any live-count decrease
        // in any region can unlock a new contradiction.
        self.h2_dirty = true;
    }

    fn refresh_dirty_and_small(&mut self, letter: Region) {
        let live = *self.region_live.get(&letter).unwrap_or(&0);
        let is_dirty = self.dirty_regions.contains(&letter);
        if is_dirty && (2..=6).contains(&live) {
            self.dirty_and_small.insert(letter);
        } else {
            self.dirty_and_small.remove(&letter);
        }
    }

    /// Mark all polyomino dirty tracking clean. Called after a pass finishes
    /// inspecting every dirty region.
    pub fn clear_polyomino_dirty(&mut self) {
        self.dirty_regions.clear();
        self.dirty_and_small.clear();
    }

    /// Capture every dynamic field of the state (everything that can change
    /// during a solve) into a clonable snapshot. Static fields (`board`, `n`,
    /// `kill_list`, `kill_masks`) are not copied because they never change.
    ///
    /// Used by `placement-forces-contradiction` to fork the state before
    /// simulating a queen placement, then roll back after checking for a
    /// contradiction.
    pub fn snapshot(&self) -> SavedState {
        SavedState {
            dead: self.dead.clone(),
            queen: self.queen.clone(),
            queen_list: self.queen_list.clone(),
            region_live: self.region_live.clone(),
            row_live: self.row_live.clone(),
            col_live: self.col_live.clone(),
            region_live_cells: self.region_live_cells.clone(),
            row_live_cols: self.row_live_cols.clone(),
            col_live_rows: self.col_live_rows.clone(),
            region_rows: self.region_rows.clone(),
            region_cols: self.region_cols.clone(),
            region_row_live: self.region_row_live.clone(),
            region_col_live: self.region_col_live.clone(),
            row_regions: self.row_regions.clone(),
            col_regions: self.col_regions.clone(),
            dirty_regions: self.dirty_regions.clone(),
            dirty_and_small: self.dirty_and_small.clone(),
            nregions_rows_dirty: self.nregions_rows_dirty,
            nregions_cols_dirty: self.nregions_cols_dirty,
            h2_dirty: self.h2_dirty,
            region_live_masks: self.region_live_masks.clone(),
        }
    }

    /// Restore every dynamic field of the state from a previous snapshot.
    /// The caller is responsible for ensuring the snapshot came from the
    /// same `State` instance; nothing else will be touched.
    pub fn restore(&mut self, saved: SavedState) {
        self.dead = saved.dead;
        self.queen = saved.queen;
        self.queen_list = saved.queen_list;
        self.region_live = saved.region_live;
        self.row_live = saved.row_live;
        self.col_live = saved.col_live;
        self.region_live_cells = saved.region_live_cells;
        self.row_live_cols = saved.row_live_cols;
        self.col_live_rows = saved.col_live_rows;
        self.region_rows = saved.region_rows;
        self.region_cols = saved.region_cols;
        self.region_row_live = saved.region_row_live;
        self.region_col_live = saved.region_col_live;
        self.row_regions = saved.row_regions;
        self.col_regions = saved.col_regions;
        self.dirty_regions = saved.dirty_regions;
        self.dirty_and_small = saved.dirty_and_small;
        self.nregions_rows_dirty = saved.nregions_rows_dirty;
        self.nregions_cols_dirty = saved.nregions_cols_dirty;
        self.h2_dirty = saved.h2_dirty;
        self.region_live_masks = saved.region_live_masks;
    }
}

/// A deep clone of every dynamic field of `State`, sufficient to restore
/// the solver to a previous state via `State::restore`. Static fields
/// (`board`, `n`, `kill_list`, `kill_masks`) are not included because they
/// never change.
#[derive(Debug, Clone)]
pub struct SavedState {
    pub dead: Vec<bool>,
    pub queen: Vec<bool>,
    pub queen_list: Vec<(usize, usize)>,
    pub region_live: BTreeMap<Region, usize>,
    pub row_live: Vec<usize>,
    pub col_live: Vec<usize>,
    pub region_live_cells: BTreeMap<Region, BTreeSet<(usize, usize)>>,
    pub row_live_cols: Vec<BTreeSet<usize>>,
    pub col_live_rows: Vec<BTreeSet<usize>>,
    pub region_rows: BTreeMap<Region, BTreeSet<usize>>,
    pub region_cols: BTreeMap<Region, BTreeSet<usize>>,
    pub region_row_live: BTreeMap<(Region, usize), usize>,
    pub region_col_live: BTreeMap<(Region, usize), usize>,
    pub row_regions: Vec<BTreeSet<Region>>,
    pub col_regions: Vec<BTreeSet<Region>>,
    pub dirty_regions: BTreeSet<Region>,
    pub dirty_and_small: BTreeSet<Region>,
    pub nregions_rows_dirty: bool,
    pub nregions_cols_dirty: bool,
    pub h2_dirty: bool,
    pub region_live_masks: BTreeMap<Region, CellMask>,
}

/// Build the per-cell kill list: for each cell `(r, c)`, the set of cells
/// that would die if a queen landed there.
fn build_kill_list(board: &Board) -> Vec<Vec<(usize, usize)>> {
    let n = board.n;
    let mut out: Vec<Vec<(usize, usize)>> = Vec::with_capacity(n * n);
    for r in 0..n {
        for c in 0..n {
            let letter = board.region_at(r, c);
            let mut kills: BTreeSet<(usize, usize)> = BTreeSet::new();
            // Same row, same column.
            for k in 0..n {
                if k != c {
                    kills.insert((r, k));
                }
                if k != r {
                    kills.insert((k, c));
                }
            }
            // Same region.
            if let Some(cells) = board.region_cells.get(&letter) {
                for &(rr, cc) in cells {
                    if (rr, cc) != (r, c) {
                        kills.insert((rr, cc));
                    }
                }
            }
            // King neighborhood.
            for dr in -1i32..=1 {
                for dc in -1i32..=1 {
                    if dr == 0 && dc == 0 {
                        continue;
                    }
                    let nr = r as i32 + dr;
                    let nc = c as i32 + dc;
                    if nr < 0 || nc < 0 || nr >= n as i32 || nc >= n as i32 {
                        continue;
                    }
                    kills.insert((nr as usize, nc as usize));
                }
            }
            out.push(kills.into_iter().collect());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board;

    fn sample_board() -> Board {
        let input = "\
PPPPPPO
PSGLLLO
PSGGGLO
PSSBGLO
PRSBBBO
PRRRRBO
POOOOOO
";
        board::parse(input).expect("parse sample board")
    }

    #[test]
    fn initial_state_has_all_cells_live() {
        let b = sample_board();
        let state = State::new(&b);
        for r in 0..b.n {
            for c in 0..b.n {
                assert!(state.is_live(r, c));
            }
        }
        assert_eq!(state.queen_list, Vec::<(usize, usize)>::new());
    }

    #[test]
    fn initial_region_live_counts_match_region_sizes() {
        let b = sample_board();
        let state = State::new(&b);
        for (&letter, cells) in &b.region_cells {
            assert_eq!(state.region_live[&letter], cells.len());
        }
    }

    #[test]
    fn initial_row_and_col_live_counts_equal_n() {
        let b = sample_board();
        let state = State::new(&b);
        for r in 0..b.n {
            assert_eq!(state.row_live[r], b.n);
        }
        for c in 0..b.n {
            assert_eq!(state.col_live[c], b.n);
        }
    }

    #[test]
    fn kill_cell_decrements_all_counters() {
        let b = sample_board();
        let mut state = State::new(&b);
        let letter = b.region_at(0, 0);
        let region_before = state.region_live[&letter];
        let changed = state.kill_cell(0, 0);
        assert!(changed);
        assert_eq!(state.row_live[0], b.n - 1);
        assert_eq!(state.col_live[0], b.n - 1);
        assert_eq!(state.region_live[&letter], region_before - 1);
        assert!(!state.is_live(0, 0));
        assert!(state.dead[0]);
    }

    #[test]
    fn kill_cell_is_idempotent() {
        let b = sample_board();
        let mut state = State::new(&b);
        assert!(state.kill_cell(0, 0));
        assert!(!state.kill_cell(0, 0));
        assert_eq!(state.row_live[0], b.n - 1);
    }

    #[test]
    fn place_queen_fires_precomputed_kill_list() {
        let b = sample_board();
        let mut state = State::new(&b);
        state.place_queen(3, 3);
        assert_eq!(state.queen_list, vec![(3, 3)]);
        // Row, column, and king-neighborhood are all dead now.
        for c in 0..b.n {
            if c != 3 {
                assert!(!state.is_live(3, c), "cell (3, {c}) should be dead");
            }
        }
        for r in 0..b.n {
            if r != 3 {
                assert!(!state.is_live(r, 3), "cell ({r}, 3) should be dead");
            }
        }
        for (dr, dc) in [(-1i32, -1), (-1, 0), (-1, 1), (0, -1), (0, 1), (1, -1), (1, 0), (1, 1)] {
            let nr = 3 + dr;
            let nc = 3 + dc;
            if (0..b.n as i32).contains(&nr) && (0..b.n as i32).contains(&nc) {
                assert!(
                    !state.is_live(nr as usize, nc as usize),
                    "king-adjacent ({nr}, {nc}) should be dead"
                );
            }
        }
        // Region is dead too.
        let letter = b.region_at(3, 3);
        for &(rr, cc) in &b.region_cells[&letter] {
            if (rr, cc) != (3, 3) {
                assert!(!state.is_live(rr, cc));
            }
        }
    }

    #[test]
    fn dropping_last_cell_in_region_row_flips_nregions_dirty() {
        let b = sample_board();
        let mut state = State::new(&b);
        state.nregions_rows_dirty = false;
        let letter = b.region_at(1, 1);
        let cells_in_row: Vec<(usize, usize)> = b.region_cells[&letter]
            .iter()
            .copied()
            .filter(|&(r, _)| r == 1)
            .collect();
        assert!(!cells_in_row.is_empty());
        for &(r, c) in &cells_in_row {
            state.kill_cell(r, c);
        }
        assert!(state.nregions_rows_dirty);
        assert!(!state.region_rows[&letter].contains(&1));
    }

    #[test]
    fn dirty_and_small_tracks_regions_with_live_count_in_two_through_six() {
        let b = sample_board();
        let mut state = State::new(&b);
        let letter = b.region_at(6, 6); // the O region
        let cells: Vec<(usize, usize)> = b.region_cells[&letter].to_vec();
        // Initially the O region has 12 cells (N=7, big region); not in
        // dirty_and_small.
        assert!(!state.dirty_and_small.contains(&letter));
        // Kill cells until the live count drops to 6.
        let mut remaining = cells.len();
        for &(r, c) in &cells {
            if remaining <= 6 {
                break;
            }
            state.kill_cell(r, c);
            remaining -= 1;
        }
        assert_eq!(state.region_live[&letter], 6);
        assert!(state.dirty_and_small.contains(&letter));
        // Drop further to 1, past the lower bound, and it leaves the set.
        for &(r, c) in &cells {
            if remaining <= 1 {
                break;
            }
            if state.is_live(r, c) {
                state.kill_cell(r, c);
                remaining -= 1;
            }
        }
        assert_eq!(state.region_live[&letter], 1);
        assert!(!state.dirty_and_small.contains(&letter));
    }

    #[test]
    fn kill_list_for_corner_contains_row_col_region_and_king_neighbors() {
        let b = sample_board();
        let state = State::new(&b);
        let kills: BTreeSet<(usize, usize)> = state.kill_list[0].iter().copied().collect();
        // (0, 1) is in row 0 and is king-adjacent.
        assert!(kills.contains(&(0, 1)));
        // (1, 0) is in col 0 and is king-adjacent.
        assert!(kills.contains(&(1, 0)));
        // (1, 1) is king-adjacent.
        assert!(kills.contains(&(1, 1)));
        // (6, 0) is in col 0 and same region.
        assert!(kills.contains(&(6, 0)));
        // (0, 0) itself is not in the kill list.
        assert!(!kills.contains(&(0, 0)));
    }

    #[test]
    fn kill_mask_for_a_corner_cell_covers_row_col_region_and_king_but_not_self() {
        let b = sample_board();
        let state = State::new(&b);
        let n = state.n;
        let mask = state.kill_masks[0];

        // (0, 1) is in row 0 and king-adjacent to (0, 0).
        assert_ne!(mask & cell_bit(0, 1, n), 0);
        // (1, 0) is in col 0 and king-adjacent.
        assert_ne!(mask & cell_bit(1, 0, n), 0);
        // (1, 1) is king-adjacent.
        assert_ne!(mask & cell_bit(1, 1, n), 0);
        // (6, 0) is in col 0 and same region.
        assert_ne!(mask & cell_bit(6, 0, n), 0);
        // (0, 0) itself is NOT in its own kill mask.
        assert_eq!(mask & cell_bit(0, 0, n), 0);
    }

    #[test]
    fn snapshot_and_restore_roundtrip_after_kill_cell() {
        let b = sample_board();
        let mut state = State::new(&b);

        let snap = state.snapshot();
        state.kill_cell(3, 3);
        assert!(!state.is_live(3, 3));
        state.restore(snap);
        assert!(state.is_live(3, 3));

        // Sanity: the just-restored state should behave exactly like the
        // original one on a fresh kill_cell.
        assert!(state.kill_cell(3, 3));
        assert!(!state.is_live(3, 3));
    }

    #[test]
    fn snapshot_and_restore_roundtrip_after_place_queen() {
        let b = sample_board();
        let mut state = State::new(&b);

        let snap = state.snapshot();
        state.place_queen(3, 3);
        assert_eq!(state.queen_list, vec![(3, 3)]);
        state.restore(snap);
        assert_eq!(state.queen_list, Vec::<(usize, usize)>::new());
        assert!(state.is_live(3, 3));
        // Placement's kill-list cascade should also be rolled back.
        assert!(state.is_live(0, 0));
        assert!(state.is_live(3, 0));
    }

    #[test]
    fn snapshot_captures_h2_dirty_flag() {
        let b = sample_board();
        let mut state = State::new(&b);
        state.h2_dirty = false;
        let snap = state.snapshot();
        state.kill_cell(0, 0);
        assert!(state.h2_dirty);
        state.restore(snap);
        assert!(!state.h2_dirty);
    }

    #[test]
    fn kill_cell_sets_h2_dirty() {
        let b = sample_board();
        let mut state = State::new(&b);
        state.h2_dirty = false;
        state.kill_cell(0, 0);
        assert!(state.h2_dirty);
    }

    #[test]
    fn region_live_mask_loses_bit_on_kill() {
        let b = sample_board();
        let mut state = State::new(&b);
        let n = state.n;
        // Pick any region with more than one cell. The P region at (0, 0)
        // has many cells; kill (0, 0) and check its bit clears while the
        // rest stay set.
        let letter = b.region_at(0, 0);
        let target = (0, 0);
        assert_ne!(
            state.region_live_masks[&letter] & cell_bit(target.0, target.1, n),
            0
        );
        state.kill_cell(target.0, target.1);
        assert_eq!(
            state.region_live_masks[&letter] & cell_bit(target.0, target.1, n),
            0
        );
        // A different P cell should still be set.
        let other_p = *b.region_cells[&letter]
            .iter()
            .find(|&&c| c != target)
            .expect("P has more than one cell");
        assert_ne!(
            state.region_live_masks[&letter] & cell_bit(other_p.0, other_p.1, n),
            0
        );
    }
}
