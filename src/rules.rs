//! Solver rules. Each public `apply_*` function takes `(&mut State,
//! &mut Counters)`, mutates the state by killing cells or placing queens,
//! and returns `true` if at least one cell transitioned during the call.
//!
//! Counter semantics follow PLAN.md: every rule except `QueenAdjacentKill`
//! increments at most once per pass. `QueenAdjacentKill` increments once
//! per queen placement (per HEURISTICS.md's "fires when a queen is
//! placed" phrasing).

use std::collections::BTreeSet;

use crate::board::Region;
use crate::counters::{Counters, RuleId};
use crate::nregions;
use crate::polyomino;
use crate::state::{CellMask, State};

/// Small-region gate for `apply_placement_forces_contradiction`. Only
/// regions with `|live| ≤ H2_SIZE_GATE` can produce a new contradiction
/// that the cheaper rules do not already catch.
const H2_SIZE_GATE: usize = 6;

/// Propagation depth cap for `apply_placement_forces_contradiction`. After
/// applying a candidate's kill list, run the baseline rules at most this
/// many times before checking for a contradiction.
const H2_DEPTH: usize = 2;

/// Place a queen on the board and bump the queen-adjacent-kill counter for
/// the implied cascade of row/column/region/king-adjacent kills.
pub fn place_queen(state: &mut State, counters: &mut Counters, r: usize, c: usize) {
    state.place_queen(r, c);
    counters.increment(RuleId::QueenAdjacentKill);
}

/// Single-cell region rule: for each region with exactly one live cell,
/// place its queen.
pub fn apply_single_cell_region(state: &mut State, counters: &mut Counters) -> bool {
    let targets: Vec<Region> = state
        .region_live
        .iter()
        .filter(|(_, &count)| count == 1)
        .map(|(&letter, _)| letter)
        .collect();
    let mut fired = false;
    for letter in targets {
        if let Some(set) = state.region_live_cells.get(&letter) {
            if let Some(&(r, c)) = set.iter().next() {
                place_queen(state, counters, r, c);
                fired = true;
            }
        }
    }
    if fired {
        counters.increment(RuleId::SingleCellRegion);
    }
    fired
}

/// Single-cell row / column rule: for each row (then each column) with
/// exactly one live cell, place a queen on it.
pub fn apply_single_cell_line(state: &mut State, counters: &mut Counters) -> bool {
    let mut fired = false;
    let n = state.n;

    let rows_to_fire: Vec<usize> = (0..n).filter(|&r| state.row_live[r] == 1).collect();
    for r in rows_to_fire {
        // The row may already have been consumed by an earlier placement in
        // this loop (a queen in row r would make row_live[r] = 0 again).
        if state.row_live[r] != 1 {
            continue;
        }
        if let Some(&c) = state.row_live_cols[r].iter().next() {
            if state.is_live(r, c) {
                place_queen(state, counters, r, c);
                fired = true;
            }
        }
    }

    let cols_to_fire: Vec<usize> = (0..n).filter(|&c| state.col_live[c] == 1).collect();
    for c in cols_to_fire {
        if state.col_live[c] != 1 {
            continue;
        }
        if let Some(&r) = state.col_live_rows[c].iter().next() {
            if state.is_live(r, c) {
                place_queen(state, counters, r, c);
                fired = true;
            }
        }
    }

    if fired {
        counters.increment(RuleId::SingleCellLine);
    }
    fired
}

/// Region-confined-to-line rule: if all live cells of a region share a
/// single row, kill every other cell in that row (same for columns).
pub fn apply_region_confined_to_line(state: &mut State, counters: &mut Counters) -> bool {
    let regions: Vec<Region> = state
        .region_live
        .iter()
        .filter(|(_, &count)| count > 0)
        .map(|(&letter, _)| letter)
        .collect();
    let mut fired = false;
    for letter in regions {
        // Row confinement.
        if state
            .region_rows
            .get(&letter)
            .map(|rs| rs.len() == 1)
            .unwrap_or(false)
        {
            let row = *state.region_rows[&letter].iter().next().unwrap();
            let cols: Vec<usize> = state.row_live_cols[row]
                .iter()
                .copied()
                .filter(|&c| state.board.region_at(row, c) != letter)
                .collect();
            for c in cols {
                if state.kill_cell(row, c) {
                    fired = true;
                }
            }
        }
        // Column confinement.
        if state
            .region_cols
            .get(&letter)
            .map(|cs| cs.len() == 1)
            .unwrap_or(false)
        {
            let col = *state.region_cols[&letter].iter().next().unwrap();
            let rows: Vec<usize> = state.col_live_rows[col]
                .iter()
                .copied()
                .filter(|&r| state.board.region_at(r, col) != letter)
                .collect();
            for r in rows {
                if state.kill_cell(r, col) {
                    fired = true;
                }
            }
        }
    }
    if fired {
        counters.increment(RuleId::RegionConfinedToLine);
    }
    fired
}

