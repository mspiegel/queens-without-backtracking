"""Unit tests for verify_partitions_graphillion."""

import pytest

from verify_partitions_graphillion import count_partitions, grid_edges


class TestGridEdges:
    def test_n1_has_no_edges(self):
        assert grid_edges(1) == []

    def test_n2_edge_count(self):
        # 2 horizontal + 2 vertical
        assert len(grid_edges(2)) == 4

    def test_n3_edge_count(self):
        # 2 * N * (N-1)
        assert len(grid_edges(3)) == 12

    def test_n4_edge_count(self):
        assert len(grid_edges(4)) == 24

    def test_n2_edges_content(self):
        # cells: 0 1 / 2 3
        assert set(grid_edges(2)) == {(0, 1), (2, 3), (0, 2), (1, 3)}

    def test_edges_only_between_orthogonal_neighbors(self):
        for n in range(2, 6):
            for u, v in grid_edges(n):
                ru, cu = divmod(u, n)
                rv, cv = divmod(v, n)
                assert abs(ru - rv) + abs(cu - cv) == 1

    def test_no_duplicate_edges(self):
        for n in range(1, 6):
            edges = grid_edges(n)
            normalized = {tuple(sorted(e)) for e in edges}
            assert len(normalized) == len(edges)


class TestCountPartitions:
    def test_n1(self):
        assert count_partitions(1) == 1

    def test_n2(self):
        assert count_partitions(2) == 6

    def test_n3(self):
        assert count_partitions(3) == 258

    def test_n4(self):
        assert count_partitions(4) == 62741

    def test_invalid_n_raises(self):
        with pytest.raises(ValueError):
            count_partitions(0)
