//! Heuristic application counters.
//!
//! Each counter tracks how many times its heuristic fired during the solve.
//! A heuristic "fires" once per pass iteration in which it marked at least
//! one new cell dead or placed a queen. Counters are **not** scaled by the
//! number of cells affected.
//!
//! See `PLAN.md` for the canonical list of counter names and their grouping.

use std::collections::BTreeMap;

use crate::polyomino::POLYOMINO_SHAPE_RULES;

/// Enum of every counted rule. Using an enum key avoids typos and lets
/// Rust's exhaustiveness checker guarantee every rule is accounted for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RuleId {
    QueenAdjacentKill,
    SingleCellRegion,
    SingleCellLine,
    RegionConfinedToLine,
    LineConfinedToRegion,
    /// One variant per polyomino shape rule. The payload is the index into
    /// `polyomino::POLYOMINO_SHAPE_RULES`.
    Polyomino(usize),
    NRegionsInNLines,
    PlacementForcesContradiction,
}

/// Heuristic application counters.
#[derive(Debug, Clone)]
pub struct Counters {
    queen_adjacent_kill: u64,
    single_cell_region: u64,
    single_cell_line: u64,
    region_confined_to_line: u64,
    line_confined_to_region: u64,
    /// Indexed by polyomino shape rule index in `POLYOMINO_SHAPE_RULES`.
    polyomino: Vec<u64>,
    n_regions_in_n_lines: u64,
    placement_forces_contradiction: u64,
}

impl Counters {
    pub fn new() -> Self {
        Counters {
            queen_adjacent_kill: 0,
            single_cell_region: 0,
            single_cell_line: 0,
            region_confined_to_line: 0,
            line_confined_to_region: 0,
            polyomino: vec![0; POLYOMINO_SHAPE_RULES.len()],
            n_regions_in_n_lines: 0,
            placement_forces_contradiction: 0,
        }
    }

    pub fn increment(&mut self, rule: RuleId) {
        match rule {
            RuleId::QueenAdjacentKill => self.queen_adjacent_kill += 1,
            RuleId::SingleCellRegion => self.single_cell_region += 1,
            RuleId::SingleCellLine => self.single_cell_line += 1,
            RuleId::RegionConfinedToLine => self.region_confined_to_line += 1,
            RuleId::LineConfinedToRegion => self.line_confined_to_region += 1,
            RuleId::Polyomino(idx) => self.polyomino[idx] += 1,
            RuleId::NRegionsInNLines => self.n_regions_in_n_lines += 1,
            RuleId::PlacementForcesContradiction => self.placement_forces_contradiction += 1,
        }
    }

    pub fn get(&self, rule: RuleId) -> u64 {
        match rule {
            RuleId::QueenAdjacentKill => self.queen_adjacent_kill,
            RuleId::SingleCellRegion => self.single_cell_region,
            RuleId::SingleCellLine => self.single_cell_line,
            RuleId::RegionConfinedToLine => self.region_confined_to_line,
            RuleId::LineConfinedToRegion => self.line_confined_to_region,
            RuleId::Polyomino(idx) => self.polyomino[idx],
            RuleId::NRegionsInNLines => self.n_regions_in_n_lines,
            RuleId::PlacementForcesContradiction => self.placement_forces_contradiction,
        }
    }