/// Line-confined-to-region rule: if every live cell of some row (or column)
/// belongs to one region, kill every other live cell of that region (those
/// outside the row/column).
pub fn apply_line_confined_to_region(state: &mut State, counters: &mut Counters) -> bool {
    let n = state.n;
    let mut fired = false;

    for r in 0..n {
        if state.row_live[r] == 0 {
            continue;
        }
        if state.row_regions[r].len() != 1 {
            continue;
        }
        let letter = *state.row_regions[r].iter().next().unwrap();
        let cells: Vec<(usize, usize)> = state
            .region_live_cells
            .get(&letter)
            .map(|set| set.iter().copied().filter(|&(rr, _)| rr != r).collect())
            .unwrap_or_default();
        for (rr, cc) in cells {
            if state.kill_cell(rr, cc) {
                fired = true;
            }
        }
    }

    for c in 0..n {
        if state.col_live[c] == 0 {
            continue;
        }
        if state.col_regions[c].len() != 1 {
            continue;
        }
        let letter = *state.col_regions[c].iter().next().unwrap();
        let cells: Vec<(usize, usize)> = state
            .region_live_cells
            .get(&letter)
            .map(|set| set.iter().copied().filter(|&(_, cc)| cc != c).collect())
            .unwrap_or_default();
        for (rr, cc) in cells {
            if state.kill_cell(rr, cc) {
                fired = true;
            }
        }
    }

    if fired {
        counters.increment(RuleId::LineConfinedToRegion);
    }
    fired
}

/// Polyomino shape rules: iterate the dirty-and-small region set and, for
/// each region whose live cells match a known polyomino shape, kill the
/// matching dead cells. Each shape has its own counter.
pub fn apply_polyomino_rules(state: &mut State, counters: &mut Counters) -> bool {
    // Snapshot the dirty set so concurrent re-dirtying during iteration
    // does not cause surprises.
    let targets: Vec<Region> = state.dirty_and_small.iter().copied().collect();
    let mut any_fired = false;
    for letter in targets {
        let cells: Vec<(usize, usize)> = state
            .region_live_cells
            .get(&letter)
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default();
        if cells.is_empty() {
            continue;
        }
        let Some((shape_idx, dead_cells)) = polyomino::dead_cells_for_region(&cells) else {
            continue;
        };
        let mut this_shape_fired = false;
        for (r, c) in dead_cells {
            if r < state.n && c < state.n && state.kill_cell(r, c) {
                this_shape_fired = true;
            }
        }
        if this_shape_fired {
            counters.increment(RuleId::Polyomino(shape_idx));
            any_fired = true;
        }
    }
    state.clear_polyomino_dirty();
    any_fired
}

/// Combined N-regions rule: runs the row variant first and short-circuits
/// if it kills any cell. If the row variant did not fire, runs the column
/// variant. The shared `NRegionsInNLines` counter is bumped at most once
/// per call, so rows and columns do not merge their counts within a single
/// pass.
pub fn apply_n_regions(state: &mut State, counters: &mut Counters) -> bool {
    if nregions::apply_n_regions_in_n_rows(state) {
        counters.increment(RuleId::NRegionsInNLines);
        return true;
    }
    if nregions::apply_n_regions_in_n_columns(state) {
        counters.increment(RuleId::NRegionsInNLines);
        return true;
    }
    false
}

