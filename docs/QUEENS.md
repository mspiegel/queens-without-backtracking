---
title: LinkedIn Queens — Rules
---

# LinkedIn Queens — Rules

## Data model

A puzzle is an `N × N` grid of cells paired with a region assignment:

```
N          : integer                    // board size, 7..11 in practice
regions    : grid[N][N] of Letter       // regions[r][c] = letter of cell (r, c);
                                        // same letter = same colored region
```

Empirical size distribution across the 668 puzzles in `archive/boards/`:

| `N` | count |
|-----|-------|
| 7   | 112   |
| 8   | 256   |
| 9   | 194   |
| 10  | 74    |
| 11  | 32    |

8×8 is the mode. No 12×12 boards have shipped.

Example for `N = 7` (matches `archive/boards/<id>.txt` format):

```
P P P P P P O
P S G L L L O
P S G G G L O
P S S B G L O
P R S B B B O
P R R R R B O
P O O O O O O
```

Invariants on `regions`:

- Exactly `N` distinct letters appear.
- Each letter's cells form a **single 4-connected component** (no diagonals — only orthogonal neighbors count for region connectedness).

A solution is a set of `N` cell coordinates — one per queen:

```
queens : set of (row, col) pairs        // |queens| = N
```

## Validator

A placement is correct iff all four checks pass:

```
function is_valid_solution(regions, queens):
    N ← number of rows in regions
    if |queens| ≠ N then return false

    // 1. one queen per row
    rows_hit ← { r : (r, c) in queens }
    if |rows_hit| ≠ N then return false

    // 2. one queen per column
    cols_hit ← { c : (r, c) in queens }
    if |cols_hit| ≠ N then return false

    // 3. one queen per colored region
    regions_hit ← { regions[r][c] : (r, c) in queens }
    if |regions_hit| ≠ N then return false

    // 4. no two queens are king-adjacent
    //    (Chebyshev distance ≥ 2; same diagonal is fine if not adjacent)
    foreach unordered pair { (r1, c1), (r2, c2) } in queens:
        if max(|r1 − r2|, |c1 − c2|) < 2 then return false

    return true
```

Rules 1, 2, and 3 each require a bijection between queens and rows/columns/regions respectively. Rule 4 replaces chess-queen attack with the much weaker "kings don't touch" relation.

### Rule 3 is not a free consequence of rules 1 and 2

A naive implementer might assume that forcing one queen per row + one per column somehow covers regions too. It doesn't. Given `N` regions partitioning `N²` cells, there are `N!` placements satisfying rules 1+2 (all permutation matrices of order `N`), and only some of them land one queen in each region. Rule 3 is a genuine third constraint, independent of the other two, and in practice it's the one that determines the puzzle.

### Rule 4 is not the chess queen rule

Don't confuse with the classical `N`-queens problem. In chess, queens attack along the full row, column, **and full diagonal**. In LinkedIn Queens, rows/columns are covered by rules 1 and 2; rule 4 only bans *immediately adjacent* queens (Chebyshev distance `< 2`). Two queens on the same long diagonal are legal as long as they're not touching.

Said another way: rule 4 is the **Moore-neighborhood** adjacency constraint — queens must form an **independent set in the king graph** (the graph whose edges connect every pair of cells a chess king could move between in one move). That's the precise graph-theory term, useful for lookup.

## Dead cells (the X-mark convention)

When humans solve a Queens puzzle on paper or in the LinkedIn UI, every cell is in one of three states:

