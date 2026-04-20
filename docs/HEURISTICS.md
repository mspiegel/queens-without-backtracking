---
title: LinkedIn Queens — Solver Heuristics
---

# LinkedIn Queens — Solver Heuristics

This document collects [constraint-propagation](https://en.wikipedia.org/wiki/Constraint_propagation) rules that a solver (or a human) applies to speed up search. Each rule is a sound deduction. It marks cells **dead** (no queen can ever go there) or forces a cell to become a **queen**, given the current partial state. Running these rules to a [fixed point](https://en.wikipedia.org/wiki/Fixed_point_(mathematics)) (applying them repeatedly until nothing changes) usually solves a LinkedIn-size puzzle outright, with no backtracking.

For the data model, vocabulary (`regions`, `queens`, `dead`, `is_live`, live / dead / queen states), and rules of the game, see [`QUEENS.md`](QUEENS.html). The pseudocode here uses those directly.

Every rule below has the same shape:

```
function apply_<rule>(regions, queens, dead):
    changed ← false
    ...
    return changed           // true iff some cell transitioned to dead or was forced to a queen
```

A driver repeatedly invokes each rule until a full pass returns `changed = false`.

## Baseline rules

These five rules are the minimum a solver needs; they also appear summarized in `QUEENS.md`. LinkedIn's UI "Auto-place Xs" toggle runs only the first one.

### Queen-adjacent kill

Placing a queen at a cell immediately rules out every cell that would conflict with it by the game's four rules: the rest of its row, the rest of its column, the rest of its region, and its eight king-adjacent neighbors.

**Fires when**: a queen is placed at `(r, c)`.
**Effect**: every cell in row `r`, column `c`, same region as `(r, c)`, or king-adjacent to `(r, c)` becomes dead.

```
function apply_queen_adjacent_kill(regions, queens, dead):
    changed ← false
    for each queen (r, c) in queens:
        for each cell (r', c') in the grid:
            if (r', c') = (r, c) or dead[r'][c'] then continue
            same_row    ← (r' = r)
            same_col    ← (c' = c)
            same_region ← (regions[r'][c'] = regions[r][c])
            king_near   ← (max(|r' − r|, |c' − c|) < 2)
            if same_row or same_col or same_region or king_near:
                dead[r'][c'] ← true
                changed ← true
    return changed
```

Valid because the four solution rules in `QUEENS.md` (one queen per row / column / region; no king-adjacent queens) directly prohibit queens in any of those cells once `(r, c)` is a queen.

### Single-cell region

When a region's live cells narrow down to exactly one, that cell is forced to hold the region's queen.

**Fires when**: some region has exactly one live cell.
**Effect**: place a queen on that cell.

```
function apply_single_cell_region(regions, queens, dead):
    changed ← false
    for each distinct region letter L:
        live ← { (r, c) : is_live(r, c) and regions[r][c] = L }
        if |live| = 1:
            place queen at the single element of live
            changed ← true
    return changed
```

Valid because every region must contain exactly one queen; if only one placement remains, it is forced.

### Single-cell row / column

When a row's (or column's) live cells narrow down to exactly one, that cell is forced to hold the row's (or column's) queen.

**Fires when**: some row or column has exactly one live cell.
**Effect**: place a queen on that cell.

```
function apply_single_cell_line(regions, queens, dead):
    changed ← false
    for each row r:
        live ← { (r, c) : is_live(r, c) }
        if |live| = 1:
            place queen at the single element of live
            changed ← true
    for each column c:
        live ← { (r', c) : is_live(r', c) }
        if |live| = 1:
            place queen at the single element of live
            changed ← true
    return changed
```

Valid by the same argument as single-cell region, applied to rows and columns.

### Region-confined-to-line

If every live cell of a region lies in a single row or column, the region's queen must land in that row or column, so the rest of the row (or column) outside the region can be marked dead.

**Fires when**: all live cells of some region share a single row (or column).
**Effect**: every cell in that row (or column) outside the region becomes dead.

```
function apply_region_confined_to_line(regions, queens, dead):
    changed ← false
    for each distinct region letter L:
        live ← { (r, c) : is_live(r, c) and regions[r][c] = L }
        if live is empty then continue
        rows ← { r : (r, c) ∈ live }
        if |rows| = 1:
            r* ← the single element of rows
            for each column c with is_live(r*, c) and regions[r*][c] ≠ L:
                dead[r*][c] ← true
                changed ← true
        cols ← { c : (r, c) ∈ live }
        if |cols| = 1:
            c* ← the single element of cols
            for each row r with is_live(r, c*) and regions[r][c*] ≠ L:
                dead[r][c*] ← true
                changed ← true
    return changed
```

Valid because the region's queen must land in its only occupied row (or column), and each row (column) on the board already holds exactly one queen. So the queen at the intersection is the row's queen, and the rest of the row can be eliminated.

### Line-confined-to-region

If every live cell of a row or column lies inside a single region, that region's queen must land in the row (or column), so the region's other cells outside the line can be marked dead.

