//! N-regions-in-N-rows / N-regions-in-N-columns rule support.
//!
//! The module implements:
//!
//! * `BipartiteGraph`: a small adjacency-list bipartite graph indexed by
//!   `usize` on both sides.
//! * `max_matching`: augmenting-path maximum bipartite matching (simple
//!   DFS, since `B ≤ 11` makes this trivially fast).
//! * `dulmage_mendelsohn_blocks`: identifies every Hall-tight block via
//!   Tarjan's strongly-connected-components algorithm on the directed
//!   graph built from the matching (matching edges go left → right, other
//!   edges go right → left).
//! * `apply_n_regions_in_n_rows` / `apply_n_regions_in_n_columns`:
//!   solver-facing entry points that run matching + D-M decomposition and
//!   apply each tight block's deductions via `State::kill_cell`.

use std::collections::BTreeSet;

use crate::board::Region;
use crate::state::State;

/// Bipartite graph with `left_count` vertices on the left side and
/// `right_count` on the right. `adj[l]` is the list of right-side indices
/// adjacent to left vertex `l`.
#[derive(Debug, Clone)]
pub struct BipartiteGraph {
    pub left_count: usize,
    pub right_count: usize,
    pub adj: Vec<Vec<usize>>,
}

impl BipartiteGraph {
    pub fn new(left_count: usize, right_count: usize) -> Self {
        BipartiteGraph {
            left_count,
            right_count,
            adj: vec![Vec::new(); left_count],
        }
    }

    pub fn add_edge(&mut self, left: usize, right: usize) {
        self.adj[left].push(right);
    }
}

/// Result of a maximum-matching run on a bipartite graph.
#[derive(Debug, Clone)]
pub struct Matching {
    /// For each left vertex, the matched right vertex (if any).
    pub left_to_right: Vec<Option<usize>>,
    /// For each right vertex, the matched left vertex (if any).
    pub right_to_left: Vec<Option<usize>>,
    /// Size of the matching.
    pub size: usize,
}

/// Compute a maximum bipartite matching using simple augmenting-path DFS.
pub fn max_matching(graph: &BipartiteGraph) -> Matching {
    let mut left_to_right = vec![None; graph.left_count];
    let mut right_to_left = vec![None; graph.right_count];
    let mut size = 0;

    for u in 0..graph.left_count {
        let mut visited = vec![false; graph.right_count];
        if augment(graph, u, &mut visited, &mut left_to_right, &mut right_to_left) {
            size += 1;
        }
    }

    Matching {
        left_to_right,
        right_to_left,
        size,
    }
}

fn augment(
    graph: &BipartiteGraph,
    u: usize,
    visited: &mut [bool],
    left_to_right: &mut [Option<usize>],
    right_to_left: &mut [Option<usize>],
) -> bool {
    for &v in &graph.adj[u] {
        if visited[v] {
            continue;
        }
        visited[v] = true;
        let available = right_to_left[v].is_none()
            || augment(
                graph,
                right_to_left[v].unwrap(),
                visited,
                left_to_right,
                right_to_left,
            );
        if available {
            left_to_right[u] = Some(v);
            right_to_left[v] = Some(u);
            return true;
        }
    }
    false
}

/// A Hall-tight block reported by the D-M decomposition.
///
/// `lefts` is a set of left-vertex indices and `rights` is the set of
/// right-vertex indices they are collectively matched to. `|lefts| == |rights|`
/// and together they form a strongly connected component of the directed
/// graph (matching edges left → right, non-matching edges right → left)
/// restricted to the perfectly-matched core of the bipartite graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TightBlock {
    pub lefts: BTreeSet<usize>,
    pub rights: BTreeSet<usize>,
}

