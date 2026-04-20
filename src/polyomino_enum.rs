// Free-polyomino enumeration primitives, shared between build.rs and the
// main crate's tests. See HEURISTICS.md and PLAN.md for context.
//
// A "free polyomino" groups every rotation and reflection of the same shape
// together. The canonical form is the lexicographically smallest element
// among the eight symmetries (four rotations times two reflections).
//
// This file is included in two ways:
//   * As `pub mod polyomino_enum;` from the main crate (tests live below).
//   * Via `include!("src/polyomino_enum.rs")` from build.rs.
// Inner doc comments (`//!`) would be rejected by the include! path, so
// every file-level comment here uses plain `//` syntax.

use std::collections::BTreeSet;

/// A polyomino is represented as a sorted, zero-normalized list of cells.
pub type Poly = Vec<(i8, i8)>;

/// Translate so the minimum row and column are both zero, then sort.
pub fn normalize(cells: &[(i8, i8)]) -> Poly {
    let min_r = cells.iter().map(|&(r, _)| r).min().unwrap();
    let min_c = cells.iter().map(|&(_, c)| c).min().unwrap();
    let mut out: Poly = cells.iter().map(|&(r, c)| (r - min_r, c - min_c)).collect();
    out.sort();
    out
}

/// 90-degree clockwise rotation: `(r, c) -> (c, -r)`.
pub fn rotate(cells: &[(i8, i8)]) -> Poly {
    let rotated: Poly = cells.iter().map(|&(r, c)| (c, -r)).collect();
    normalize(&rotated)
}

/// Horizontal reflection: `(r, c) -> (r, -c)`.
pub fn reflect(cells: &[(i8, i8)]) -> Poly {
    let reflected: Poly = cells.iter().map(|&(r, c)| (r, -c)).collect();
    normalize(&reflected)
}

/// Canonical form: the lexicographic minimum over the eight symmetries.
pub fn canonical(cells: &[(i8, i8)]) -> Poly {
    let mut current = normalize(cells);
    let mut best = current.clone();
    for _ in 0..4 {
        if current < best {
            best = current.clone();
        }
        let r = reflect(&current);
        if r < best {
            best = r;
        }
        current = rotate(&current);
    }
    best
}

/// Given a set of size-N canonical polyominoes, extend each by one orthogonally
/// adjacent cell to produce the set of size-(N+1) canonical polyominoes.
pub fn grow(polyominoes: &BTreeSet<Poly>) -> BTreeSet<Poly> {
    let mut next: BTreeSet<Poly> = BTreeSet::new();
    for p in polyominoes {
        let cell_set: BTreeSet<(i8, i8)> = p.iter().copied().collect();
        for &(r, c) in p {
            for (dr, dc) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                let nr = r + dr;
                let nc = c + dc;
                if cell_set.contains(&(nr, nc)) {
                    continue;
                }
                let mut extended: Poly = p.clone();
                extended.push((nr, nc));
                next.insert(canonical(&extended));
            }
        }
    }
    next
}

/// Returns the one-element set containing the monomino, useful as a seed
/// for `grow`.
pub fn monomino() -> BTreeSet<Poly> {
    let mut s = BTreeSet::new();
    s.insert(canonical(&[(0, 0)]));
    s
}