**Fires when**: all live cells of some row (or column) lie inside a single region.
**Effect**: every cell of that region outside the row (or column) becomes dead.

```
function apply_line_confined_to_region(regions, queens, dead):
    changed ← false
    for each row r:
        live ← { (r, c) : is_live(r, c) }
        if live is empty then continue
        letters ← { regions[r][c] : (r, c) ∈ live }
        if |letters| = 1:
            L ← the single element of letters
            for each cell (r', c') with regions[r'][c'] = L and r' ≠ r and is_live(r', c'):
                dead[r'][c'] ← true
                changed ← true
    for each column c:
        live ← { (r, c) : is_live(r, c) }
        if live is empty then continue
        letters ← { regions[r][c] : (r, c) ∈ live }
        if |letters| = 1:
            L ← the single element of letters
            for each cell (r', c') with regions[r'][c'] = L and c' ≠ c and is_live(r', c'):
                dead[r'][c'] ← true
                changed ← true
    return changed
```

Valid because the row's (column's) queen must be inside the region, and each region on the board already holds exactly one queen. So that queen is both the row's and the region's, and the rest of the region can be eliminated.

## Extended heuristics

These 30 rules look at larger patterns than individual cells. They fire less often than the baseline rules, but when they do, they tend to unlock puzzles that the baseline rules can't solve on their own. The first 27 recognise specific [free-polyomino](https://en.wikipedia.org/wiki/Polyomino) shapes of size 2 through 6 among the live cells of a region (a "free polyomino" treats rotations and reflections of the same shape as identical). The next two apply [pigeonhole](https://en.wikipedia.org/wiki/Pigeonhole_principle) reasoning across multiple regions. The last one, `placement-forces-contradiction`, simulates a single queen placement and checks whether a short burst of baseline propagation exposes a contradiction; it subsumes the other rules but runs several times slower than they do, so the solver driver invokes it only after the cheaper rules stop firing.

### Polyomino

