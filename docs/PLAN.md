---
title: LinkedIn Queens — Solver Plan
---

# LinkedIn Queens — Solver Plan

A command-line solver for [LinkedIn Queens](QUEENS.html) puzzles implemented purely through heuristic propagation. The solver does **not** use backtracking. It either solves the board with the heuristics catalogued in [HEURISTICS.html](HEURISTICS.html), or it reports the partially-solved state and exits with a non-zero status.

## Input

The solver reads one board from standard input. The board is a sequence of lines, one line per row of the `N × N` grid. Each character of a line is a region letter, following the format in `archive/boards/<id>.txt` (no separators between cells, one character per cell).

Example 8×8 board (from `archive/boards/1.txt`):

```
BBPPPSSS
BRPRPOSS
BRPRPSSS
BRRRPGNS
BRRRPGNN
BRTRPGNN
TRTRPGGN
TTTTNNNN
```

Parsing notes:

- Lines are trimmed of trailing whitespace. Empty trailing lines are ignored.
- The board size `N` is taken from the width of the first non-empty line. All rows must have the same width, and the number of non-empty rows must equal the width.
- The input is assumed valid per the invariants in QUEENS.md. Exactly `N` distinct region letters appear, and each region is 4-connected.

## Output (success)

When the heuristics converge to a complete solution, the solver writes two sections to standard output, separated by a blank line, and exits with status `0`.

### 1. Solved board

One line per row, using:

- `♛` (U+265B, BLACK CHESS QUEEN) for each queen.
- `.` for every other cell.

Example (for a hypothetical 7×7 solution):

```
...♛...
♛......
..♛....
....♛..
......♛
.♛.....
.....♛.
```

### 2. Heuristic counters

A list of every named heuristic with the number of times it fired during the solve. Zeros are included so that the report is always the same shape. A heuristic "fires" once per pass iteration in which its effect marked at least one new cell dead or placed a queen. Pattern matches that produce no new deduction do not count. The proposed output format is one heuristic per line, name left-padded to a fixed width, followed by the integer count:

```
Queen-adjacent kill              : 7
Single-cell region               : 2
Single-cell row / column         : 0
Region-confined-to-line          : 3
Line-confined-to-region          : 1
Polyomino: domino                : 1
...
Polyomino: T-hexomino            : 0
N-regions-in-N-lines             : 2
Placement-forces-contradiction   : 0
```

## Output (failure)

If the heuristics cannot complete the board, the solver writes the partial state (single section, no counter table) and exits with status `1`. The partial state uses:

- `♛` (U+265B) for placed queens.
- `⨯` (U+2A2F) for dead cells.
- `.` for live cells (cells that are neither queen nor dead).

Example 7×7 partial state:

```
⨯⨯⨯♛⨯⨯⨯
♛⨯⨯⨯⨯⨯⨯
⨯⨯.⨯...
⨯..⨯...
⨯..⨯...
⨯..⨯...
⨯..⨯...
```

## Heuristic catalog and counters

Thirty-one counters in total. Each counter is incremented once per successful application of its rule, meaning one increment per pass iteration in which the rule marked at least one new cell dead or placed a queen. Counters are **not** scaled by the number of cells affected.

### Baseline rules (five counters)

| Counter name | Heuristic |
|---|---|
| `queen_adjacent_kill` | Queen-adjacent kill |
| `single_cell_region` | Single-cell region |
| `single_cell_line` | Single-cell row / column |
| `region_confined_to_line` | Region-confined-to-line |
| `line_confined_to_region` | Line-confined-to-region |

### Polyomino shape rules (twenty-four counters)

One counter per `#####` heading under `### Polyomino` in HEURISTICS.md that carries an actual deduction. The I-tetromino, I-pentomino, and I-hexomino rules have no polyomino-specific deduction (their effect is handled entirely by `region-confined-to-line`), so they do not get their own counters.

| Size | Shape counters |
|---|---|
| 2 (domino) | `polyomino_domino` |
| 3 (tromino) | `polyomino_i_tromino`, `polyomino_l_tromino` |
| 4 (tetromino) | `polyomino_t_tetromino`, `polyomino_l_tetromino`, `polyomino_s_tetromino` |
| 5 (pentomino) | `polyomino_l_pentomino`, `polyomino_f_pentomino`, `polyomino_n_pentomino`, `polyomino_t_pentomino`, `polyomino_u_pentomino`, `polyomino_v_pentomino`, `polyomino_w_pentomino` |
| 6 (hexomino) | `polyomino_z_with_tab_hexomino`, `polyomino_l_hexomino`, `polyomino_n_hexomino`, `polyomino_long_n_hexomino`, `polyomino_c_hexomino`, `polyomino_s_block_hexomino`, `polyomino_f_ext_a_hexomino`, `polyomino_n_ext_a_hexomino`, `polyomino_t_ext_hexomino`, `polyomino_w_ext_hexomino`, `polyomino_t_hexomino` |