    /// Return every counter paired with its display name. Ordering:
    /// baseline rules in the order presented in HEURISTICS.md, followed by
    /// every polyomino shape rule in table order, followed by the combined
    /// N-regions counter.
    pub fn named_values(&self) -> Vec<(&'static str, u64)> {
        let mut out: Vec<(&'static str, u64)> = Vec::new();
        out.push(("Queen-adjacent kill", self.queen_adjacent_kill));
        out.push(("Single-cell region", self.single_cell_region));
        out.push(("Single-cell row / column", self.single_cell_line));
        out.push(("Region-confined-to-line", self.region_confined_to_line));
        out.push(("Line-confined-to-region", self.line_confined_to_region));
        for (i, shape) in POLYOMINO_SHAPE_RULES.iter().enumerate() {
            out.push((shape.name, self.polyomino[i]));
        }
        out.push(("N-regions-in-N-lines", self.n_regions_in_n_lines));
        out.push((
            "Placement-forces-contradiction",
            self.placement_forces_contradiction,
        ));
        out
    }

    /// Convenience: total count of all rule applications.
    pub fn total(&self) -> u64 {
        let mut sum = self.queen_adjacent_kill
            + self.single_cell_region
            + self.single_cell_line
            + self.region_confined_to_line
            + self.line_confined_to_region
            + self.n_regions_in_n_lines
            + self.placement_forces_contradiction;
        sum += self.polyomino.iter().sum::<u64>();
        sum
    }

    /// Return each counter grouped by name into a map (for test assertions).
    pub fn as_map(&self) -> BTreeMap<&'static str, u64> {
        self.named_values().into_iter().collect()
    }
}

impl Default for Counters {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_are_zero_by_default() {
        let c = Counters::new();
        for rule in [
            RuleId::QueenAdjacentKill,
            RuleId::SingleCellRegion,
            RuleId::SingleCellLine,
            RuleId::RegionConfinedToLine,
            RuleId::LineConfinedToRegion,
            RuleId::NRegionsInNLines,
            RuleId::PlacementForcesContradiction,
        ] {
            assert_eq!(c.get(rule), 0);
        }
        for i in 0..POLYOMINO_SHAPE_RULES.len() {
            assert_eq!(c.get(RuleId::Polyomino(i)), 0);
        }
        assert_eq!(c.total(), 0);
    }

    #[test]
    fn increment_is_independent_per_rule() {
        let mut c = Counters::new();
        c.increment(RuleId::QueenAdjacentKill);
        c.increment(RuleId::QueenAdjacentKill);
        c.increment(RuleId::SingleCellRegion);
        c.increment(RuleId::NRegionsInNLines);
        c.increment(RuleId::Polyomino(0));
        assert_eq!(c.get(RuleId::QueenAdjacentKill), 2);
        assert_eq!(c.get(RuleId::SingleCellRegion), 1);
        assert_eq!(c.get(RuleId::SingleCellLine), 0);
        assert_eq!(c.get(RuleId::NRegionsInNLines), 1);
        assert_eq!(c.get(RuleId::Polyomino(0)), 1);
        assert_eq!(c.get(RuleId::Polyomino(1)), 0);
        assert_eq!(c.total(), 5);
    }

    #[test]
    fn named_values_count_matches_expectation() {
        let c = Counters::new();
        let named = c.named_values();
        // 5 baseline + 24 polyomino + 1 combined N-regions + 1 H2 = 31.
        assert_eq!(named.len(), 5 + POLYOMINO_SHAPE_RULES.len() + 1 + 1);
        assert_eq!(named.len(), 31);
    }

    #[test]
    fn named_values_start_with_baseline_and_end_with_h2() {
        let c = Counters::new();
        let named = c.named_values();
        assert_eq!(named[0].0, "Queen-adjacent kill");
        assert_eq!(named[4].0, "Line-confined-to-region");
        assert_eq!(named.last().unwrap().0, "Placement-forces-contradiction");
    }

    #[test]
    fn placement_forces_contradiction_counter_increments_independently() {
        let mut c = Counters::new();
        c.increment(RuleId::PlacementForcesContradiction);
        c.increment(RuleId::PlacementForcesContradiction);
        assert_eq!(c.get(RuleId::PlacementForcesContradiction), 2);
        // Every other counter stays at zero.
        assert_eq!(c.get(RuleId::QueenAdjacentKill), 0);
        assert_eq!(c.get(RuleId::NRegionsInNLines), 0);
    }

    #[test]
    fn as_map_includes_every_polyomino_shape_by_name() {
        let c = Counters::new();
        let map = c.as_map();
        for shape in POLYOMINO_SHAPE_RULES {
            assert!(map.contains_key(shape.name), "missing shape {}", shape.name);
        }
    }
}