/// Run Dulmage-Mendelsohn decomposition on the given bipartite graph using
/// the provided maximum matching. Returns every minimal Hall-tight block.
///
/// The decomposition contracts each matched pair `(u, matched[u])` into a
/// single super-vertex, then connects pair `P1` to pair `P2` whenever the
/// bipartite graph has a non-matching edge from `P1`'s left vertex to
/// `P2`'s right vertex. Hall-tight subsets of pairs correspond exactly to
/// the forward-closed subsets in this condensed digraph. The minimal
/// forward-closed set containing a given SCC is that SCC together with
/// every SCC reachable from it, so the algorithm emits one tight block per
/// SCC (its own forward closure, lefts plus their matched rights).
///
/// Unmatched left vertices lie in the under-determined part of the
/// decomposition and are not emitted.
pub fn dulmage_mendelsohn_blocks(graph: &BipartiteGraph, matching: &Matching) -> Vec<TightBlock> {
    // Condense each matched pair into a single super-vertex indexed by its
    // left vertex. `succ[u]` holds `u'` whenever the original graph has a
    // non-matching edge from `u` to `matched[u']`.
    let n = graph.left_count;
    let mut succ: Vec<Vec<usize>> = vec![Vec::new(); n];
    for u in 0..n {
        let Some(v_match) = matching.left_to_right[u] else {
            continue;
        };
        for &v in &graph.adj[u] {
            if v == v_match {
                continue;
            }
            if let Some(u_other) = matching.right_to_left[v] {
                if u_other != u {
                    succ[u].push(u_other);
                }
            }
        }
    }

    let sccs = tarjan_sccs(&succ);

    // Record every pair's SCC id so we can build the SCC DAG.
    let mut scc_of: Vec<Option<usize>> = vec![None; n];
    for (scc_id, scc) in sccs.iter().enumerate() {
        for &u in scc {
            scc_of[u] = Some(scc_id);
        }
    }

    // SCC-level adjacency list (no self-loops, no duplicates).
    let mut scc_succ: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); sccs.len()];
    for u in 0..n {
        let Some(s1) = scc_of[u] else {
            continue;
        };
        for &u_other in &succ[u] {
            let Some(s2) = scc_of[u_other] else {
                continue;
            };
            if s1 != s2 {
                scc_succ[s1].insert(s2);
            }
        }
    }

    // For each SCC, compute the forward-reachable SCC set (including itself).
    let mut blocks: Vec<TightBlock> = Vec::new();
    for start_scc in 0..sccs.len() {
        let mut reachable: BTreeSet<usize> = BTreeSet::new();
        let mut stack: Vec<usize> = vec![start_scc];
        while let Some(s) = stack.pop() {
            if reachable.insert(s) {
                for &t in &scc_succ[s] {
                    if !reachable.contains(&t) {
                        stack.push(t);
                    }
                }
            }
        }

        let mut lefts: BTreeSet<usize> = BTreeSet::new();
        let mut rights: BTreeSet<usize> = BTreeSet::new();
        for &s in &reachable {
            for &u in &sccs[s] {
                if let Some(v) = matching.left_to_right[u] {
                    lefts.insert(u);
                    rights.insert(v);
                }
            }
        }
        if lefts.is_empty() {
            continue;
        }
        debug_assert_eq!(lefts.len(), rights.len());
        blocks.push(TightBlock { lefts, rights });
    }

    blocks
}

