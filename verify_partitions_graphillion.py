"""Independent verification of the partition-count sequence using Graphillion.

Counts the number of ways to partition an N x N grid into exactly N connected
node-induced regions (orthogonal adjacency). Regions are unlabeled; rotations
and reflections of the board are NOT quotiented out.

Usage:
    python verify_partitions_graphillion.py [--max-n N]
"""

from __future__ import annotations

import argparse
import time
from typing import List, Tuple

# Values for N=1..7 below were independently computed by this script via
# Graphillion and matched the Rust count-partitions output. N>=8 exceeded
# available RAM under Graphillion on a 32 GB machine; those entries are
# Rust-only reference values pending independent verification.
EXPECTED = {
    1: 1,
    2: 6,
    3: 258,
    4: 62741,
    5: 72137699,
    6: 356612826084,
    7: 7146137621219723,
    8: 556983247769141192388,
    9: 163738881245258156011991945,
    10: 177275143473620927081039509553940,
    11: 693522704165389775924553269084028511919,
}


def grid_edges(n: int) -> List[Tuple[int, int]]:
    """Edges of the N x N grid graph with row-major cell ids r*n + c."""
    edges: List[Tuple[int, int]] = []
    for r in range(n):
        for c in range(n):
            v = r * n + c
            if c + 1 < n:
                edges.append((v, v + 1))
            if r + 1 < n:
                edges.append((v, v + n))
    return edges


def count_partitions(n: int) -> int:
    """Count partitions of an N x N grid into exactly N connected regions."""
    if n <= 0:
        raise ValueError("n must be >= 1")
    if n == 1:
        return 1
    from graphillion import GraphSet

    GraphSet.set_universe(grid_edges(n))
    return GraphSet.partitions(num_comp_lb=n, num_comp_ub=n).len()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--max-n", type=int, default=5)
    parser.add_argument("--min-n", type=int, default=1)
    args = parser.parse_args()

    print(f"{'N':>3}  {'count':>48}  {'time_s':>10}  status")
    for n in range(args.min_n, args.max_n + 1):
        t0 = time.perf_counter()
        count = count_partitions(n)
        elapsed = time.perf_counter() - t0
        expected = EXPECTED.get(n)
        if expected is None:
            status = "(no reference)"
        elif count == expected:
            status = "PASS"
        else:
            status = f"FAIL (expected {expected})"
        print(f"{n:>3}  {count:>48}  {elapsed:>10.3f}  {status}")


if __name__ == "__main__":
    main()