/// Compute the set of polyomino-specific always-dead cell offsets for a shape.
///
/// A cell `(r, c)` outside the shape is "always dead" if, for every possible
/// queen position inside the shape, `(r, c)` is in the queen's row, the
/// queen's column, or the queen's king-neighborhood (Chebyshev distance 1).
///
/// If the shape is confined to a single row (or single column), the cells on
/// that shared row (column) are instead handled by the baseline
/// `region-confined-to-line` rule and are therefore excluded from the
/// polyomino-specific offsets returned here.
///
/// The returned offsets are in the same coordinate frame as `cells`. Callers
/// typically normalize `cells` first so the offsets are relative to a clean
/// zero-origin bounding box.
pub fn always_dead_offsets(cells: &[(i8, i8)]) -> Vec<(i8, i8)> {
    let shape: BTreeSet<(i8, i8)> = cells.iter().copied().collect();
    if shape.is_empty() {
        return Vec::new();
    }
    let min_r = shape.iter().map(|&(r, _)| r).min().unwrap();
    let max_r = shape.iter().map(|&(r, _)| r).max().unwrap();
    let min_c = shape.iter().map(|&(_, c)| c).min().unwrap();
    let max_c = shape.iter().map(|&(_, c)| c).max().unwrap();

    // Cells confined to a shared row or column extend arbitrarily far on
    // that line, but `region-confined-to-line` handles them. We filter those
    // out so the polyomino rule only reports genuinely polyomino-specific
    // deductions.
    let single_row = min_r == max_r;
    let single_col = min_c == max_c;

    // Cells outside the bounding box by more than one cell in either
    // direction cannot be always-dead for non-line shapes: row and column
    // exclusion only fire for queens that share the row or column, and
    // king-adjacency fails for cells more than one step from the nearest
    // queen. Searching a ring of radius 2 around the bounding box therefore
    // catches every relevant cell.
    let radius: i8 = 2;
    let mut dead: Vec<(i8, i8)> = Vec::new();
    for r in (min_r - radius)..=(max_r + radius) {
        for c in (min_c - radius)..=(max_c + radius) {
            if shape.contains(&(r, c)) {
                continue;
            }
            if single_row && r == min_r {
                continue;
            }
            if single_col && c == min_c {
                continue;
            }
            let killed_by_all = shape.iter().all(|&(qr, qc)| {
                let row_kill = r == qr;
                let col_kill = c == qc;
                let king_kill = (r - qr).abs() <= 1 && (c - qc).abs() <= 1;
                row_kill || col_kill || king_kill
            });
            if killed_by_all {
                dead.push((r, c));
            }
        }
    }

    dead.sort();
    dead
}

#[cfg(test)]
mod tests {
    use super::*;

    fn domino_horizontal() -> Poly {
        canonical(&[(0, 0), (0, 1)])
    }

    fn domino_vertical() -> Poly {
        canonical(&[(0, 0), (1, 0)])
    }

    // ---- primitive behaviour ----

    #[test]
    fn normalize_is_idempotent() {
        let cells = [(2, 3), (2, 4), (3, 3)];
        let once = normalize(&cells);
        let twice = normalize(&once);
        assert_eq!(once, twice);
    }

    #[test]
    fn normalize_translates_to_origin() {
        let cells = [(5, 7), (5, 8), (6, 7)];
        let n = normalize(&cells);
        let min_r = n.iter().map(|&(r, _)| r).min().unwrap();
        let min_c = n.iter().map(|&(_, c)| c).min().unwrap();
        assert_eq!(min_r, 0);
        assert_eq!(min_c, 0);
    }

    #[test]
    fn rotate_four_times_is_identity() {
        let start = normalize(&[(0, 0), (0, 1), (1, 1)]);
        let mut current = start.clone();
        for _ in 0..4 {
            current = rotate(&current);
        }
        assert_eq!(current, start);
    }

    #[test]
    fn reflect_twice_is_identity() {
        let start = normalize(&[(0, 0), (0, 1), (1, 1)]);
        let once = reflect(&start);
        let twice = reflect(&once);
        assert_eq!(twice, start);
    }

    #[test]
    fn canonical_agrees_across_all_eight_symmetries() {
        // An L-tromino has no internal symmetry so all 8 variants are distinct,
        // but every one of them must canonicalize to the same shape.
        let base = normalize(&[(0, 0), (0, 1), (1, 0)]);
        let canon = canonical(&base);
        let mut current = base;
        for _ in 0..4 {
            assert_eq!(canonical(&current), canon);
            assert_eq!(canonical(&reflect(&current)), canon);
            current = rotate(&current);
        }
    }

    #[test]
    fn canonical_maps_horizontal_and_vertical_domino_together() {
        assert_eq!(domino_horizontal(), domino_vertical());
    }

    // ---- 2a-1 size-2 generation ----

    #[test]
    fn size_two_produces_exactly_one_domino() {
        let grown = grow(&monomino());
        assert_eq!(grown.len(), 1);
        assert!(grown.contains(&domino_horizontal()));
    }

    // ---- 2a-2 size-3 generation ----

    #[test]
    fn size_three_produces_exactly_two_trominoes() {
        let size_2 = grow(&monomino());
        let size_3 = grow(&size_2);
        assert_eq!(size_3.len(), 2);

        // Expect the I-tromino (three in a row) and the L-tromino (2x2 minus a corner).
        let i_tromino = canonical(&[(0, 0), (0, 1), (0, 2)]);
        let l_tromino = canonical(&[(0, 0), (0, 1), (1, 0)]);
        assert!(size_3.contains(&i_tromino));
        assert!(size_3.contains(&l_tromino));
    }

    // ---- 2a-3 size-4 generation ----