### Pigeonhole rules (one counter)

| Counter name | Heuristic |
|---|---|
| `n_regions_in_n_lines` | Combined counter for `N-regions-in-N-rows` and `N-regions-in-N-columns`. Incremented once per pass iteration in which either variant marked a new cell dead. |

### Placement-forces-contradiction (one counter)

| Counter name | Heuristic |
|---|---|
| `placement_forces_contradiction` | Incremented once per pass iteration in which the rule marked at least one candidate cell dead. Because the driver invokes this rule only after the cheap-tier rules have reached a fixed point (see "Solver control flow" below), this counter records how many times the expensive tier was needed to make progress. |

## Solver control flow

1. Read and parse stdin into a `regions` grid. Derive `N`.
2. Initialize `queens` as the empty set and `dead` as all-false.
3. Build the static precomputed tables described in HEURISTICS.md (per-cell kill list, canonical-polyomino-to-dead-offsets hash, bipartite incidence graphs for the N-regions rules, per-cell kill-mask bitmasks for the placement-forces-contradiction rule).
4. Initialize the incremental data structures (live-cell counts, live-cell sets, distinct-rows/cols per region, distinct-regions per row/col, per-region dirty flag, dirty-and-small region set, per-region live-mask bitmasks, N-regions dirty flags, `h2_dirty` flag for the placement-forces-contradiction rule).
5. Run one initial sweep of the cheap-tier rules so that any deductions visible from the start state fire before the main loop.
6. Main loop, two tiers:
    1. **Cheap tier**: repeatedly invoke every cheap rule in the order shown below until a full pass leaves `dead` and `queens` unchanged.
    2. **Expensive tier**: if the cheap tier reached a fixed point with `|queens| < N`, invoke `apply_placement_forces_contradiction` once.
    3. If the expensive tier marked any cell dead, loop back to the cheap tier. Otherwise exit the main loop.
7. After the main loop: if `|queens| == N`, emit the success output. Otherwise emit the failure output and exit `1`.

Cheap-tier rule order within each pass:

1. Baseline rules (likely to fire most often).
2. Polyomino shape rules (iterate the dirty-and-small region set once, look up each region's live-cell canonical form in the shared polyomino-to-dead-offsets table).
3. N-regions-in-N-rows and N-regions-in-N-columns (skipped in `O(1)` when their dirty flags are clear).

With the precomputed structures in place each cheap-tier rule costs `O(B)` per pass (or `O(B²)` for a dirty N-regions pass), so cheap-tier ordering is for clarity rather than performance. The expensive tier is the `placement-forces-contradiction` rule, which runs `O(B²)` per dirty pass but with tight bitmask constants several times larger than the baseline rules. Running it only after the cheap tier stops firing keeps the common case fast and still unlocks the puzzles that need the stronger deduction.

## Implementation notes

- Language: Rust, shipped as a Cargo crate at the project root.
- Crate layout:
  - `Cargo.toml` and `src/` at the project root.
  - `src/main.rs` is the binary entry point. It reads stdin, writes stdout, and sets its exit status (`0` on success, `1` on failure).
  - `src/lib.rs` exposes the core solver types and functions so that unit tests can exercise the solver directly.
  - Module suggestions under `src/`: `board.rs` (parsing, region indexing), `state.rs` (live-cell counts, sets, dirty flags, bitmask live-masks and kill-masks, undo log), `polyomino.rs` (canonical-form hashing and dead-offset table), `nregions.rs` (bipartite graph plus Dulmage–Mendelsohn decomposition), `rules.rs` (individual rule functions, each returning a bool for "fired this pass"; the cheap-tier rules plus the expensive `apply_placement_forces_contradiction`), `counters.rs` (the 31 named counters), and `output.rs` (success and failure formatters).
- Running: `cargo run --release < archive/boards/1.txt` solves a single board.
- Testing: unit tests only. Each module under `src/` carries its own `#[cfg(test)] mod tests` block covering the canonical-form builder, the bipartite matching and decomposition, each individual rule's fire-and-effect behaviour on small crafted boards, and the I/O formatters.
- External crates: keep dependencies light. Standard library is enough for I/O and data structures. A small bipartite-matching helper can live in-tree rather than pulling a crate.
- Unicode: the solver's output is UTF-8 by default in Rust. Terminals that do not render `♛` or `⨯` will show replacement characters.
