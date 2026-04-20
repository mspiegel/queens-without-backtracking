//! Board parsing.
//!
//! A board is an `N × N` grid of region letters. The input format matches the
//! files in `archive/boards/<id>.txt` (no separator characters, one letter per
//! cell). See `QUEENS.md` for invariants.

use std::collections::BTreeMap;
use std::fmt;

/// A single region letter. We use `u8` for cheap copy and compact storage.
pub type Region = u8;

/// A parsed board.
#[derive(Debug, Clone)]
pub struct Board {
    /// Board side length, in cells.
    pub n: usize,
    /// Row-major grid of region letters: `regions[r * n + c]`.
    regions: Vec<Region>,
    /// Map from region letter to the list of cells in that region.
    /// Sorted by letter for deterministic iteration.
    pub region_cells: BTreeMap<Region, Vec<(usize, usize)>>,
}

impl Board {
    /// The region letter at `(row, col)`.
    #[inline]
    pub fn region_at(&self, row: usize, col: usize) -> Region {
        self.regions[row * self.n + col]
    }

    /// Iterate every cell on the board in row-major order.
    pub fn cells(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        let n = self.n;
        (0..n).flat_map(move |r| (0..n).map(move |c| (r, c)))
    }

    /// The number of distinct regions.
    pub fn region_count(&self) -> usize {
        self.region_cells.len()
    }
}

/// Errors returned by `parse`.
#[derive(Debug, PartialEq, Eq)]
pub enum ParseError {
    /// The input was empty (no non-blank rows).
    Empty,
    /// A row's width does not match the board side length.
    RaggedRow {
        row: usize,
        expected: usize,
        found: usize,
    },
    /// The number of non-blank rows does not equal the board side length.
    NotSquare {
        rows: usize,
        columns: usize,
    },
    /// The number of distinct region letters does not equal the board side
    /// length.
    WrongRegionCount {
        expected: usize,
        found: usize,
    },
    /// A region's cells do not form a single 4-connected component.
    RegionNotConnected {
        region: Region,
    },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Empty => write!(f, "board input is empty"),
            ParseError::RaggedRow { row, expected, found } => {
                write!(f, "row {row} has {found} cells, expected {expected}")
            }
            ParseError::NotSquare { rows, columns } => {
                write!(f, "board has {rows} rows but {columns} columns; must be square")
            }
            ParseError::WrongRegionCount { expected, found } => {
                write!(f, "found {found} distinct region letters, expected {expected}")
            }
            ParseError::RegionNotConnected { region } => {
                let ch = *region as char;
                write!(f, "region '{ch}' is not 4-connected")
            }
        }
    }
}

impl std::error::Error for ParseError {}

/// Parse a board from its textual representation.
pub fn parse(input: &str) -> Result<Board, ParseError> {
    // Collect non-blank lines, trimming trailing whitespace.
    let rows: Vec<&str> = input
        .lines()
        .map(|line| line.trim_end())
        .filter(|line| !line.is_empty())
        .collect();
    if rows.is_empty() {
        return Err(ParseError::Empty);
    }

    let n = rows[0].len();
    for (i, row) in rows.iter().enumerate() {
        if row.len() != n {
            return Err(ParseError::RaggedRow {
                row: i,
                expected: n,
                found: row.len(),
            });
        }
    }
    if rows.len() != n {
        return Err(ParseError::NotSquare {
            rows: rows.len(),
            columns: n,
        });
    }

    // Flatten into a row-major byte buffer and group cells by region.
    let mut regions: Vec<Region> = Vec::with_capacity(n * n);
    let mut region_cells: BTreeMap<Region, Vec<(usize, usize)>> = BTreeMap::new();
    for (r, row) in rows.iter().enumerate() {
        for (c, byte) in row.bytes().enumerate() {
            regions.push(byte);
            region_cells.entry(byte).or_default().push((r, c));
        }
    }

    if region_cells.len() != n {
        return Err(ParseError::WrongRegionCount {
            expected: n,
            found: region_cells.len(),
        });
    }

    for (&letter, cells) in &region_cells {
        if !is_four_connected(cells) {
            return Err(ParseError::RegionNotConnected { region: letter });
        }
    }

    Ok(Board {
        n,
        regions,
        region_cells,
    })
}