    #[test]
    fn size_four_produces_exactly_five_tetrominoes() {
        let size_2 = grow(&monomino());
        let size_3 = grow(&size_2);
        let size_4 = grow(&size_3);
        assert_eq!(size_4.len(), 5);

        // Check each of the five expected tetrominoes individually.
        let i_tetromino = canonical(&[(0, 0), (0, 1), (0, 2), (0, 3)]);
        let o_tetromino = canonical(&[(0, 0), (0, 1), (1, 0), (1, 1)]);
        let t_tetromino = canonical(&[(0, 0), (0, 1), (0, 2), (1, 1)]);
        let l_tetromino = canonical(&[(0, 0), (1, 0), (2, 0), (2, 1)]);
        let s_tetromino = canonical(&[(0, 1), (0, 2), (1, 0), (1, 1)]);
        for shape in [&i_tetromino, &o_tetromino, &t_tetromino, &l_tetromino, &s_tetromino] {
            assert!(size_4.contains(shape), "missing tetromino {:?}", shape);
        }
    }

    // ---- 2f verify pentominoes (n=5) ----

    #[test]
    fn dead_offsets_for_all_pentominoes_match_heuristics() {
        // Twelve free pentominoes. Per HEURISTICS.md only six of them
        // produce polyomino-specific deductions:
        //   F -> 1, N -> 2, T -> 1, U -> 1, V -> 1, W -> 1.
        // I, L, P, X, Y, Z produce zero dead offsets in this catalog
        // (I is line-confined; the others have no cell killed by every queen).
        let size_2 = grow(&monomino());
        let size_3 = grow(&size_2);
        let size_4 = grow(&size_3);
        let size_5 = grow(&size_4);
        assert_eq!(size_5.len(), 12);

        let f_pentomino = canonical(&[(0, 1), (0, 2), (1, 0), (1, 1), (2, 1)]);
        let i_pentomino = canonical(&[(0, 0), (0, 1), (0, 2), (0, 3), (0, 4)]);
        let l_pentomino = canonical(&[(0, 0), (1, 0), (2, 0), (3, 0), (3, 1)]);
        let n_pentomino = canonical(&[(0, 1), (1, 1), (2, 0), (2, 1), (3, 0)]);
        let p_pentomino = canonical(&[(0, 0), (0, 1), (1, 0), (1, 1), (2, 0)]);
        let t_pentomino = canonical(&[(0, 0), (0, 1), (0, 2), (1, 1), (2, 1)]);
        let u_pentomino = canonical(&[(0, 0), (0, 2), (1, 0), (1, 1), (1, 2)]);
        let v_pentomino = canonical(&[(0, 0), (1, 0), (2, 0), (2, 1), (2, 2)]);
        let w_pentomino = canonical(&[(0, 0), (1, 0), (1, 1), (2, 1), (2, 2)]);
        let x_pentomino = canonical(&[(0, 1), (1, 0), (1, 1), (1, 2), (2, 1)]);
        let y_pentomino = canonical(&[(0, 0), (1, 0), (1, 1), (2, 0), (3, 0)]);
        let z_pentomino = canonical(&[(0, 0), (0, 1), (1, 1), (2, 1), (2, 2)]);

        // The six shapes HEURISTICS.md catalogs.
        assert_eq!(always_dead_offsets(&f_pentomino).len(), 1);
        assert_eq!(always_dead_offsets(&n_pentomino).len(), 2);
        assert_eq!(always_dead_offsets(&t_pentomino).len(), 1);
        assert_eq!(always_dead_offsets(&u_pentomino).len(), 1);
        assert_eq!(always_dead_offsets(&v_pentomino).len(), 1);
        assert_eq!(always_dead_offsets(&w_pentomino).len(), 1);

        // The I-pentomino is line-confined so its offsets are all filtered.
        assert_eq!(always_dead_offsets(&i_pentomino).len(), 0);

        // Consume the other shape canonical forms so Rust does not warn
        // about unused bindings. Their exact deduction counts are not
        // constrained by this test; whatever the case-analysis computes is
        // kept in the generated table.
        let _ = (&l_pentomino, &p_pentomino, &x_pentomino, &y_pentomino, &z_pentomino);
    }

    // ---- 2e verify tetrominoes (n=4) ----