The 27 polyomino shape rules all share the same underlying idea. The region's queen must land somewhere in the region's live cells. If a cell *outside* the region would be killed no matter which live cell the queen ends up on (either by sharing the queen's row, column, or [king-neighborhood](https://en.wikipedia.org/wiki/King_(chess))), then that cell can be marked dead right away, before any queen has actually been placed. Each rule below identifies which cells those are for one specific shape.

#### Dominoes

##### domino region

**Fires when**: the live cells of some region form a [free domino](https://en.wikipedia.org/wiki/Polyomino) (exactly two cells sharing an edge).
**Effect**: mark dead the four cells that flank the domino on its long sides (above and below a horizontal domino; left and right of a vertical domino).

```
horizontal domino            vertical domino

. . . . .                    . . . . .
. ✕ ✕ . .                    . ✕ D ✕ .
. D D . .                    . ✕ D ✕ .
. ✕ ✕ . .                    . . . . .
. . . . .
```

(`D` = domino cell, `✕` = newly-dead cell. Cells outside the board are simply skipped when the domino sits on an edge.)

Valid because the queen must sit in one of the two domino cells, and each flank is king-adjacent to both cells and therefore to the queen wherever it lands. By the no-king-adjacency rule, the flanks cannot also hold queens.

Note that the baseline `region-confined-to-line` already handles the domino's shared row (or column), killing every other cell in that line. The polyomino-domino rule adds the four flanks that sit *outside* that line, which the baseline rules leave live.

#### Trominoes

##### I-tromino region

**Fires when**: the live cells of some region form a [free I-tromino](https://en.wikipedia.org/wiki/Tromino) (three collinear cells sharing consecutive edges).
**Effect**: mark dead the two cells that flank the middle cell on its long sides (directly above and below the middle cell of a horizontal I-tromino; directly left and right of the middle cell of a vertical I-tromino).

```
. . . . .
. . ✕ . .
. I I I .
. . ✕ . .
. . . . .
```

(`I` = tromino cell, `✕` = newly-dead cell. Only the horizontal orientation is shown; the vertical orientation is the same free polyomino and marks the two cells left and right of the middle cell.)

Valid because the queen must sit in one of the three collinear cells, and each flank is king-adjacent to all three and therefore to the queen wherever it lands. Neither flank can itself hold a queen.

The baseline `region-confined-to-line` rule also fires on any I-tromino region (all three cells share a row or a column) and kills every other cell in that line. The polyomino-I-tromino rule adds the two flanks that sit *outside* that line, which the baseline rules leave live.

##### L-tromino region

**Fires when**: the live cells of some region form a [free L-tromino](https://en.wikipedia.org/wiki/Tromino) (three cells filling a 2×2 square with one corner missing).
**Effect**: mark dead three cells. The three are the missing corner of the 2×2 bounding box, plus the two cells that extend the *elbow* outward along the two axes opposite its arms. (The elbow is the L cell adjacent to both other L cells; its arms are the other two.)

```
. ✕ . .
✕ L L .
. L ✕ .
. . . .
```

(`L` = tromino cell, `✕` = newly-dead cell. Only one orientation is shown; the other three rotations are the same free polyomino and produce the analogous three dead cells: missing corner plus the two extensions opposite the elbow's arms.)

Valid by case analysis: for each of the three possible queen positions, every marked cell is in the queen's row, column, or king-neighborhood.

#### Tetrominoes

##### I-tetromino region

**Fires when**: the live cells of some region form a [free I-tetromino](https://en.wikipedia.org/wiki/Tetromino) (four collinear cells sharing consecutive edges).
**Effect**: no polyomino-specific deduction. The baseline `region-confined-to-line` rule already fires on any I-tetromino (all four cells share a row or a column) and kills every other cell in that line. Unlike the I-tromino, no cell outside the I-tetromino is king-adjacent to all four of its cells, so there are no extra flanks to mark.

##### T-tetromino region

**Fires when**: the live cells of some region form a [free T-tetromino](https://en.wikipedia.org/wiki/Tetromino) (three collinear cells with a fourth cell extending from the middle, forming a "T").
**Effect**: one cell becomes dead. That cell would complete the T into a plus sign (directly opposite the stem, across the middle of the bar).

```
. . . . .
. . ✕ . .
. T T T .
. . T . .
. . . . .
```

(`T` = tetromino cell, `✕` = newly-dead cell.)

Valid because the plus-completer is in the queen's column for the two central-axis queens (middle of bar, stem) or king-adjacent to the queen for the two bar-end queens. It cannot hold a queen either way.

##### L-tetromino region

**Fires when**: the live cells of some region form a [free L-tetromino](https://en.wikipedia.org/wiki/Tetromino). This is three collinear cells (the "long arm") with a fourth cell (the "short arm") attached perpendicular to one end (the "bend").
**Effect**: two cells become dead.

  1. The cell that completes the L's bend into a 2×2 square (the missing corner of the 2×2 block at the bend).
  2. The cell directly past the bend, continuing the long arm's line away from its free end.

```
. . . . .
. L . . .
. L ✕ . .
. L L . .
. ✕ . . .
```

(`L` = tetromino cell, `✕` = newly-dead cell.)

Valid by case analysis on which of the four L cells holds the queen:

- The 2×2 corner cell is king-adjacent to all four L cells, so it is killed by king-adjacency regardless of which L cell holds the queen.
- The extension cell lies in the same column (or row) as the three long-arm cells, so it is killed by column (row) exclusion whenever the queen is in the long arm; when the queen is in the short arm, the extension cell is king-adjacent to it diagonally across the bend.

##### S-tetromino region

**Fires when**: the live cells of some region form a [free S-tetromino](https://en.wikipedia.org/wiki/Tetromino) (two edge-adjacent pairs of cells offset diagonally, forming an "S" or "Z" shape).
**Effect**: mark dead two cells. These are the two cells that would complete the S into a 2×3 rectangle (the two missing corners of the S's bounding box).

```
. . . . .
. ✕ S S .
. S S ✕ .
. . . . .
```

(`S` = tetromino cell, `✕` = newly-dead cell.)

Valid by case analysis: for each of the four queen positions, both marked cells are in the queen's row, column, or king-neighborhood.

#### Pentominoes

##### I-pentomino region

**Fires when**: the live cells of some region form a [free I-pentomino](https://en.wikipedia.org/wiki/Pentomino) (five collinear cells sharing consecutive edges).
**Effect**: no polyomino-specific deduction. The baseline `region-confined-to-line` rule already fires on any I-pentomino (all five cells share a row or a column) and kills every other cell in that line. As with the I-tetromino, no cell outside the I-pentomino is king-adjacent to all five of its cells, so there are no extra flanks to mark.

##### L-pentomino region

**Fires when**: the live cells of some region form a [free L-pentomino](https://en.wikipedia.org/wiki/Pentomino). It has four collinear cells (the "long arm") with a fifth cell attached perpendicular to one end (the "bend").
**Effect**: one cell becomes dead. That cell continues the long arm's line past the bend (opposite the long arm's free end).

```
. . . . . . .
. ✕ # # # # .
. . # . . . .
. . . . . . .
```

(`#` = pentomino cell, `✕` = newly-dead cell.)

Valid because the marked cell shares the long arm's row with all four long-arm queen positions, and is king-adjacent to the short-arm queen position (diagonally across the bend). This is the pentomino analogue of the L-tetromino's extension-cell deduction, extended by one additional cell in the long arm. The 2×2 corner-completer at the bend is *not* always dead: it is too far from the long arm's free end to be king-adjacent to that queen.

##### F-pentomino region

**Fires when**: the live cells of some region form a [free F-pentomino](https://en.wikipedia.org/wiki/Pentomino).
**Effect**: one cell becomes dead. That cell completes the F's middle row into three collinear cells of the 3×3 bounding box (adjacent on the right to the F's central cell).

```
. . . . . .
. . F F . .
. F F ✕ . .
. . F . . .
. . . . . .
```

(`F` = pentomino cell, `✕` = newly-dead cell.)

Valid by case analysis: the marked cell is in the queen's row for the two left-arm queens, the queen's column for the top-right queen, or king-adjacent to the queen for the central and stem queens.

##### N-pentomino region

**Fires when**: the live cells of some region form a [free N-pentomino](https://en.wikipedia.org/wiki/Pentomino) (a five-cell chain with a single one-cell step, forming a zigzag).
**Effect**: mark dead two cells. These are the two cells of the 4×2 bounding box that are orthogonally adjacent to two N cells each (the missing bounding-box cells closest to the jog, one on each arm).

```
. . . . .
. . N . .
. ✕ N . .
. N N . .
. N ✕ . .
. . . . .
```

(`N` = pentomino cell, `✕` = newly-dead cell.)

Valid by case analysis: for each of the five queen positions, both marked cells are in the queen's row, column, or king-neighborhood.

The third missing bounding-box cell (the corner diagonally opposite the short arm's end) is *not* always dead. This is because the queen on the short arm's far end neither shares its row or column, nor is king-adjacent to that corner.

##### T-pentomino region

**Fires when**: the live cells of some region form a [free T-pentomino](https://en.wikipedia.org/wiki/Pentomino) (three collinear cells with a two-cell stem extending perpendicular from the middle).
**Effect**: one cell becomes dead. That cell extends the stem across the bar (directly opposite the stem, one cell past the middle of the bar).

```
. . . . .
. . ✕ . .
. T T T .
. . T . .
. . T . .
. . . . .
```

(`T` = pentomino cell, `✕` = newly-dead cell.)

Valid because the marked cell is in the queen's column for the three central-axis queens (middle of bar, both stem cells) or king-adjacent to the queen for the two bar-end queens.

##### U-pentomino region

**Fires when**: the live cells of some region form a [free U-pentomino](https://en.wikipedia.org/wiki/Pentomino) (a 2×3 rectangle with the middle cell of one long side removed, forming a "U").
**Effect**: one cell becomes dead. That cell is the mouth of the U (the missing cell of the 2×3 bounding box).

```
. . . . .
. U ✕ U .
. U U U .
. . . . .
```

(`U` = pentomino cell, `✕` = newly-dead cell.)

Valid because the mouth cell is king-adjacent to all five U cells, so it is king-adjacent to the queen regardless of which cell holds it.

##### V-pentomino region

**Fires when**: the live cells of some region form a [free V-pentomino](https://en.wikipedia.org/wiki/Pentomino) (two three-cell arms meeting at a shared corner, forming a right angle).
**Effect**: one cell becomes dead. That cell sits diagonally off the corner cell, on the concave (inside) side of the V.

```
. . . . .
. V . . .
. V ✕ . .
. V V V .
. . . . .
```

(`V` = pentomino cell, `✕` = newly-dead cell.)

Valid because the inside-corner cell is king-adjacent to all five V cells (it sits diagonally or orthogonally off every one of them).

##### W-pentomino region

**Fires when**: the live cells of some region form a [free W-pentomino](https://en.wikipedia.org/wiki/Pentomino) (a three-step diagonal staircase).
**Effect**: one cell becomes dead. That cell completes the lower step of the staircase into a 2×2 square (the inside corner on the concave side of the staircase's lower bend).

```
. . . . .
. W . . .
. W W . .
. ✕ W W .
. . . . .
```

(`W` = pentomino cell, `✕` = newly-dead cell.)

Valid because the marked cell shares a column with the two W cells above it, shares a row with the two W cells to its right, and is king-adjacent to the W's centre cell. Every queen position kills it.

#### Hexominoes

There are 35 free hexominoes in total, but most of them cannot mark any cells dead. For most shapes, no cell outside the shape is guaranteed to be in every queen position's row, column, or king-neighborhood, which is why only twelve rules exist out of the 35 possible hexomino shapes. The I-hexomino is handled by `region-confined-to-line`, and the eleven shapes below each mark one or two cells via a polyomino rule. The shape names ("Z-with-tab", "F-ext-A", and so on) are informal labels used only in this document. Each shape is fully identified by the diagram at the top of its rule. Only one canonical orientation of each free hexomino is shown.

##### I-hexomino region

**Fires when**: the live cells of some region form a [free I-hexomino](https://en.wikipedia.org/wiki/Hexomino) (six collinear cells sharing consecutive edges).
**Effect**: no polyomino-specific deduction. The baseline `region-confined-to-line` rule fires (all six cells share a row or column) and kills every other cell in that line. No cell outside the I-hexomino is king-adjacent to all six of its cells.

##### Z-with-tab hexomino region

**Fires when**: the live cells of some region form this free hexomino (bounding box 2×4). It has a top row of three cells with an offset bottom row of three cells sharing the leftmost column and extending one cell past the right end.
**Effect**: one cell becomes dead. That cell is the middle of the bottom row, filling the shape's internal notch.

```
. . . . . .
. # # # . .
. # ✕ # # .
. . . . . .
```

Valid by case analysis: the marked cell shares row 1 with three queen positions, column 1 with two queen positions, and is king-adjacent to the remaining queen position.

##### L-hexomino region

**Fires when**: the live cells of some region form a free L-hexomino. This is five collinear cells (the "long arm") with a sixth cell attached perpendicular to one end (the "bend").
**Effect**: one cell becomes dead. That cell continues the long arm's line past the bend (opposite the long arm's free end).

```
. . . . . . . .
. ✕ # # # # # .
. . # . . . . .
. . . . . . . .
```

Valid because the marked cell shares the long arm's row with all five long-arm queen positions, and is king-adjacent to the short-arm queen position (diagonally across the bend). Unlike the L-tetromino, the 2×2 corner-completer is *not* always dead. It is too far from the long arm's free end to be king-adjacent to that queen.

##### N-hexomino region

**Fires when**: the live cells of some region form a free N-hexomino (bounding box 2×5). It has a four-cell horizontal arm with a two-cell arm dropping down from its right end.
**Effect**: one cell becomes dead. That cell is the 2×2 corner completer at the bend.

```
. . . . . . .
. # # # # ✕ .
. . . . # # .
. . . . . . .
```

Valid because the marked cell is king-adjacent to all four cells of the 2×2 block at the bend (which contains three hexomino cells), shares row 0 with the four long-arm queen positions, and is king-adjacent to the bend's lower cell.

##### long-N hexomino region

**Fires when**: the live cells of some region form a free long-N hexomino (bounding box 2×5). It has two three-cell horizontal arms offset by one row and two columns (a 3+3 zigzag).
**Effect**: two cells become dead. Each is the 2×2 corner completer at one of the zigzag's two bends.

```
. . . . . . .
. # # # ✕ . .
. . ✕ # # # .
. . . . . . .
```

Valid by case analysis: each marked cell is king-adjacent to three hexomino cells in the 2×2 block at its bend and shares a row or column with the three cells on the far arm.

##### C-hexomino region

**Fires when**: the live cells of some region form a free C-hexomino (bounding box 3×3). It is a 3×3 block missing the centre cell and two cells along one edge.
**Effect**: one cell becomes dead. That cell is the centre of the 3×3 bounding box.

```
. . . . .
. # # # .
. # ✕ # .
. # . . .
. . . . .
```

Valid because the centre cell is king-adjacent to all six hexomino cells (every hexomino cell lies within distance 1 of the centre), so it is king-adjacent to the queen regardless of which hexomino cell holds it.

##### S-block hexomino region

**Fires when**: the live cells of some region form a free S-block hexomino (bounding box 3×3). It is three two-cell horizontal pairs stacked with alternating horizontal offsets.
**Effect**: one cell becomes dead. That cell is the notch on the middle row's open side.

```
. . . . .
. # # . .
. ✕ # # .
. # # . .
. . . . .
```

Valid by case analysis: the marked cell shares column 0 with two queen positions, shares row 1 with two queen positions, and is king-adjacent to the remaining two queen positions.

##### F-ext-A hexomino region

**Fires when**: the live cells of some region form a free F-ext-A hexomino (bounding box 3×4). It is an extension of the F-pentomino with one additional cell forming a right-angle tail.
**Effect**: one cell becomes dead. That cell sits below the bar's middle, inside the elbow between the bar and the stem.

```
. . . . . .
. # # # . .
. . ✕ # # .
. . . # . .
. . . . . .
```

Valid by case analysis: the marked cell is king-adjacent to five of the six hexomino cells (all except the rightmost cell of the middle row) and shares row 1 with that remaining cell.

##### N-ext-A hexomino region

**Fires when**: the live cells of some region form a free N-ext-A hexomino (bounding box 3×4). It is a three-step diagonal staircase.
**Effect**: one cell becomes dead. That cell is the 2×2 corner completer at the staircase's upper bend.

```
. . . . . .
. # # # ✕ .
. . . # # .
. . . . # .
. . . . . .
```

Valid because the marked cell is king-adjacent to four hexomino cells (the upper bend's 2×2 block plus the far-right cell of the top row) and shares column 3 with the remaining two staircase cells.

##### T-ext hexomino region

**Fires when**: the live cells of some region form a free T-ext hexomino (bounding box 3×4). It is a plus-sign with one arm truncated and an extra cell added to the opposite end of the horizontal bar.
**Effect**: one cell becomes dead. That cell sits left of the central cross.

```
. . . . . .
. # # . . .
. ✕ # # # .
. . # . . .
. . . . . .
```

Valid by case analysis: the marked cell shares row 1 with three queen positions, shares column 0 with one queen position (the top-left cell), and is king-adjacent to the remaining two queen positions.

##### W-ext hexomino region

**Fires when**: the live cells of some region form a free W-ext hexomino (bounding box 3×4). It is a four-cell staircase with an extra cell at one end.
**Effect**: one cell becomes dead. That cell is the 2×2 corner completer at the upper bend.

```
. . . . . .
. # # ✕ . .
. . # # # .
. . . # . .
. . . . . .
```

Valid by case analysis: the marked cell is king-adjacent to four hexomino cells (the 2×2 block at the upper bend plus the bend below) and shares column 2 with the two cells in column 2.

##### T-hexomino region

**Fires when**: the live cells of some region form a free T-hexomino. It is three collinear cells forming a bar with a three-cell stem extending perpendicular from the middle.
**Effect**: one cell becomes dead. That cell extends the stem across the bar (directly opposite the stem, one cell past the middle of the bar).

```
. . . . .
. . ✕ . .
. # # # .
. . # . .
. . # . .
. . # . .
. . . . .
```

Valid because the marked cell is in the queen's column for the four central-axis queens (middle of bar, all three stem cells) or king-adjacent to the queen for the two bar-end queens.

### Pigeonhole rules

The last two extended rules work differently from the polyomino rules above. Instead of looking at one region's shape, they compare multiple regions to each other and apply [pigeonhole](https://en.wikipedia.org/wiki/Pigeonhole_principle) reasoning. The pseudocode below describes the straightforward (but expensive) version of each rule. The "Cache-friendly re-evaluation" subsection near the end of this document describes the efficient implementation that should actually be used in a solver.

#### N-regions-in-N-rows

**Fires when**: there exist `N` distinct regions whose live cells all lie within some set of `N` rows (not necessarily contiguous).
**Effect**: every live cell of any *other* region within those `N` rows becomes dead.

```
function apply_N_regions_in_N_rows(regions, queens, dead):
    changed ← false
    all_letters ← { distinct region letters }
    for N from 1 to |all_letters| − 1:
        for each subset S of all_letters with |S| = N:
            rows_used ← { r : exists c with is_live(r, c) and regions[r][c] ∈ S }
            if |rows_used| = N:
                for each live cell (r, c) with r ∈ rows_used and regions[r][c] ∉ S:
                    dead[r][c] ← true
                    changed ← true
    return changed
```

Valid by pigeonhole: the `N` regions in `S` supply exactly `N` queens (one each), all of which must land inside `rows_used`. Those rows contain exactly `N` queens in total (one per row). So the queens of `S` are exactly the queens of `rows_used`, leaving no slot for any other region's queen in those rows.

**Note on the `N = 1` case**: it collapses to `region-confined-to-line` (row variant). A single region whose live cells lie in a single row is exactly the `N = 1` instance of this rule. So `N-regions-in-N-rows` is a strict generalization.

#### N-regions-in-N-columns

**Fires when**: there exist `N` distinct regions whose live cells all lie within some set of `N` columns.
**Effect**: every live cell of any other region within those `N` columns becomes dead.

```
function apply_N_regions_in_N_columns(regions, queens, dead):
    changed ← false
    all_letters ← { distinct region letters }
    for N from 1 to |all_letters| − 1:
        for each subset S of all_letters with |S| = N:
            cols_used ← { c : exists r with is_live(r, c) and regions[r][c] ∈ S }
            if |cols_used| = N:
                for each live cell (r, c) with c ∈ cols_used and regions[r][c] ∉ S:
                    dead[r][c] ← true
                    changed ← true
    return changed
```

Valid by the column-wise pigeonhole argument, mirror-image of the previous rule.

**Note on the `N = 1` case**: it collapses to `region-confined-to-line` (column variant).

### Placement-forces-contradiction

**Fires when**: placing a queen at a live cell `(r, c)` would, after a short burst of baseline-rule propagation, leave either some region with zero live cells or some row or column with two queens.
**Effect**: mark `(r, c)` dead.

```
function apply_placement_forces_contradiction(regions, queens, dead):
    changed ← false
    if not h2_dirty then return false
    h2_dirty ← false
    K ← 6
    k ← 2
    candidates ← { (r, c) : is_live(r, c)
                          and kill_list[(r, c)] intersects live_cells(Y)
                          for some region Y with |live(Y)| ≤ K }
    for each (r, c) in candidates:
        save ← snapshot(state)
        simulate: mark (r, c) as a queen and apply kill_list[(r, c)]
        for i from 1 to k:
            if no baseline rule fires in this step, break
            apply single-cell-region, single-cell-line,
                  region-confined-to-line, line-confined-to-region
        if some region has 0 live cells, or some row or column has ≥ 2 queens:
            restore(state, save)
            dead[r][c] ← true
            changed ← true
        else:
            restore(state, save)
    return changed
```

Valid because if `(r, c)` held a queen, the propagated state would violate one of the game's four rules (Rule 1 or 2 on one queen per row or column, or Rule 3 on one queen per region). So no valid completion of the puzzle can place a queen at `(r, c)`.

This rule generalises the direct "one cell's kill list swallows another region" pattern to catch chains where the contradiction only surfaces after one round of `single-cell-region` / `single-cell-line` propagation. The motivating case is a four-region chain where placing a queen kills one cell of each of several 2-cell regions, forcing each of those regions to its remaining cell, and two of those forced cells land in the same row or column.

**Two tunable parameters**:

- **K, the small-region gate.** A candidate `(r, c)` can only produce a new contradiction if its kill list intersects some region whose live-cell count is already low. Default `K = 6`, the same bound the polyomino rules and the dirty-and-small region set use. Cells whose kill list reaches no region smaller than this bound cannot cause `placement-forces-contradiction` to fire beyond what the cheaper rules already catch, so they are skipped.
- **k, the propagation depth cap.** Cap the simulation at `k` baseline passes after the kill list is applied. Default `k = 2`. `k = 1` collapses to a direct "kill list swallows another region" superset check. `k ≥ 3` starts approximating backtracking and is explicitly out of scope for a heuristics-only solver.

**Supporting data structures**:

- **Live-mask bitmask per region.** A `B² ≤ 121`-bit value tracking which cells of each region are still live. For `B ≤ 11` this fits in two `u64` words, so "does this kill list swallow this region" becomes `(live_mask[Y] AND NOT kill_mask[X]) == 0`, one AND plus one compare.
- **Kill-mask bitmask per cell.** Same layout as the live-mask, one per cell, precomputed once at game start from the per-cell kill list described in "Data structures and precomputation".
- **Undo log.** A list of `(cell, previous_state)` pairs recorded during simulation. Rollback iterates the list in reverse. This avoids copying the full state per candidate.
- **`h2_dirty` flag.** Set whenever any region's live count decreases; cleared at the end of an `apply_placement_forces_contradiction` call. Lets clean passes skip the rule in `O(1)`.

**Cost**. With the gates and bitmasks in place, `O(K) = O(1)` candidates sit in each small region, so the candidate set has size `O(B)`; each candidate applies its kill list, runs at most `k = 2` baseline passes (each `O(B)` with precomputed structures), checks for contradiction in `O(B)`, and rolls back in `O(B)`. The per-pass cost is therefore `O(B²)` on a dirty pass and `O(1)` on a clean pass. The naive cost without gates or bitmasks is `O(B⁴)` per pass.

**Apply this rule only after the cheaper rules have reached a fixed point.** Even at `O(B²)`, this rule is several times more expensive per pass than the baseline rules, the polyomino shape rules, and the pigeonhole rules, because each candidate runs a miniature simulation with rollback instead of reading a counter. The solver driver should drive every other rule to a fixed point first and invoke `placement-forces-contradiction` only when nothing cheaper fires. If this rule marks any cell dead, loop back to the cheaper rules and repeat the cycle.

## Applying the rules

Run all rules to a fixed point. Keep invoking them in some order, and stop when a full pass leaves `changed = false` across every rule. At that point the partial state is as tight as these rules can make it. The result is either a complete solution or a stuck state that needs branching.

### Data structures and precomputation

The `regions` grid is fixed for the duration of a solve. Only `dead` and `queens` change. Building a few lookup tables up front and maintaining a few counters incrementally cuts most per-pass costs from [`O(B²)`](https://en.wikipedia.org/wiki/Big_O_notation) down to `O(B)` or `O(1)`. The intuition is that when a cell dies, we do not re-scan the board. Instead we decrement one counter and update one set, and leave every other rule's state in place until something relevant to it actually changes.

**Static precomputations** (built once at game start from `regions` alone):

- **Per-cell kill list**: for each cell `(r, c)`, the set of cells that would die if a queen landed there. This is every cell in row `r`, column `c`, the region containing `(r, c)`, and the king-neighborhood of `(r, c)`. Queen-adjacent kill then reduces to iterating this precomputed list rather than rescanning the board.

- **Canonical-polyomino to dead-offsets table**: hash every free polyomino of size 2 through 6 by a fingerprint that is the same across all its rotations and reflections (this fingerprint is called a [canonical form](https://en.wikipedia.org/wiki/Canonical_form)), then map each fingerprint to the list of dead-cell offsets its rule marks. One table is shared across all 27 shape rules. A region's polyomino check becomes one canonicalization followed by one hash lookup, instead of 27 separate scans.

**Incrementally maintained** (updated on each cell death or queen placement):

- **Live-cell counts** per region, row, and column. Single-cell rules read these directly.
- **Live-cell sets** per region, row, and column. These support finding the surviving cell in `O(1)` when a count hits one, and iterating a region's live cells when canonicalizing.
- **Distinct-rows and distinct-cols per region**, maintained with per-(region, row) and per-(region, column) reference counters. Region-confined-to-line fires when either set has size 1.
- **Distinct-regions per row and per column**, maintained the same way. Line-confined-to-region fires when either set has size 1.
- **Per-region dirty flag**, set when any cell of the region dies. Polyomino shape rules re-canonicalize only dirty regions; regions untouched since their last check are skipped.
- **Dirty-and-small region set**: the subset of regions that are both dirty *and* have a live count in [2, 6]. These are the only regions where a polyomino shape rule can fire. Membership is updated in `O(1)` on each cell death (check the dirty flag and the now-current live count for the affected region, then add or remove accordingly). A polyomino pass iterates just this set instead of walking all `B` regions, and each canonicalization runs on at most 6 live cells, so it is `O(1)` per region.

Update cost per cell death is `O(1)` counter and set updates. A queen placement iterates its precomputed kill list (`O(B + |region|)` cells) and applies the same `O(1)` update per killed cell. The total is `O(B + |region|)` per queen.

### Cache-friendly re-evaluation for the N-regions rules

The `N-regions-in-N-rows` and `N-regions-in-N-columns` rules have a special structure that lets us amortize their cost across the whole solve. Think of a [bipartite graph](https://en.wikipedia.org/wiki/Bipartite_graph) where each region is one kind of node, each row is the other kind, and an edge connects a region to a row when that region has at least one live cell in that row. This graph is fully determined by the static `regions` grid at game start. The only dynamic event during a solve is *edge deletion*. A row drops out of a region's signature when the region's last live cell in that row dies.

**Rare-event observation**: across a whole solve, at most `B²` edges can ever be deleted. There are `B²` region-row pairs, and each pair's live-cell counter (already maintained incrementally, as described above) drops from 1 to 0 at most once. So the rule's precondition changes at most `O(B²)` times *total*, not per pass.

**What we maintain**:

- **Precomputed bipartite incidence graph** `(region, row)`, built once at game start from the `regions` grid. A parallel `(region, column)` graph covers the column-variant rule.
- **Per-`(region, row)` live-cell count**, already maintained for the polyomino rules.
- **N-regions-rows dirty flag**, flipped whenever any `(region, row)` counter transitions from 1 to 0. Cleared whenever the rule runs to completion. A separate dirty flag tracks `(region, column)` for the column variant.

**Per-pass behavior**:

- If the dirty flag is clear, the rule is known not to fire. Skip entirely in `O(1)`.
- If dirty, run one [Dulmage–Mendelsohn decomposition](https://en.wikipedia.org/wiki/Dulmage%E2%80%93Mendelsohn_decomposition) of the current bipartite graph in `O(V + E) = O(B²)`. This is a standard polynomial-time graph algorithm that finds every subset of regions locked into the same number of rows (a subset like this is called a [Hall-tight subset](https://en.wikipedia.org/wiki/Hall%27s_marriage_theorem) in graph theory). Each Hall-tight subset is exactly one N-regions-in-N-rows match, so one decomposition finds them all without iterating the `2^B` region subsets.

This caches the rule's result: most passes cost `O(1)` because the dirty flag is clean, and dirty passes cost `O(B²)`. Across a full solve, total work is bounded by `O(B²)` dirty events × `O(B²)` per D–M pass = `O(B⁴)` worst-case, but in practice far less since edge deletions cluster around queen placements.

A one-shot sweep at game start catches every tight subset that exists in the initial bipartite graph; those fire immediately and never need re-checking.

### Cost ordering

Rough per-pass cost, with `B` = board size. The "Naive cost" column corresponds to the straightforward pseudocode shown in each rule's section. The "With precomputation" column assumes the data structures from the previous two subsections are in place.

| Rule                                  | Naive cost        | With precomputation         |
|---------------------------------------|-------------------|-----------------------------|
| Queen-adjacent kill                   | `O(B²)` per queen | `O(B + |region|)` per queen |
| Single-cell region                    | `O(B²)`           | `O(B)`                      |
| Single-cell row / column              | `O(B²)`           | `O(B)`                      |
| Region-confined-to-line               | `O(B²)`           | `O(B)`                      |
| Line-confined-to-region               | `O(B²)`           | `O(B)`                      |
| All polyomino shape rules (27 total)  | `O(27 · B²)`      | `O(B)`                      |
| N-regions-in-N-rows                   | `O(2^B · B²)`     | `O(B²)` dirty / `O(1)` clean |
| N-regions-in-N-columns                | `O(2^B · B²)`     | `O(B²)` dirty / `O(1)` clean |
| Placement-forces-contradiction        | `O(B⁴)`           | `O(B²)` dirty / `O(1)` clean |

Strategy: apply the rules in two tiers. The cheap tier is every rule above `placement-forces-contradiction`: the baseline rules, the polyomino shape rules, and the pigeonhole N-regions rules. With the precomputed structures in place each cheap-tier rule costs `O(B)` per pass (except a dirty N-regions pass at `O(B²)`), so the driver can run them all each pass and converge to a cheap-tier fixed point quickly. Only when that fixed point is reached and the puzzle is still unsolved should the driver invoke `placement-forces-contradiction`. If the `placement-forces-contradiction` pass marks any cell dead, loop back to the cheap tier and repeat. Stop when a full cycle of both tiers leaves everything unchanged. For LinkedIn boards (`B ≤ 11`) a full cheap-tier pass takes well under a millisecond, and a `placement-forces-contradiction` pass is several times that but still bounded by `O(B²)` bit-ops. In practice a meaningful share of shipped puzzles stall under the cheap tier and only finish once `placement-forces-contradiction` runs; see the usage chart in the project overview for the actual mix.