/// Tarjan's strongly-connected-components algorithm.
fn tarjan_sccs(succ: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let n = succ.len();
    let mut index_of = vec![usize::MAX; n];
    let mut lowlink = vec![0usize; n];
    let mut on_stack = vec![false; n];
    let mut stack: Vec<usize> = Vec::new();
    let mut next_index = 0usize;
    let mut sccs: Vec<Vec<usize>> = Vec::new();

    // Iterative Tarjan avoids stack overflow on large inputs; overkill for
    // B <= 11 but no harm.
    fn strongconnect(
        v: usize,
        succ: &[Vec<usize>],
        index_of: &mut [usize],
        lowlink: &mut [usize],
        on_stack: &mut [bool],
        stack: &mut Vec<usize>,
        next_index: &mut usize,
        sccs: &mut Vec<Vec<usize>>,
    ) {
        index_of[v] = *next_index;
        lowlink[v] = *next_index;
        *next_index += 1;
        stack.push(v);
        on_stack[v] = true;

        for &w in &succ[v] {
            if index_of[w] == usize::MAX {
                strongconnect(
                    w, succ, index_of, lowlink, on_stack, stack, next_index, sccs,
                );
                lowlink[v] = lowlink[v].min(lowlink[w]);
            } else if on_stack[w] {
                lowlink[v] = lowlink[v].min(index_of[w]);
            }
        }

        if lowlink[v] == index_of[v] {
            let mut component: Vec<usize> = Vec::new();
            loop {
                let w = stack.pop().unwrap();
                on_stack[w] = false;
                component.push(w);
                if w == v {
                    break;
                }
            }
            sccs.push(component);
        }
    }

    for v in 0..n {
        if index_of[v] == usize::MAX {
            strongconnect(
                v,
                succ,
                &mut index_of,
                &mut lowlink,
                &mut on_stack,
                &mut stack,
                &mut next_index,
                &mut sccs,
            );
        }
    }

    sccs
}

/// Apply the `N-regions-in-N-rows` rule. Returns `true` iff at least one
/// live cell was marked dead by this pass.
///
/// When the rule's dirty flag is clear, the function returns `false`
/// immediately in `O(1)`. Otherwise it:
///
/// 1. Builds a bipartite graph whose left vertices are regions with at least
///    one remaining row in `state.region_rows` and whose right vertices are
///    the rows used by those regions.
/// 2. Computes a maximum matching and the Dulmage-Mendelsohn decomposition.
/// 3. For each Hall-tight block `(S, rows_used)`, kills every live cell
///    `(r, c)` with `r ∈ rows_used` whose region is not in `S`.
///
/// The dirty flag is cleared at entry; subsequent kills inside this pass
/// will set it again via `state.kill_cell`, so the rule runs again next
/// pass if anything that it touched leaves an edge-deletion trail.
pub fn apply_n_regions_in_n_rows(state: &mut State) -> bool {
    if !state.nregions_rows_dirty {
        return false;
    }
    state.nregions_rows_dirty = false;

    let active_regions: Vec<Region> = state
        .region_rows
        .iter()
        .filter(|(_, rows)| !rows.is_empty())
        .map(|(&letter, _)| letter)
        .collect();
    if active_regions.is_empty() {
        return false;
    }

    // Canonical row order so the bipartite graph's right-side indices match
    // the row numbers we'll feed back to state.kill_cell.
    let mut active_rows_set: BTreeSet<usize> = BTreeSet::new();
    for letter in &active_regions {
        for &row in &state.region_rows[letter] {
            active_rows_set.insert(row);
        }
    }
    let active_rows: Vec<usize> = active_rows_set.into_iter().collect();

    // Build a bipartite graph indexed 0..active_regions.len() / 0..active_rows.len().
    let mut graph = BipartiteGraph::new(active_regions.len(), active_rows.len());
    let row_index = |row: usize| -> usize {
        active_rows.binary_search(&row).expect("row in active set")
    };
    for (left_idx, letter) in active_regions.iter().enumerate() {
        for &row in &state.region_rows[letter] {
            graph.add_edge(left_idx, row_index(row));
        }
    }

    let matching = max_matching(&graph);
    let blocks = dulmage_mendelsohn_blocks(&graph, &matching);

    // Apply each tight block.
    let mut changed = false;
    for block in blocks {
        let s_letters: BTreeSet<Region> = block.lefts.iter().map(|&i| active_regions[i]).collect();
        let rows_used: BTreeSet<usize> = block.rights.iter().map(|&i| active_rows[i]).collect();
        for &row in &rows_used {
            let live_cols: Vec<usize> = state.row_live_cols[row].iter().copied().collect();
            for col in live_cols {
                let letter = state.board.region_at(row, col);
                if !s_letters.contains(&letter) && state.kill_cell(row, col) {
                    changed = true;
                }
            }
        }
    }

    changed
}