/// Placement-forces-contradiction rule: simulate placing a queen at each
/// qualifying candidate, run at most `H2_DEPTH` baseline-rule passes, and
/// mark the candidate dead if the simulation produces a contradiction (a
/// region with zero live cells and no queen, a row or column with no
/// queen and no live cells, or two queens in the same row or column).
///
/// Candidates are restricted to live cells whose precomputed kill mask
/// intersects some region with at most `H2_SIZE_GATE` live cells. Cells
/// whose kill list reaches no small region cannot cause a new contradiction
/// beyond what the cheaper rules already catch.
pub fn apply_placement_forces_contradiction(
    state: &mut State,
    counters: &mut Counters,
) -> bool {
    if !state.h2_dirty {
        return false;
    }
    state.h2_dirty = false;

    // Masks for regions that pass the small-region gate.
    let small_region_masks: Vec<CellMask> = state
        .region_live
        .iter()
        .filter(|(_, &live)| live > 0 && live <= H2_SIZE_GATE)
        .filter_map(|(letter, _)| state.region_live_masks.get(letter).copied())
        .filter(|m| *m != 0)
        .collect();

    if small_region_masks.is_empty() {
        return false;
    }

    // Candidate cells: live cells whose kill mask intersects at least one
    // small region's live mask.
    let n = state.n;
    let mut candidates: Vec<(usize, usize)> = Vec::new();
    for r in 0..n {
        for c in 0..n {
            if !state.is_live(r, c) {
                continue;
            }
            let kill_mask = state.kill_masks[r * n + c];
            if small_region_masks.iter().any(|m| kill_mask & m != 0) {
                candidates.push((r, c));
            }
        }
    }

    let mut cells_to_kill: Vec<(usize, usize)> = Vec::new();
    let mut dummy_counters = Counters::new();

    for (r, c) in candidates {
        if !state.is_live(r, c) {
            continue;
        }

        let snap = state.snapshot();
        state.place_queen(r, c);

        for _ in 0..H2_DEPTH {
            let mut fired = false;
            if apply_single_cell_region(state, &mut dummy_counters) {
                fired = true;
            }
            if apply_single_cell_line(state, &mut dummy_counters) {
                fired = true;
            }
            if apply_region_confined_to_line(state, &mut dummy_counters) {
                fired = true;
            }
            if apply_line_confined_to_region(state, &mut dummy_counters) {
                fired = true;
            }
            if !fired {
                break;
            }
        }

        let contradiction = h2_detect_contradiction(state);
        state.restore(snap);

        if contradiction {
            cells_to_kill.push((r, c));
        }
    }

    let mut any_killed = false;
    for (r, c) in cells_to_kill {
        if state.kill_cell(r, c) {
            any_killed = true;
        }
    }
    if any_killed {
        counters.increment(RuleId::PlacementForcesContradiction);
    }
    any_killed
}

/// Is the current state contradictory? A state is contradictory if any
/// region, row, or column is both live-exhausted and does not yet hold a
/// queen, or if two queens sit in the same row or column.
fn h2_detect_contradiction(state: &State) -> bool {
    let n = state.n;

    // Row and column queen counts.
    let mut row_queen: Vec<u32> = vec![0; n];
    let mut col_queen: Vec<u32> = vec![0; n];
    for &(r, c) in &state.queen_list {
        row_queen[r] += 1;
        col_queen[c] += 1;
    }
    for r in 0..n {
        if row_queen[r] > 1 {
            return true;
        }
        if row_queen[r] == 0 && state.row_live[r] == 0 {
            return true;
        }
    }
    for c in 0..n {
        if col_queen[c] > 1 {
            return true;
        }
        if col_queen[c] == 0 && state.col_live[c] == 0 {
            return true;
        }
    }

    // Regions that already hold a queen.
    let regions_with_queen: BTreeSet<Region> = state
        .queen_list
        .iter()
        .map(|&(r, c)| state.board.region_at(r, c))
        .collect();
    for (&letter, &live) in &state.region_live {
        if live == 0 && !regions_with_queen.contains(&letter) {
            return true;
        }
    }

    false
}

/// Run a single pass of every rule in order. Returns `true` iff any rule
/// marked at least one cell dead or placed a queen.
pub fn run_one_pass(state: &mut State, counters: &mut Counters) -> bool {
    let mut changed = false;
    if apply_single_cell_region(state, counters) {
        changed = true;
    }
    if apply_single_cell_line(state, counters) {
        changed = true;
    }
    if apply_region_confined_to_line(state, counters) {
        changed = true;
    }
    if apply_line_confined_to_region(state, counters) {
        changed = true;
    }
    if apply_polyomino_rules(state, counters) {
        changed = true;
    }
    if apply_n_regions(state, counters) {
        changed = true;
    }
    changed
}

/// Drive the cheap-tier rules to a fixed point. Each iteration calls
/// `run_one_pass` until no rule fires.
pub fn run_cheap_tier_to_fixed_point(state: &mut State, counters: &mut Counters) {
    while run_one_pass(state, counters) {}
}

