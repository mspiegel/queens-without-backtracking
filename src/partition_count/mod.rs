//! Count the number of tilings of an N×N square board by exactly N fixed
//! polyominoes.
//!
//! Uses a frontier-based (Simpath-style) counting DP that processes cells in
//! row-major order, maintaining `HashMap<State, Count>` for a single layer at
//! a time. Each polyomino is implicitly a placed connected piece identified by
//! a label in the frontier state.

pub mod count;
pub mod simpath;
pub mod state;

pub use simpath::{count_partitions, CountError, CountOptions, CountReport};

/// Independent reference enumerator used only for testing. Exposed publicly
/// so integration tests can cross-validate against it; not intended as a
/// stable API.
pub mod brute_force;

/// Independent set-partition enumerator used only for testing. Exhaustive and
/// slow; serves as ground truth for small N (practical up to N=3).
pub mod partition_oracle;