- **Queen** — a cell with a placed queen (`♛`).
- **Dead cell** — a cell proven to *not* contain a queen in any valid completion of the current partial state, typically drawn with an [X mark](https://en.wikipedia.org/wiki/X_mark) (`✕`).
- **Live cell** — a cell that is neither a queen nor dead: its status is still undetermined.

Dead cells are a solving aid, not part of the puzzle definition. They're exactly what a programmer would call the output of **constraint propagation** over the current partial state. As soon as you place a queen at `(r, c)`, every cell in the same row, same column, same region, or king-adjacent to `(r, c)` becomes dead. More subtle propagations exist — e.g. if a region's cells all lie in a single row, that region consumes the row's queen, killing every cell of that row outside the region — and strong human solvers chain these together.

Call this:

```
dead    : grid[N][N] of boolean         // dead[r][c] = true  ⇒  no queen can go here
is_live : (r, c) → boolean              // is_live(r, c) = not dead[r][c] and (r, c) ∉ queens
```

Useful identities on a partial state `queens ⊆ cells`:

- A cell is dead iff placing a queen there would violate some rule given the current `queens` — or, transitively, given any fact derivable from `queens` plus the rules.
- If a row / column / region has exactly one live cell left, a queen **must** go there (propagation-by-elimination). In human-solver jargon this is a "forced move."
- If every cell in some row / column / region is dead, the current partial state is contradictory and any search branch leading here must backtrack.

A backtracking solver can ignore dead cells entirely and still find the solution (just slower). A solver that tracks them — updating the `dead` grid after each placement and using it to prune candidates — is doing the same thing a human does with a pencil and an X-mark eraser.

### Named propagation rules

The propagation a strong human (and a good solver) runs is a fixed-point of a handful of named rules. Each rule takes the current `(queens, dead)` state and either marks more cells dead, or forces a new queen:

| Rule                     | Fires when                                                     | Effect                                   |
|--------------------------|----------------------------------------------------------------|------------------------------------------|
| Queen-adjacent kill      | A queen is placed at `(r, c)`                                  | Every cell in row `r`, column `c`, same region as `(r, c)`, or king-adjacent to `(r, c)` becomes dead |
| Single-cell region       | A region has only one live cell                            | That cell holds a queen (forced)         |
| Single-cell row / column | A row or column has only one live cell                     | That cell holds a queen (forced)         |
| Region-confined-to-line  | All live cells of a region share one row (or column)       | That row (column) consumes its queen inside the region → kill all cells in the row (column) outside the region |
| Line-confined-to-region  | All live cells of a row (column) lie inside one region     | That region consumes its queen inside the row (column) → kill all cells in the region outside the row (column) |

Apply all rules, re-apply whenever anything changed, stop when nothing changed in a full pass. That's the fixed-point.

LinkedIn's UI exposes only the first rule as the "Auto-place Xs" setting — toggling it on kills the immediate row/column/region/king-adjacent cells around each queen you place. The other four rules are what humans have to run in their heads; a solver runs them trivially.

See [`HEURISTICS.md`](HEURISTICS.html) for the same rules as full pseudocode plus three additional heuristics (adjacency-blocks-region, N-regions-in-N-rows, N-regions-in-N-columns).

## Solver

Because `N` is small (≤ 11), a straightforward row-by-row backtracking solver finishes instantly. The row constraint is absorbed into the recursion structure — we fill exactly one queen per row, in order:

```
function solve(regions):
    N ← number of rows in regions
    queens ← empty list
    results ← empty list
    backtrack(0, regions, queens, results)
    return results

function backtrack(row, regions, queens, results):
    if row = N:
        append copy of queens to results
        return

    for col from 0 to N − 1:
        // column: skip if some earlier queen already in this column
        if any (r', c') in queens has c' = col then continue

        // region: skip if some earlier queen already in this region
        if any (r', c') in queens has regions[r'][c'] = regions[row][col] then continue

        // adjacency: skip if king-adjacent to any earlier queen
        if any (r', c') in queens has max(|r' − row|, |c' − col|) < 2 then continue

        push (row, col) onto queens
        backtrack(row + 1, regions, queens, results)
        pop (row, col) from queens
```

A well-formed puzzle has exactly one solution, so `|solve(regions)| = 1`. Puzzle designers test this before shipping.

You can also throw it at a SAT or MILP solver: give each cell a boolean variable `x[r][c]`, encode rules 1/2/3 as "sum equals 1" constraints, and encode rule 4 as a pairwise `x[a] + x[b] ≤ 1` for each adjacent pair. Overkill for `N = 12`, but the formulation is completely standard.

## Relation to Star Battle

**LinkedIn Queens is a parameter-specialized version of the Star Battle puzzle.** Star Battle is the general puzzle; Queens is what you get when you hardcode one of its parameters.

### The parameter

Star Battle takes an integer `K` (stars per row/column/region):

```
function is_valid_star_battle(regions, stars, K):
    N ← number of rows in regions
    if |stars| ≠ N × K then return false

    // exactly K stars in every row
    for each row r:
        if count of stars with row = r ≠ K then return false

    // exactly K stars in every column
    for each col c:
        if count of stars with col = c ≠ K then return false

    // exactly K stars in every colored region
    for each region letter L:
        if count of stars (r, c) with regions[r][c] = L ≠ K then return false

    // same king-adjacency rule as Queens
    for each unordered pair of stars (s1, s2):
        if max(|s1.row − s2.row|, |s1.col − s2.col|) < 2 then return false

    return true
```

- **Star Battle** (the puzzle family): `K ∈ {1, 2, 3, …}`. Most published puzzles use `K = 2` on a `10 × 10` grid.
- **LinkedIn Queens**: `K = 1`, `N ∈ {7, …, 11}`, and stars are drawn as crown glyphs (`♛`) instead of five-pointed stars (`★`).

If you already have a Star Battle solver, you get a Queens solver for free by passing `K = 1`. Conversely, upgrading a Queens solver to Star Battle means replacing three "count = 1" checks with "count = K" — everything else is identical.

### What `K = 1` actually changes in practice

| | Star Battle (typical `K = 2`) | LinkedIn Queens (`K = 1`) |
|---|---|---|
| Stars per row/column/region | 2 | 1 |
| Total stars on board | `2N` | `N` |
| Board size | almost always `10 × 10` | varies daily, `7`–`11` |
| Adjacency interactions | many — pairs within a row matter | few — at most one queen per row, so row-adjacency never fires |
| Difficulty | harder | easier; basic set-intersection propagation kills most cells |
| Visual convention | black-and-white regions, `★` symbol | colored regions, `♛` symbol |

### Complexity

Star Battle (general `K`) is NP-complete. Queens (`K = 1`) is also NP-complete — it's still a constrained exact-cover — but at `N ≤ 12` any solver, even a brute-force one, runs in microseconds. The worst-case asymptotic complexity and the real-world runtime aren't in the same universe here.

## References

- [Shortcuts for the Queens LinkedIn Puzzle — Joshua Siktar (Medium)](https://joshuasiktar.medium.com/shortcuts-for-the-queens-linkedin-puzzle-6fa185f8f686) — solver heuristics and pattern analysis.
- [Play Queens game on LinkedIn — LinkedIn Help](https://www.linkedin.com/help/linkedin/answer/a6269510) — official rule statement.
- [Star Battle Rules and Info — The Art of Puzzles](https://www.gmpuzzles.com/blog/star-battle-rules-and-info/) — canonical rules for the `K`-parameter family.
- [Star Battle (also known as Queens) by Thomas Snyder — GM Puzzles](https://www.gmpuzzles.com/blog/2024/11/bonus-star-battle-aka-queens-by-thomas-snyder-9/) — explicit statement that LinkedIn Queens is 1-Star Battle.