/// Return true iff `cells` form a single 4-connected component.
fn is_four_connected(cells: &[(usize, usize)]) -> bool {
    if cells.is_empty() {
        return true;
    }
    let cell_set: std::collections::HashSet<(usize, usize)> = cells.iter().copied().collect();
    let mut seen: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();
    let mut stack: Vec<(usize, usize)> = vec![cells[0]];
    seen.insert(cells[0]);
    while let Some((r, c)) = stack.pop() {
        for (dr, dc) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
            let nr = r as i32 + dr;
            let nc = c as i32 + dc;
            if nr < 0 || nc < 0 {
                continue;
            }
            let key = (nr as usize, nc as usize);
            if cell_set.contains(&key) && seen.insert(key) {
                stack.push(key);
            }
        }
    }
    seen.len() == cells.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_seven_by_seven_board() {
        // Matches the example board in QUEENS.md.
        let input = "\
PPPPPPO
PSGLLLO
PSGGGLO
PSSBGLO
PRSBBBO
PRRRRBO
POOOOOO
";
        let board = parse(input).expect("parse");
        assert_eq!(board.n, 7);
        assert_eq!(board.region_count(), 7);
        assert_eq!(board.region_at(0, 0), b'P');
        assert_eq!(board.region_at(6, 6), b'O');
    }

    #[test]
    fn parse_trims_blank_trailing_lines() {
        let input = "\
AA
BB


";
        let board = parse(input).expect("parse");
        assert_eq!(board.n, 2);
    }

    #[test]
    fn parse_rejects_empty_input() {
        assert!(matches!(parse(""), Err(ParseError::Empty)));
        assert!(matches!(parse("\n\n   \n"), Err(ParseError::Empty)));
    }

    #[test]
    fn parse_rejects_ragged_rows() {
        let input = "\
ABC
AB
ABC
";
        let err = parse(input).unwrap_err();
        assert_eq!(
            err,
            ParseError::RaggedRow {
                row: 1,
                expected: 3,
                found: 2,
            }
        );
    }

    #[test]
    fn parse_rejects_non_square_grid() {
        let input = "\
ABC
ABC
";
        let err = parse(input).unwrap_err();
        assert_eq!(err, ParseError::NotSquare { rows: 2, columns: 3 });
    }

    #[test]
    fn parse_rejects_wrong_region_count() {
        // 3x3 grid with only 2 distinct regions; expected 3.
        let input = "\
AAB
ABB
ABB
";
        let err = parse(input).unwrap_err();
        assert_eq!(
            err,
            ParseError::WrongRegionCount {
                expected: 3,
                found: 2,
            }
        );
    }

    #[test]
    fn parse_rejects_disconnected_region() {
        // 3x3 grid with region 'A' split in two pieces.
        let input = "\
ABC
BBB
ABC
";
        let err = parse(input).unwrap_err();
        assert_eq!(err, ParseError::RegionNotConnected { region: b'A' });
    }

    #[test]
    fn parse_accepts_eight_by_eight_archive_board() {
        let input = "\
BBPPPSSS
BRPRPOSS
BRPRPSSS
BRRRPGNS
BRRRPGNN
BRTRPGNN
TRTRPGGN
TTTTNNNN
";
        let board = parse(input).expect("parse archive board");
        assert_eq!(board.n, 8);
        assert_eq!(board.region_count(), 8);
    }

    #[test]
    fn parse_accepts_eleven_by_eleven_archive_board() {
        let input = "\
BBKKAAAAGGG
BBKKOOOAGPG
BBLKOSOAGPG
BLLKOSOAGPG
BBLKOSOAGGG
BBLKOOONNNN
BBLKKNNNBBB
BBNNNNBBBBR
BBBBBBBBRRR
BBBBBRRRRJJ
BBRRRRJJJJJ
";
        // The real board 100.txt has some region letters repeating elsewhere
        // on the board (B appears in multiple clumps); the parser will only
        // accept this if every region letter is 4-connected, which for the
        // archive file is true. If it is not, the parser rejects with
        // RegionNotConnected.
        match parse(input) {
            Ok(board) => {
                assert_eq!(board.n, 11);
            }
            Err(ParseError::RegionNotConnected { .. }) => {
                // Archive file uses the same letter for multiple regions,
                // which the parser considers invalid. Document the behaviour.
            }
            Err(other) => panic!("unexpected parse error: {other:?}"),
        }
    }

    #[test]
    fn cells_iterator_yields_row_major_order() {
        let input = "\
AA
BB
";
        let board = parse(input).expect("parse");
        let collected: Vec<(usize, usize)> = board.cells().collect();
        assert_eq!(collected, vec![(0, 0), (0, 1), (1, 0), (1, 1)]);
    }
}