/// Mirror of [`apply_n_regions_in_n_rows`] for columns.
pub fn apply_n_regions_in_n_columns(state: &mut State) -> bool {
    if !state.nregions_cols_dirty {
        return false;
    }
    state.nregions_cols_dirty = false;

    let active_regions: Vec<Region> = state
        .region_cols
        .iter()
        .filter(|(_, cols)| !cols.is_empty())
        .map(|(&letter, _)| letter)
        .collect();
    if active_regions.is_empty() {
        return false;
    }

    let mut active_cols_set: BTreeSet<usize> = BTreeSet::new();
    for letter in &active_regions {
        for &col in &state.region_cols[letter] {
            active_cols_set.insert(col);
        }
    }
    let active_cols: Vec<usize> = active_cols_set.into_iter().collect();

    let mut graph = BipartiteGraph::new(active_regions.len(), active_cols.len());
    let col_index = |col: usize| -> usize {
        active_cols.binary_search(&col).expect("col in active set")
    };
    for (left_idx, letter) in active_regions.iter().enumerate() {
        for &col in &state.region_cols[letter] {
            graph.add_edge(left_idx, col_index(col));
        }
    }

    let matching = max_matching(&graph);
    let blocks = dulmage_mendelsohn_blocks(&graph, &matching);

    let mut changed = false;
    for block in blocks {
        let s_letters: BTreeSet<Region> = block.lefts.iter().map(|&i| active_regions[i]).collect();
        let cols_used: BTreeSet<usize> = block.rights.iter().map(|&i| active_cols[i]).collect();
        for &col in &cols_used {
            let live_rows: Vec<usize> = state.col_live_rows[col].iter().copied().collect();
            for row in live_rows {
                let letter = state.board.region_at(row, col);
                if !s_letters.contains(&letter) && state.kill_cell(row, col) {
                    changed = true;
                }
            }
        }
    }

    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board;

    #[test]
    fn matching_handles_empty_graph() {
        let g = BipartiteGraph::new(0, 0);
        let m = max_matching(&g);
        assert_eq!(m.size, 0);
    }

    #[test]
    fn matching_finds_perfect_matching_when_one_exists() {
        // 4x4 bipartite graph where the identity matching works.
        let mut g = BipartiteGraph::new(4, 4);
        for i in 0..4 {
            g.add_edge(i, i);
        }
        // Add extras so augmenting path has some work to do.
        g.add_edge(0, 1);
        g.add_edge(2, 3);
        let m = max_matching(&g);
        assert_eq!(m.size, 4);
    }

    #[test]
    fn matching_reports_size_three_on_hall_violator() {
        // 4 lefts, 4 rights, but lefts 0 and 1 share the same single right 0.
        let mut g = BipartiteGraph::new(4, 4);
        g.add_edge(0, 0);
        g.add_edge(1, 0);
        g.add_edge(2, 2);
        g.add_edge(3, 3);
        let m = max_matching(&g);
        assert_eq!(m.size, 3);
    }

    #[test]
    fn dm_returns_one_tight_block_for_identity_three() {
        // 3x3 perfect matching over identity edges. Each left is tight with
        // its matched right, so three singleton SCCs form three tight blocks
        // of size 1 each.
        let mut g = BipartiteGraph::new(3, 3);
        for i in 0..3 {
            g.add_edge(i, i);
        }
        let m = max_matching(&g);
        assert_eq!(m.size, 3);
        let blocks = dulmage_mendelsohn_blocks(&g, &m);
        assert_eq!(blocks.len(), 3);
        for b in &blocks {
            assert_eq!(b.lefts.len(), 1);
            assert_eq!(b.rights.len(), 1);
        }
    }

    #[test]
    fn apply_n_regions_in_n_rows_kills_expected_cells() {
        // 4x4 board. Region B is confined to row 0; regions A+B together
        // are confined to rows 0-1. The row-variant rule should kill A's
        // cells in row 0 (from tight block {B}) and C's cells in row 1
        // (from tight block {A, B}).
        let input = "\
AABB
AACC
DDCC
DDCC
";
        let board = board::parse(input).expect("parse");
        let mut state = crate::state::State::new(&board);
        let changed = apply_n_regions_in_n_rows(&mut state);
        assert!(changed);
        // A at (0, 0), (0, 1) dead.
        assert!(!state.is_live(0, 0));
        assert!(!state.is_live(0, 1));
        // C at (1, 2), (1, 3) dead.
        assert!(!state.is_live(1, 2));
        assert!(!state.is_live(1, 3));
        // D still alive in rows 2-3.
        assert!(state.is_live(2, 0));
        assert!(state.is_live(3, 0));
    }

    #[test]
    fn apply_n_regions_in_n_columns_kills_expected_cells() {
        // Column-variant mirror of the previous test. Regions A (col 0) and
        // the left pair {A, B} (cols 0-1) form tight column blocks.
        let input = "\
AAAD
ABBD
ACCD
ACCD
";
        // A: cols 0-2, rows 0-3. B: col 1-2, row 1. C: col 1-2, rows 2-3.
        // D: col 3, all rows. So:
        //   A -> cols {0, 1, 2}
        //   B -> cols {1, 2}
        //   C -> cols {1, 2}
        //   D -> cols {3}
        // Tight block {D} confines column 3 to D. Tight block {B, C} confines
        // columns 1, 2 to {B, C}. Applying the rule should kill A at (0, 1),
        // (0, 2) (from {B, C}) and keep D's column 3 clean.
        let board = board::parse(input).expect("parse");
        let mut state = crate::state::State::new(&board);
        let changed = apply_n_regions_in_n_columns(&mut state);
        assert!(changed);
        // A's cells in columns 1 and 2 are dead.
        assert!(!state.is_live(0, 1));
        assert!(!state.is_live(0, 2));
        // A's col-0 cells are still alive.
        assert!(state.is_live(0, 0));
        assert!(state.is_live(1, 0));
        // D's col-3 cells are still alive.
        assert!(state.is_live(0, 3));
        assert!(state.is_live(3, 3));
    }

    #[test]
    fn apply_n_regions_in_n_rows_is_noop_when_dirty_flag_clear() {
        let input = "\
AABB
AACC
DDCC
DDCC
";
        let board = board::parse(input).expect("parse");
        let mut state = crate::state::State::new(&board);
        state.nregions_rows_dirty = false;
        let changed = apply_n_regions_in_n_rows(&mut state);
        assert!(!changed);
        assert!(state.is_live(0, 0));
        assert!(state.is_live(1, 2));
    }

    #[test]
    fn dm_collapses_two_lefts_with_shared_rights_into_one_block() {
        // Lefts 0, 1 both connect to rights 0, 1 (they share both). That is
        // a tight block of size 2. Left 2 only connects to right 2.
        let mut g = BipartiteGraph::new(3, 3);
        g.add_edge(0, 0);
        g.add_edge(0, 1);
        g.add_edge(1, 0);
        g.add_edge(1, 1);
        g.add_edge(2, 2);
        let m = max_matching(&g);
        assert_eq!(m.size, 3);
        let blocks = dulmage_mendelsohn_blocks(&g, &m);
        // Expect two blocks: {lefts: {0, 1}, rights: {0, 1}} and
        //                    {lefts: {2},    rights: {2}}.
        assert_eq!(blocks.len(), 2);
        let has_pair_block = blocks
            .iter()
            .any(|b| b.lefts.len() == 2 && b.rights.len() == 2);
        let has_singleton_block = blocks
            .iter()
            .any(|b| b.lefts == [2].into_iter().collect::<BTreeSet<_>>());
        assert!(has_pair_block);
        assert!(has_singleton_block);
    }
}