/// Two-tier fixed-point loop. Repeatedly drive the cheap tier to a fixed
/// point, then invoke `apply_placement_forces_contradiction` once. Loop
/// back to the cheap tier if the expensive rule made any progress.
/// Terminate when both tiers leave the state unchanged.
pub fn run_to_fixed_point(state: &mut State, counters: &mut Counters) {
    loop {
        run_cheap_tier_to_fixed_point(state, counters);
        if !apply_placement_forces_contradiction(state, counters) {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board;

    fn small_four_by_four() -> board::Board {
        let input = "\
AABB
AABB
CCDD
CCDD
";
        board::parse(input).expect("parse small 4x4")
    }

    #[test]
    fn place_queen_increments_queen_adjacent_kill() {
        let b = small_four_by_four();
        let mut state = State::new(&b);
        let mut counters = Counters::new();
        place_queen(&mut state, &mut counters, 0, 0);
        assert_eq!(counters.get(RuleId::QueenAdjacentKill), 1);
    }

    #[test]
    fn single_cell_region_fires_when_one_region_narrows_to_one_cell() {
        let b = small_four_by_four();
        let mut state = State::new(&b);
        let mut counters = Counters::new();

        // Kill every cell of region A except (0, 0). region_live[A] becomes 1.
        state.kill_cell(0, 1);
        state.kill_cell(1, 0);
        state.kill_cell(1, 1);

        let fired = apply_single_cell_region(&mut state, &mut counters);
        assert!(fired);
        assert_eq!(counters.get(RuleId::SingleCellRegion), 1);
        // The single-cell-region rule also places a queen, which bumps
        // queen-adjacent-kill.
        assert_eq!(counters.get(RuleId::QueenAdjacentKill), 1);
        // (0, 0) is now a queen.
        assert_eq!(state.queen_list, vec![(0, 0)]);
    }

    #[test]
    fn single_cell_line_fires_when_row_narrows_to_one_cell() {
        let b = small_four_by_four();
        let mut state = State::new(&b);
        let mut counters = Counters::new();
        // Kill all of row 0 except column 2.
        state.kill_cell(0, 0);
        state.kill_cell(0, 1);
        state.kill_cell(0, 3);
        let fired = apply_single_cell_line(&mut state, &mut counters);
        assert!(fired);
        assert_eq!(counters.get(RuleId::SingleCellLine), 1);
        assert_eq!(counters.get(RuleId::QueenAdjacentKill), 1);
        assert_eq!(state.queen_list, vec![(0, 2)]);
    }

    #[test]
    fn single_cell_line_fires_when_column_narrows_to_one_cell() {
        let b = small_four_by_four();
        let mut state = State::new(&b);
        let mut counters = Counters::new();
        // Kill all of column 3 except row 2.
        state.kill_cell(0, 3);
        state.kill_cell(1, 3);
        state.kill_cell(3, 3);
        let fired = apply_single_cell_line(&mut state, &mut counters);
        assert!(fired);
        assert_eq!(counters.get(RuleId::SingleCellLine), 1);
        assert_eq!(state.queen_list, vec![(2, 3)]);
    }

    #[test]
    fn h2_is_noop_when_dirty_flag_clear() {
        let input = "\
POOBBGG
POOBGGG
OOOGGGS
OOGGGSS
OGGGSSS
GGGRSSS
GGRRLLS
";
        let board = board::parse(input).expect("parse");
        let mut state = State::new(&board);
        let mut counters = Counters::new();
        state.h2_dirty = false;
        let fired = apply_placement_forces_contradiction(&mut state, &mut counters);
        assert!(!fired);
        assert_eq!(counters.get(RuleId::PlacementForcesContradiction), 0);
    }

    #[test]
    fn two_tier_driver_solves_board_582() {
        let input = "\
POOBBGG
POOBGGG
OOOGGGS
OOGGGSS
OGGGSSS
GGGRSSS
GGRRLLS
";
        let board = board::parse(input).expect("parse");
        let mut state = State::new(&board);
        let mut counters = Counters::new();
        run_to_fixed_point(&mut state, &mut counters);
        assert_eq!(state.queen_list.len(), board.n);
        assert!(counters.get(RuleId::PlacementForcesContradiction) >= 1);
    }

    #[test]
    fn two_tier_driver_terminates_on_unsolvable_2x2() {
        let input = "\
AA
BB
";
        let board = board::parse(input).expect("parse");
        let mut state = State::new(&board);
        let mut counters = Counters::new();
        run_to_fixed_point(&mut state, &mut counters);
        assert!(state.queen_list.len() < board.n);
    }

    #[test]
    fn h2_breaks_stall_on_board_582() {
        // Board 582 from archive/boards/. Classic two-region mutual
        // dependency stall: regions O and G each have two diagonally
        // adjacent live cells after the cheap tier runs. Placing a queen
        // at O's cell (3, 1) would empty region G entirely, so H2 marks
        // (3, 1) dead. That unblocks single-cell-region, which cascades
        // through the rest of the board.
        let input = "\
POOBBGG
POOBGGG
OOOGGGS
OOGGGSS
OGGGSSS
GGGRSSS
GGRRLLS
";
        let board = board::parse(input).expect("parse");
        let mut state = State::new(&board);
        let mut counters = Counters::new();

        // Drive the cheap tier to a fixed point.
        while run_one_pass(&mut state, &mut counters) {}

        // The board should stall at four queens with the remaining
        // regions trapped in 2-cell diagonal dominoes.
        let queens_before_h2 = state.queen_list.len();
        assert!(queens_before_h2 < board.n);

        // H2 should fire and mark at least one cell dead.
        let fired = apply_placement_forces_contradiction(&mut state, &mut counters);
        assert!(fired);
        assert_eq!(counters.get(RuleId::PlacementForcesContradiction), 1);
    }

    #[test]
    fn fixed_point_loop_terminates_on_seven_by_seven_board() {
        // The 7x7 example from QUEENS.md. The solver may or may not fully
        // solve this with heuristics alone; the test asserts only that the
        // fixed-point loop returns, that every placed queen sits on a
        // distinct row / column / region, and that no two queens are
        // king-adjacent.
        let input = "\
PPPPPPO
PSGLLLO
PSGGGLO
PSSBGLO
PRSBBBO
PRRRRBO
POOOOOO
";
        let board = board::parse(input).expect("parse");
        let mut state = State::new(&board);
        let mut counters = Counters::new();
        run_to_fixed_point(&mut state, &mut counters);
        let rows: std::collections::BTreeSet<usize> =
            state.queen_list.iter().map(|&(r, _)| r).collect();
        let cols: std::collections::BTreeSet<usize> =
            state.queen_list.iter().map(|&(_, c)| c).collect();
        let regions: std::collections::BTreeSet<u8> = state
            .queen_list
            .iter()
            .map(|&(r, c)| board.region_at(r, c))
            .collect();
        assert_eq!(rows.len(), state.queen_list.len());
        assert_eq!(cols.len(), state.queen_list.len());
        assert_eq!(regions.len(), state.queen_list.len());
        for (i, &(r1, c1)) in state.queen_list.iter().enumerate() {
            for &(r2, c2) in state.queen_list.iter().skip(i + 1) {
                let chebyshev = (r1 as i32 - r2 as i32)
                    .abs()
                    .max((c1 as i32 - c2 as i32).abs());
                assert!(chebyshev >= 2, "queens ({r1},{c1}) and ({r2},{c2}) are king-adjacent");
            }
        }
    }

    #[test]
    fn fixed_point_loop_terminates_on_stuck_board() {
        // A 2x2 board has no valid queen arrangement (any two 2x2 cells are
        // king-adjacent). The heuristics should mark cells dead but
        // ultimately stall without placing two queens. The test confirms
        // run_to_fixed_point returns rather than looping forever.
        let input = "\
AA
BB
";
        let board = board::parse(input).expect("parse");
        let mut state = State::new(&board);
        let mut counters = Counters::new();
        run_to_fixed_point(&mut state, &mut counters);
        // Solver stalled without completing. Fewer than N queens placed.
        assert!(state.queen_list.len() < state.n);
    }

    #[test]
    fn apply_n_regions_bumps_combined_counter_when_either_variant_fires() {
        // Same board as nregions::tests::apply_n_regions_in_n_rows_kills_expected_cells.
        let input = "\
AABB
AACC
DDCC
DDCC
";
        let board = board::parse(input).expect("parse");
        let mut state = State::new(&board);
        let mut counters = Counters::new();
        let fired = apply_n_regions(&mut state, &mut counters);
        assert!(fired);
        assert_eq!(counters.get(RuleId::NRegionsInNLines), 1);
    }

    #[test]
    fn apply_n_regions_is_noop_when_both_dirty_flags_clear() {
        let input = "\
AABB
AACC
DDCC
DDCC
";
        let board = board::parse(input).expect("parse");
        let mut state = State::new(&board);
        let mut counters = Counters::new();
        state.nregions_rows_dirty = false;
        state.nregions_cols_dirty = false;
        let fired = apply_n_regions(&mut state, &mut counters);
        assert!(!fired);
        assert_eq!(counters.get(RuleId::NRegionsInNLines), 0);
    }

    #[test]
    fn polyomino_l_tromino_kills_three_cells() {
        // 5x5 board where region B is an L-tromino at (1, 1), (1, 2), (2, 1).
        // Regions C and E are L-tetrominoes and fire their own rules in the
        // same pass; the test asserts that B's deductions take effect while
        // the overall polyomino-shape counters bump as expected.
        //
        // Layout:
        //   0: A A A A A
        //   1: C B B A A
        //   2: C B D D D
        //   3: C C E D D
        //   4: E E E D D
        let input = "\
AAAAA
CBBAA
CBDDD
CCEDD
EEEDD
";
        let board = board::parse(input).expect("parse");
        let mut state = State::new(&board);
        let mut counters = Counters::new();
        assert!(state.dirty_and_small.contains(&b'B'));
        let fired = apply_polyomino_rules(&mut state, &mut counters);
        assert!(fired);
        // B's L-tromino at elbow (1, 1) kills missing corner (2, 2) plus
        // elbow extensions (0, 1) and (1, 0).
        assert!(!state.is_live(0, 1));
        assert!(!state.is_live(1, 0));
        assert!(!state.is_live(2, 2));
        // dirty_and_small is cleared after the pass.
        assert!(state.dirty_and_small.is_empty());
        // Three polyomino shape rules fire in this pass: L-tromino (B),
        // and L-tetromino twice (C and E).
        let total_polyomino_counts: u64 = (0..polyomino::POLYOMINO_SHAPE_RULES.len())
            .map(|i| counters.get(RuleId::Polyomino(i)))
            .sum();
        assert_eq!(total_polyomino_counts, 3);
    }

    #[test]
    fn line_confined_to_region_kills_other_cells_of_the_region() {
        // 5x5 board. We kill enough cells so that row 0 contains live cells
        // only from region A, triggering line-confined-to-region. Every
        // other A cell outside row 0 should die.
        let input = "\
AAAAA
AABBB
AACCC
DDEEE
DDEEE
";
        let board = board::parse(input).expect("parse");
        let mut state = State::new(&board);
        let mut counters = Counters::new();
        // Row 0 initially contains only region A.
        let fired = apply_line_confined_to_region(&mut state, &mut counters);
        assert!(fired);
        assert_eq!(counters.get(RuleId::LineConfinedToRegion), 1);
        // A cells outside row 0 are dead: (1, 0), (1, 1), (2, 0), (2, 1).
        assert!(!state.is_live(1, 0));
        assert!(!state.is_live(1, 1));
        assert!(!state.is_live(2, 0));
        assert!(!state.is_live(2, 1));
        // A cells in row 0 still alive.
        assert!(state.is_live(0, 0));
        assert!(state.is_live(0, 4));
    }

    #[test]
    fn region_confined_to_row_kills_other_cells_in_that_row() {
        // 5x5 board. Region B occupies a single row-5 slice.
        let input = "\
AAAAA
AAAAA
AAAAA
BBCCC
DDEEE
";
        let board = board::parse(input).expect("parse");
        let mut state = State::new(&board);
        let mut counters = Counters::new();
        // Region B at row 3 cols 0-1. Living only in row 3.
        let fired = apply_region_confined_to_line(&mut state, &mut counters);
        assert!(fired);
        assert_eq!(counters.get(RuleId::RegionConfinedToLine), 1);
        // C cells at row 3 cols 2, 3, 4 are dead now (non-B cells in row 3).
        assert!(!state.is_live(3, 2));
        assert!(!state.is_live(3, 3));
        assert!(!state.is_live(3, 4));
        // Row 4 cells still alive.
        assert!(state.is_live(4, 0));
    }

    #[test]
    fn single_cell_region_is_noop_when_no_region_has_exactly_one_live_cell() {
        let b = small_four_by_four();
        let mut state = State::new(&b);
        let mut counters = Counters::new();
        let fired = apply_single_cell_region(&mut state, &mut counters);
        assert!(!fired);
        assert_eq!(counters.get(RuleId::SingleCellRegion), 0);
        assert_eq!(counters.get(RuleId::QueenAdjacentKill), 0);
    }
}
