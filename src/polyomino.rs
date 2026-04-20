//! Runtime polyomino shape rule lookup.
//!
//! The static table `POLYOMINO_SHAPE_RULES` is produced at build time by
//! `build.rs` from the canonical-polyomino enumerator in
//! `src/polyomino_enum.rs`. Each entry holds the shape's canonical cells
//! (zero-normalized, lex-min under the eight symmetries) and the list of
//! dead-cell offsets that this rule marks whenever it fires.

use crate::polyomino_enum::{canonical, Poly};

/// Apply one of the eight symmetries of the square to a set of `(i32, i32)`
/// offsets. Indices 0..4 are the four rotations, 4..8 are the same four
/// rotations composed with a horizontal reflection.
fn apply_symmetry(cells: &[(i32, i32)], idx: usize) -> Vec<(i32, i32)> {
    cells
        .iter()
        .map(|&(r, c)| match idx {
            0 => (r, c),
            1 => (c, -r),
            2 => (-r, -c),
            3 => (-c, r),
            4 => (r, -c),
            5 => (-c, -r),
            6 => (-r, c),
            7 => (c, r),
            _ => unreachable!(),
        })
        .collect()
}

/// Translate a set of cells so the minimum row and column are both zero,
/// then sort.
fn normalize_i32(cells: &[(i32, i32)]) -> Vec<(i32, i32)> {
    let min_r = cells.iter().map(|&(r, _)| r).min().unwrap();
    let min_c = cells.iter().map(|&(_, c)| c).min().unwrap();
    let mut out: Vec<(i32, i32)> = cells.iter().map(|&(r, c)| (r - min_r, c - min_c)).collect();
    out.sort();
    out
}

/// A polyomino shape rule generated at build time.
#[derive(Debug)]
pub struct Shape {
    /// A human-readable identifier, e.g. `"polyomino_l_tromino"`.
    pub name: &'static str,
    /// The canonical cells of this polyomino, zero-normalized and sorted.
    pub cells: &'static [(i8, i8)],
    /// The offsets (in the same coordinate frame as `cells`) of every cell
    /// that is always dead whenever the region's live cells match this
    /// shape.
    pub dead_offsets: &'static [(i8, i8)],
}

include!(concat!(env!("OUT_DIR"), "/polyomino_table.rs"));

/// If the given set of cells matches one of the known polyomino shape rules,
/// return a reference to that rule's entry (containing its name and dead
/// offsets). Cells are canonicalized before the lookup so rotated and
/// reflected inputs all find the same rule.
pub fn match_shape(cells: &[(i8, i8)]) -> Option<&'static Shape> {
    let canon: Poly = canonical(cells);
    // Linear scan: at most a few dozen entries, comparisons are on short
    // slices, so branch prediction handles this faster than any HashMap.
    POLYOMINO_SHAPE_RULES
        .iter()
        .find(|shape| shape.cells.len() == canon.len() && shape.cells == canon.as_slice())
}

/// Index of the matched shape within [`POLYOMINO_SHAPE_RULES`], or `None`
/// if no shape matches. Useful for counter bookkeeping.
pub fn match_shape_index(cells: &[(i8, i8)]) -> Option<usize> {
    let canon: Poly = canonical(cells);
    POLYOMINO_SHAPE_RULES
        .iter()
        .position(|shape| shape.cells.len() == canon.len() && shape.cells == canon.as_slice())
}

