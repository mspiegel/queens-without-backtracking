//! Stdout formatters for the solver's two output modes, per PLAN.md.
//!
//! * [`format_success`]: fully-solved board (`♛` / `.`) followed by the
//!   heuristic counter table, separated by a blank line.
//! * [`format_failure`]: partial state using `♛` / `⨯` / `.`.

use std::fmt::Write;

use crate::counters::Counters;
use crate::state::State;

/// Unicode queen glyph (U+265B BLACK CHESS QUEEN).
pub const QUEEN: char = '♛';
/// Unicode dead-cell glyph (U+2A2F VECTOR OR CROSS PRODUCT).
pub const DEAD: char = '⨯';
/// Live-cell / empty-cell glyph.
pub const EMPTY: char = '.';

/// Format a fully-solved state. Every non-queen cell is rendered as `.`.
/// The counter table is appended after a blank line.
pub fn format_success(state: &State, counters: &Counters) -> String {
    let mut out = String::new();
    for r in 0..state.n {
        for c in 0..state.n {
            let idx = r * state.n + c;
            let ch = if state.queen[idx] { QUEEN } else { EMPTY };
            out.push(ch);
        }
        out.push('\n');
    }
    out.push('\n');

    // Counter table. Determine max name width so counts line up.
    let named = counters.named_values();
    let name_width = named.iter().map(|(name, _)| name.len()).max().unwrap_or(0);
    for (name, count) in named {
        // Left-pad the name to `name_width` spaces so the colons align.
        let _ = writeln!(&mut out, "{name:<name_width$} : {count}", name_width = name_width);
    }
    out
}

/// Format a partial state (when the solver stalls). Queens, dead cells, and
/// live cells are rendered as `♛`, `⨯`, and `.` respectively.
pub fn format_failure(state: &State) -> String {
    let mut out = String::new();
    for r in 0..state.n {
        for c in 0..state.n {
            let idx = r * state.n + c;
            let ch = if state.queen[idx] {
                QUEEN
            } else if state.dead[idx] {
                DEAD
            } else {
                EMPTY
            };
            out.push(ch);
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board;
    use crate::counters::RuleId;

    fn small_board() -> board::Board {
        let input = "\
AABB
AABB
CCDD
CCDD
";
        board::parse(input).expect("parse")
    }

    #[test]
    fn format_success_writes_board_then_counter_table() {
        let b = small_board();
        let mut state = State::new(&b);
        let mut counters = Counters::new();
        // Synthesize a "solved" state by directly placing four queens so
        // the solver loop doesn't have to run. This is enough to verify
        // formatter output.
        state.place_queen(0, 1);
        state.place_queen(1, 3);
        state.place_queen(2, 0);
        state.place_queen(3, 2);
        counters.increment(RuleId::QueenAdjacentKill);
        counters.increment(RuleId::SingleCellRegion);
        let text = format_success(&state, &counters);
        let expected_board_section = "\
.♛..
...♛
♛...
..♛.
";
        assert!(text.starts_with(expected_board_section), "board prefix mismatch: {text:?}");
        assert!(
            text.contains("Queen-adjacent kill"),
            "counter table missing Queen-adjacent kill"
        );
        assert!(
            text.contains("N-regions-in-N-lines"),
            "counter table missing N-regions-in-N-lines"
        );
        // There should be one blank line between the board and the counter
        // table.
        assert!(
            text.contains("\n\nQueen-adjacent"),
            "expected blank separator between board and counters"
        );
    }

    #[test]
    fn format_failure_renders_dead_live_and_queen_cells() {
        let b = small_board();
        let mut state = State::new(&b);
        state.place_queen(0, 0);
        // (1, 2) is not killed by that queen's kill list, let us verify it
        // is still rendered as live ('.').
        let text = format_failure(&state);
        // Row 0 col 0 is the queen.
        assert_eq!(text.chars().next().unwrap(), QUEEN);
        // Row 0 col 1 is in row 0, so it died via queen-adjacent kill.
        let row0: Vec<char> = text.lines().next().unwrap().chars().collect();
        assert_eq!(row0[1], DEAD);
        assert_eq!(row0[2], DEAD);
        assert_eq!(row0[3], DEAD);
        // Row 2 col 2 should still be live (not in queen's kill list).
        let row2: Vec<char> = text.lines().nth(2).unwrap().chars().collect();
        assert_eq!(row2[2], EMPTY);
        // No counter table in the failure output.
        assert!(!text.contains("Queen-adjacent kill"));
    }
}