    #[test]
    fn dead_offsets_for_all_tetrominoes_match_heuristics() {
        // Five free tetrominoes. Per HEURISTICS.md:
        //   I -> 0 (no polyomino-specific deduction; line-confined).
        //   O -> 0 (no external cell is killed by every queen).
        //   T -> 1 (plus-completer).
        //   L -> 2 (2x2 corner-completer plus long-arm extension).
        //   S -> 2 (two rectangle-completers).
        let size_2 = grow(&monomino());
        let size_3 = grow(&size_2);
        let size_4 = grow(&size_3);
        assert_eq!(size_4.len(), 5);

        let i_tetromino = canonical(&[(0, 0), (0, 1), (0, 2), (0, 3)]);
        let o_tetromino = canonical(&[(0, 0), (0, 1), (1, 0), (1, 1)]);
        let t_tetromino = canonical(&[(0, 0), (0, 1), (0, 2), (1, 1)]);
        let l_tetromino = canonical(&[(0, 0), (1, 0), (2, 0), (2, 1)]);
        let s_tetromino = canonical(&[(0, 1), (0, 2), (1, 0), (1, 1)]);

        assert_eq!(always_dead_offsets(&i_tetromino).len(), 0);
        assert_eq!(always_dead_offsets(&o_tetromino).len(), 0);
        assert_eq!(always_dead_offsets(&t_tetromino).len(), 1);
        assert_eq!(always_dead_offsets(&l_tetromino).len(), 2);
        assert_eq!(always_dead_offsets(&s_tetromino).len(), 2);

        let total: usize = size_4.iter().map(|p| always_dead_offsets(p).len()).sum();
        assert_eq!(total, 0 + 0 + 1 + 2 + 2);
    }

    // ---- 2d verify trominoes (n=3) ----

    #[test]
    fn dead_offsets_for_all_trominoes_total_five() {
        // Two free trominoes:
        //   I-tromino -> 2 dead offsets (flanks of the middle cell).
        //   L-tromino -> 3 dead offsets (missing corner plus two elbow
        //                                extensions).
        let size_2 = grow(&monomino());
        let size_3 = grow(&size_2);
        assert_eq!(size_3.len(), 2);

        let i_tromino = canonical(&[(0, 0), (0, 1), (0, 2)]);
        let l_tromino = canonical(&[(0, 0), (0, 1), (1, 0)]);

        let i_offsets = always_dead_offsets(&i_tromino);
        assert_eq!(i_offsets.len(), 2, "I-tromino should have 2 dead offsets");

        let l_offsets = always_dead_offsets(&l_tromino);
        assert_eq!(l_offsets.len(), 3, "L-tromino should have 3 dead offsets");

        let total: usize = size_3.iter().map(|p| always_dead_offsets(p).len()).sum();
        assert_eq!(total, 5);
    }

    // ---- 2c verify domino (n=2) ----

    #[test]
    fn dead_offsets_for_all_dominoes_total_four() {
        // Exactly one free domino. Its polyomino-specific dead-cell offsets
        // are the four flanks above and below the domino (the shared-row
        // cells are handled by region-confined-to-line and filtered out).
        let size_2 = grow(&monomino());
        assert_eq!(size_2.len(), 1);
        let mut total = 0;
        for shape in &size_2 {
            let offsets = always_dead_offsets(shape);
            assert_eq!(offsets.len(), 4, "domino should have 4 dead offsets");
            total += offsets.len();
        }
        assert_eq!(total, 4);
    }

    // ---- 2b always-dead cell computer ----

    #[test]
    fn dead_offsets_for_monomino_are_four_diagonal_neighbors() {
        // The monomino is line-confined to row 0 and column 0, so those are
        // filtered out. The only remaining always-dead cells are the four
        // diagonal king-adjacent neighbors. Real polyomino rules start at
        // size 2 so this case is only exercised by tests.
        let offsets = always_dead_offsets(&[(0, 0)]);
        let expected: Vec<(i8, i8)> = vec![(-1, -1), (-1, 1), (1, -1), (1, 1)];
        assert_eq!(offsets, expected);
    }

    #[test]
    fn dead_offsets_for_horizontal_domino_are_four_flanks() {
        let offsets = always_dead_offsets(&[(0, 0), (0, 1)]);
        let expected: Vec<(i8, i8)> = vec![(-1, 0), (-1, 1), (1, 0), (1, 1)];
        assert_eq!(offsets, expected);
    }

    #[test]
    fn dead_offsets_exclude_shared_row_for_line_confined_shape() {
        // An I-tromino (horizontal) has all cells in one row. Only the two
        // flanks above and below the middle cell are polyomino-specific.
        let offsets = always_dead_offsets(&[(0, 0), (0, 1), (0, 2)]);
        let expected: Vec<(i8, i8)> = vec![(-1, 1), (1, 1)];
        assert_eq!(offsets, expected);
    }

