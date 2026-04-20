# Queens Without Backtracking

A heuristics-only solver for [LinkedIn Queens](https://www.linkedin.com/games/queens) puzzles, written in Rust.

**Project site: <https://mspiegel.github.io/queens-without-backtracking/>**

## Hypothesis

After solving lots of LinkedIn Queens puzzles by hand, I began to suspect that these boards did not need a backtracking algorithm to solve them. I was solving them with heuristics, resorting at most to a short contradiction check with a small depth cutoff. **Hypothesis: heuristics alone can solve these boards.**

## The toolkit

The solver runs three tiers of constraint-propagation rules to a fixed point — no tree search.

- **Baseline (5 rules):** queen-adjacent kill, single-cell region, single-cell row/column, region-confined-to-line, line-confined-to-region.
- **Polyomino shape rules (27 rules):** when a region's live cells form a recognizable free polyomino of size 2–6, specific outside cells can be killed immediately.
- **Pigeonhole and bounded contradiction:** `N-regions-in-N-rows` / `-in-N-columns`, plus `placement-forces-contradiction` simulated to a hard depth cap of 2.

Full pseudocode and validity arguments are in [`HEURISTICS.md`](HEURISTICS.md); the puzzle rules are in [`QUEENS.md`](QUEENS.md).

## Result

Against an archive of 667 scraped historical LinkedIn Queens boards (7×7 through 11×11, mode 8×8), the solver correctly solves every one with heuristics only. Open followup questions — selection bias, the role of unique solutions, and whether polyominoes of some bounded size provably suffice for some board size — are discussed on the [project site](https://mspiegel.github.io/queens-without-backtracking/).

## Running the solver

```
cargo build --release
cat archive/boards/<id>.txt | ./target/release/queens-solver
```

Board format: whitespace-separated letter grid, one letter per cell, same letter means same region. See `QUEENS.md` for an example.