/// Match a region's live cells against the polyomino shape table and, if a
/// match is found, translate the shape's `dead_offsets` back into the
/// board's coordinate frame so callers can pass them straight to
/// `State::kill_cell`.
///
/// Returns a pair `(shape_index, dead_cells_in_board_coords)` on success.
pub fn dead_cells_for_region(
    region_cells: &[(usize, usize)],
) -> Option<(usize, Vec<(usize, usize)>)> {
    if region_cells.is_empty() {
        return None;
    }
    let region_i32: Vec<(i32, i32)> = region_cells
        .iter()
        .map(|&(r, c)| (r as i32, c as i32))
        .collect();
    let region_min_r = region_i32.iter().map(|&(r, _)| r).min().unwrap();
    let region_min_c = region_i32.iter().map(|&(_, c)| c).min().unwrap();
    let region_norm = normalize_i32(&region_i32);

    for (shape_idx, shape) in POLYOMINO_SHAPE_RULES.iter().enumerate() {
        if shape.cells.len() != region_cells.len() {
            continue;
        }
        let shape_cells_i32: Vec<(i32, i32)> =
            shape.cells.iter().map(|&(r, c)| (r as i32, c as i32)).collect();
        for t_idx in 0..8 {
            let shape_transformed = apply_symmetry(&shape_cells_i32, t_idx);
            let shape_normed = normalize_i32(&shape_transformed);
            if shape_normed != region_norm {
                continue;
            }
            // Found the matching symmetry. Transform the dead offsets the
            // same way and translate them into the board's coordinate frame.
            let shape_dead_i32: Vec<(i32, i32)> = shape
                .dead_offsets
                .iter()
                .map(|&(r, c)| (r as i32, c as i32))
                .collect();
            let dead_transformed = apply_symmetry(&shape_dead_i32, t_idx);
            let shift_r = shape_transformed.iter().map(|&(r, _)| r).min().unwrap();
            let shift_c = shape_transformed.iter().map(|&(_, c)| c).min().unwrap();
            let mut out: Vec<(usize, usize)> = Vec::new();
            for &(dr, dc) in &dead_transformed {
                let board_r = dr - shift_r + region_min_r;
                let board_c = dc - shift_c + region_min_c;
                if board_r >= 0 && board_c >= 0 {
                    out.push((board_r as usize, board_c as usize));
                }
            }
            return Some((shape_idx, out));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_has_expected_entry_count() {
        // One domino + two trominoes + three tetrominoes + seven pentominoes
        // (including the L-pentomino) + eleven hexominoes.
        assert_eq!(POLYOMINO_SHAPE_RULES.len(), 1 + 2 + 3 + 7 + 11);
    }

    #[test]
    fn every_entry_has_unique_name() {
        let mut names: Vec<&str> = POLYOMINO_SHAPE_RULES.iter().map(|s| s.name).collect();
        let count = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), count, "duplicate shape name in table");
    }

    #[test]
    fn every_entry_has_at_least_one_dead_offset() {
        for shape in POLYOMINO_SHAPE_RULES {
            assert!(
                !shape.dead_offsets.is_empty(),
                "shape {} has empty dead_offsets",
                shape.name
            );
        }
    }

    #[test]
    fn match_shape_identifies_the_domino() {
        let shape = match_shape(&[(0, 0), (0, 1)]).expect("domino present");
        assert_eq!(shape.name, "polyomino_domino");
        assert_eq!(shape.dead_offsets.len(), 4);
    }

    #[test]
    fn match_shape_is_orientation_independent() {
        // Pass a vertical domino, a rotated L-tromino, and a reflected
        // S-tetromino; each should land on its canonical rule.
        let vertical_domino = match_shape(&[(5, 5), (6, 5)]).expect("domino match");
        assert_eq!(vertical_domino.name, "polyomino_domino");

        let rotated_l = match_shape(&[(0, 0), (0, 1), (1, 1)]).expect("L-tromino match");
        assert_eq!(rotated_l.name, "polyomino_l_tromino");

        let reflected_s = match_shape(&[(0, 0), (0, 1), (1, 1), (1, 2)]).expect("S-tetromino match");
        assert_eq!(reflected_s.name, "polyomino_s_tetromino");
    }

    #[test]
    fn match_shape_returns_none_for_unknown_shape() {
        // The O-tetromino (2x2 block) has no polyomino-specific deduction
        // and is intentionally absent from the table.
        assert!(match_shape(&[(0, 0), (0, 1), (1, 0), (1, 1)]).is_none());
    }

    #[test]
    fn l_pentomino_is_present() {
        let shape =
            match_shape(&[(0, 0), (1, 0), (2, 0), (3, 0), (3, 1)]).expect("L-pentomino present");
        assert_eq!(shape.name, "polyomino_l_pentomino");
        assert_eq!(shape.dead_offsets.len(), 1);
    }

    #[test]
    fn f_pentomino_matches_by_name() {
        let shape =
            match_shape(&[(0, 1), (0, 2), (1, 0), (1, 1), (2, 1)]).expect("F-pentomino present");
        assert_eq!(shape.name, "polyomino_f_pentomino");
    }

    #[test]
    fn l_hexomino_matches_by_name() {
        let shape = match_shape(&[(0, 0), (0, 1), (0, 2), (0, 3), (0, 4), (1, 0)])
            .expect("L-hexomino present");
        assert_eq!(shape.name, "polyomino_l_hexomino");
    }

    #[test]
    fn dead_cells_for_horizontal_domino_are_flanks() {
        // Horizontal domino at row 3, cols 5-6. Flanks are at row 2 and 4,
        // cols 5-6.
        let (idx, dead) = dead_cells_for_region(&[(3, 5), (3, 6)]).expect("matches");
        assert_eq!(POLYOMINO_SHAPE_RULES[idx].name, "polyomino_domino");
        let set: std::collections::BTreeSet<(usize, usize)> = dead.into_iter().collect();
        let expected: std::collections::BTreeSet<(usize, usize)> =
            [(2, 5), (2, 6), (4, 5), (4, 6)].iter().copied().collect();
        assert_eq!(set, expected);
    }

    #[test]
    fn dead_cells_for_vertical_domino_are_flanks() {
        // Vertical domino at col 5, rows 3-4. Flanks are at col 4 and 6,
        // rows 3-4.
        let (idx, dead) = dead_cells_for_region(&[(3, 5), (4, 5)]).expect("matches");
        assert_eq!(POLYOMINO_SHAPE_RULES[idx].name, "polyomino_domino");
        let set: std::collections::BTreeSet<(usize, usize)> = dead.into_iter().collect();
        let expected: std::collections::BTreeSet<(usize, usize)> =
            [(3, 4), (3, 6), (4, 4), (4, 6)].iter().copied().collect();
        assert_eq!(set, expected);
    }

    #[test]
    fn dead_cells_for_l_tromino_match_expected() {
        // L-tromino with elbow at (5, 5): (5, 5), (5, 6), (6, 5).
        // Expected dead cells: missing corner (6, 6), left-of-elbow (5, 4),
        // above-elbow (4, 5).
        let (idx, dead) = dead_cells_for_region(&[(5, 5), (5, 6), (6, 5)]).expect("matches");
        assert_eq!(POLYOMINO_SHAPE_RULES[idx].name, "polyomino_l_tromino");
        let set: std::collections::BTreeSet<(usize, usize)> = dead.into_iter().collect();
        let expected: std::collections::BTreeSet<(usize, usize)> =
            [(4, 5), (5, 4), (6, 6)].iter().copied().collect();
        assert_eq!(set, expected);
    }

    #[test]
    fn dead_cells_returns_none_for_unknown_shape() {
        // O-tetromino is not in the table.
        let o = dead_cells_for_region(&[(0, 0), (0, 1), (1, 0), (1, 1)]);
        assert!(o.is_none());
    }
}