    #[test]
    fn dead_offsets_for_l_tromino_are_three_cells() {
        // Elbow at (0, 0), arms at (0, 1) and (1, 0). Expect the missing
        // corner (1, 1), plus the two elbow extensions (0, -1) and (-1, 0).
        let offsets = always_dead_offsets(&[(0, 0), (0, 1), (1, 0)]);
        let mut expected: Vec<(i8, i8)> = vec![(-1, 0), (0, -1), (1, 1)];
        expected.sort();
        assert_eq!(offsets, expected);
    }

    // ---- 2g verify hexominoes (n=6) ----

    #[test]
    fn dead_offsets_for_all_hexominoes_total_twelve() {
        // Per HEURISTICS.md and the enumerator: 11 of the 35 free hexominoes
        // carry a polyomino-specific deduction, contributing 12 dead cells
        // in total (ten shapes with one dead cell each plus the long-N
        // hexomino with two).
        let size_2 = grow(&monomino());
        let size_3 = grow(&size_2);
        let size_4 = grow(&size_3);
        let size_5 = grow(&size_4);
        let size_6 = grow(&size_5);
        assert_eq!(size_6.len(), 35);

        let mut positive = 0;
        let mut total = 0;
        for shape in &size_6 {
            let offsets = always_dead_offsets(shape);
            if !offsets.is_empty() {
                positive += 1;
                total += offsets.len();
            }
        }
        assert_eq!(positive, 11);
        assert_eq!(total, 12);
    }

    // ---- 2a-5 size-6 generation ----

    #[test]
    fn size_six_produces_exactly_thirty_five_hexominoes() {
        let size_2 = grow(&monomino());
        let size_3 = grow(&size_2);
        let size_4 = grow(&size_3);
        let size_5 = grow(&size_4);
        let size_6 = grow(&size_5);
        assert_eq!(size_6.len(), 35);

        // Spot-check: the I-hexomino (six in a row) and the O-hexomino (2x3 rectangle)
        // are both easy to recognise.
        let i_hexomino = canonical(&[(0, 0), (0, 1), (0, 2), (0, 3), (0, 4), (0, 5)]);
        let o_hexomino = canonical(&[(0, 0), (0, 1), (0, 2), (1, 0), (1, 1), (1, 2)]);
        assert!(size_6.contains(&i_hexomino));
        assert!(size_6.contains(&o_hexomino));
    }

    // ---- 2a-4 size-5 generation ----

    #[test]
    fn size_five_produces_exactly_twelve_pentominoes() {
        let size_2 = grow(&monomino());
        let size_3 = grow(&size_2);
        let size_4 = grow(&size_3);
        let size_5 = grow(&size_4);
        assert_eq!(size_5.len(), 12);

        // Spot-check a handful of the named pentominoes.
        let i_pentomino = canonical(&[(0, 0), (0, 1), (0, 2), (0, 3), (0, 4)]);
        let f_pentomino = canonical(&[(0, 1), (0, 2), (1, 0), (1, 1), (2, 1)]);
        let n_pentomino = canonical(&[(0, 1), (1, 1), (2, 0), (2, 1), (3, 0)]);
        let t_pentomino = canonical(&[(0, 0), (0, 1), (0, 2), (1, 1), (2, 1)]);
        let u_pentomino = canonical(&[(0, 0), (0, 2), (1, 0), (1, 1), (1, 2)]);
        let v_pentomino = canonical(&[(0, 0), (1, 0), (2, 0), (2, 1), (2, 2)]);
        let w_pentomino = canonical(&[(0, 0), (1, 0), (1, 1), (2, 1), (2, 2)]);
        let x_pentomino = canonical(&[(0, 1), (1, 0), (1, 1), (1, 2), (2, 1)]);
        let z_pentomino = canonical(&[(0, 0), (0, 1), (1, 1), (2, 1), (2, 2)]);
        let l_pentomino = canonical(&[(0, 0), (1, 0), (2, 0), (3, 0), (3, 1)]);
        let p_pentomino = canonical(&[(0, 0), (0, 1), (1, 0), (1, 1), (2, 0)]);
        let y_pentomino = canonical(&[(0, 0), (1, 0), (1, 1), (2, 0), (3, 0)]);
        for shape in [
            &i_pentomino,
            &f_pentomino,
            &n_pentomino,
            &t_pentomino,
            &u_pentomino,
            &v_pentomino,
            &w_pentomino,
            &x_pentomino,
            &z_pentomino,
            &l_pentomino,
            &p_pentomino,
            &y_pentomino,
        ] {
            assert!(size_5.contains(shape), "missing pentomino {:?}", shape);
        }
    }
}
