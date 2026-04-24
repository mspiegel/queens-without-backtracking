//! Frontier DP that counts tilings of an N×N board by exactly N fixed
//! polyominoes.
//!
//! Cells are placed in row-major order. A `FxHashMap<State, Count>` holds the
//! current layer; after each cell is placed the old layer is replaced by the
//! newly-built one. At any point during the scan, the state describes the
//! labeling of the last `min(k, N)` placed cells — a superset of the true
//! frontier. Labels are kept in restricted-growth canonical form so that two
//! states are equal iff their connectivity patterns are equal.
//!
//! For each cell k, the up-neighbor (index k − N, always at slot 0 when it
//! exists) and the left-neighbor (always at the last occupied slot when
//! c > 0) contribute labels that drive the choice of transition:
//!
//!   A. Start a new polyomino (always permitted; reachability prune decides).
//!   B. Join the up-neighbor's polyomino.
//!   C. Join the left-neighbor's polyomino (only if distinct from up's).
//!   D. Merge the up and left polyominoes (only if both exist and differ).
//!
//! Option A is deliberately ungated: a cell may start a fresh polyomino even
//! when the total has already reached N, because a later cell can merge two
//! labels back together. The reachability prune `|total − N| ≤ cells_remaining`
//! drops any state that can no longer hit exactly N by the final cell.
//!
//! After placement the window slides when `k ≥ N`: slot 0 drops and may
//! close a polyomino (its label is incremented in `closed`). `closed` is
//! monotone non-decreasing, so `closed > N` is a hard prune.
//!
//! Layer advance is parallelized with Rayon: each thread emits transitions
//! into a thread-local `FxHashMap`, which is then merged at the end of the
//! layer. Merging is the serialization point.
//!
//! ## Mirror-symmetry collapse
//!
//! At row boundaries (just before placing cell `r*N` for `r = 1..N-1`) the
//! frontier is exactly row `r-1`: slot `i` corresponds to cell `(r-1, i)`.
//! The board has horizontal reflection symmetry, so every tiling pairs with
//! its horizontal mirror (counted as a distinct tiling). A state `s` at a
//! row boundary and its mirror `H(s)` have the same number of completions
//! by bijection, so we can fold the pair into a single representative
//! holding `count(s) + count(H(s))`, halving (roughly) the state space
//! entering each row.
//!
//! The collapse is only valid at row boundaries: mid-row, the DP transition
//! does not commute with horizontal mirror (the scan is left-to-right, but
//! mirror would require right-to-left placement to keep the same order of
//! cell visits). Between row boundaries we run the DP unchanged.

use crate::partition_count::count::{add_into, Count};
use crate::partition_count::state::{State, MAX_N};
use num_bigint::BigUint;
use parking_lot::Mutex;
use rayon::prelude::*;
use rustc_hash::FxHashMap;
use std::io::Write;
use std::time::{Duration, Instant};

/// Number of per-layer hashmap shards. Each produced state is routed to a
/// single shard by hashing its packed `u128`. Lock contention across 8–16
/// threads with 256 shards is negligible in practice.
const SHARD_BITS: u32 = 8;
const NUM_SHARDS: usize = 1 << SHARD_BITS;
const SHARD_MASK: usize = NUM_SHARDS - 1;

#[inline]
fn shard_idx(state: State) -> usize {
    // Fibonacci-hash of the xor-folded 128-bit state. The low slot bits of
    // `state.0` are not uniformly distributed (slot 0 is always label 1 when
    // occupied in canonical form), so we xor-fold and multiply to spread.
    let lo = state.0 as u64;
    let hi = (state.0 >> 64) as u64;
    let mixed = (lo ^ hi).wrapping_mul(0x9E3779B97F4A7C15);
    (mixed as usize) & SHARD_MASK
}

#[derive(Clone, Debug)]
pub struct CountOptions {
    pub n: usize,
    /// Abort if peak RSS exceeds this many bytes. `0` disables the check.
    pub max_memory_bytes: u64,
    /// Minimum time between progress writes to the caller-supplied writer.
    /// `Duration::ZERO` disables progress writes regardless of the writer.
    pub progress_interval: Duration,
}

impl CountOptions {
    pub fn new(n: usize) -> Self {
        Self {
            n,
            max_memory_bytes: 0,
            progress_interval: Duration::ZERO,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CountReport {
    pub n: usize,
    pub count: BigUint,
    pub peak_layer_states: usize,
    pub peak_rss_bytes: u64,
    pub elapsed: Duration,
}

#[derive(Debug)]
pub enum CountError {
    MemoryLimitExceeded {
        peak_rss_bytes: u64,
        limit_bytes: u64,
        layer: usize,
    },
    Io(std::io::Error),
}

impl std::fmt::Display for CountError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CountError::MemoryLimitExceeded {
                peak_rss_bytes,
                limit_bytes,
                layer,
            } => write!(
                f,
                "peak RSS {} exceeded limit {} after layer {}",
                peak_rss_bytes, limit_bytes, layer
            ),
            CountError::Io(e) => write!(f, "io error: {}", e),
        }
    }
}

impl std::error::Error for CountError {}

impl From<std::io::Error> for CountError {
    fn from(e: std::io::Error) -> Self {
        CountError::Io(e)
    }
}

/// Count tilings of an N×N board by exactly N fixed polyominoes, writing
/// human-readable progress lines to `progress` (typically `&mut io::stderr()`)
/// at most once per `opts.progress_interval`.
pub fn count_partitions(
    opts: &CountOptions,
    mut progress: Option<&mut dyn Write>,
) -> Result<CountReport, CountError> {
    let n = opts.n;
    assert!(n >= 1 && n <= MAX_N, "n must be in 1..={}", MAX_N);

    let total_cells = n * n;
    let n_u8 = n as u8;

    let start = Instant::now();
    let mut last_progress = start;

    let mut layer: FxHashMap<State, Count> = FxHashMap::default();
    layer.insert(State::EMPTY, Count::ONE);
    let mut peak_layer_states = layer.len();

    for k in 0..total_cells {
        let c = k % n;
        let window_size_before = k.min(n);
        let is_sliding = k >= n;
        let cells_remaining = total_cells - k - 1;

        // Row-boundary mirror-symmetry collapse: at the start of row r
        // (1 ≤ r ≤ N-1) the frontier is exactly row r-1, and horizontal
        // reflection of that row produces a state with the same completion
        // count. Fold each pair {s, H(s)} into a single representative.
        if c == 0 && k > 0 {
            layer = collapse_mirror(layer, n);
        }

        // Two-phase parallel layer advance:
        //   1. Fold: each Rayon worker builds its own thread-local
        //      `FxHashMap`, with zero lock contention on the hot path.
        //   2. Sharded merge: partials are partitioned by shard_idx(state)
        //      and flushed in parallel, one lock per shard per worker.
        //      This replaces rayon's tree `reduce` whose final merge was
        //      serial.
        let partials: Vec<FxHashMap<State, Count>> = layer
            .par_iter()
            .fold(FxHashMap::default, |mut local, (state, count)| {
                advance_one_local(
                    *state,
                    count,
                    c,
                    is_sliding,
                    window_size_before,
                    cells_remaining,
                    n_u8,
                    &mut local,
                );
                local
            })
            .collect();

        let shards: Vec<Mutex<FxHashMap<State, Count>>> =
            (0..NUM_SHARDS).map(|_| Mutex::new(FxHashMap::default())).collect();

        {
            let shards_ref: &[Mutex<FxHashMap<State, Count>>] = &shards;
            partials.into_par_iter().for_each(|partial| {
                // Bucket by shard first so we hold each lock only once.
                let mut batches: Vec<Vec<(State, Count)>> =
                    (0..NUM_SHARDS).map(|_| Vec::new()).collect();
                for (s, c) in partial {
                    batches[shard_idx(s)].push((s, c));
                }
                for (idx, batch) in batches.into_iter().enumerate() {
                    if batch.is_empty() {
                        continue;
                    }
                    let mut shard = shards_ref[idx].lock();
                    for (s, c) in batch {
                        match shard.get_mut(&s) {
                            Some(existing) => add_into(existing, &c),
                            None => {
                                shard.insert(s, c);
                            }
                        }
                    }
                }
            });
        }

        // Consolidate shards into a single map for the next iteration.
        let total_entries: usize = shards.iter().map(|m| m.lock().len()).sum();
        let mut next: FxHashMap<State, Count> =
            FxHashMap::with_capacity_and_hasher(total_entries, Default::default());
        for shard in shards.into_iter() {
            next.extend(shard.into_inner());
        }

        peak_layer_states = peak_layer_states.max(next.len());
        layer = next;

        // Memory safeguard: check peak RSS after each layer.
        let rss = peak_rss_bytes();
        if opts.max_memory_bytes > 0 && rss > opts.max_memory_bytes {
            return Err(CountError::MemoryLimitExceeded {
                peak_rss_bytes: rss,
                limit_bytes: opts.max_memory_bytes,
                layer: k + 1,
            });
        }

        // Progress emission.
        if let Some(w) = progress.as_deref_mut() {
            if !opts.progress_interval.is_zero()
                && last_progress.elapsed() >= opts.progress_interval
            {
                writeln!(
                    w,
                    "layer {}/{} | states={} | peak_rss={} | elapsed={}",
                    k + 1,
                    total_cells,
                    layer.len(),
                    format_bytes(rss),
                    format_duration(start.elapsed()),
                )?;
                w.flush()?;
                last_progress = Instant::now();
            }
        }
    }

    // Surviving states are exactly those that reached N polyominoes total;
    // the prune guarantees `active + closed == N` for every entry here.
    let mut total = Count::ZERO;
    for (_, count) in layer.iter() {
        add_into(&mut total, count);
    }

    let elapsed = start.elapsed();
    let peak_rss_bytes_final = peak_rss_bytes();

    // One final summary line if progress is enabled.
    if let Some(w) = progress.as_deref_mut() {
        if !opts.progress_interval.is_zero() {
            writeln!(
                w,
                "done | states_final={} | peak_states={} | peak_rss={} | elapsed={}",
                layer.len(),
                peak_layer_states,
                format_bytes(peak_rss_bytes_final),
                format_duration(elapsed),
            )?;
            w.flush()?;
        }
    }

    Ok(CountReport {
        n,
        count: total.into_big(),
        peak_layer_states,
        peak_rss_bytes: peak_rss_bytes_final,
        elapsed,
    })
}

fn advance_one_local(
    state: State,
    count: &Count,
    c: usize,
    is_sliding: bool,
    window_size_before: usize,
    cells_remaining: usize,
    n_u8: u8,
    next: &mut FxHashMap<State, Count>,
) {
    let up_label = if is_sliding { state.slot(0) } else { 0 };
    let left_label = if c > 0 {
        state.slot(window_size_before - 1)
    } else {
        0
    };
    let active = state.active();

    // A. Start a new polyomino.
    {
        let new_label = active + 1;
        let produced = apply_place(state, new_label, is_sliding, window_size_before);
        try_accept_local(next, produced, count, n_u8, cells_remaining);
    }

    // B. Join the up-neighbor's polyomino.
    if up_label != 0 {
        let produced = apply_place(state, up_label, is_sliding, window_size_before);
        try_accept_local(next, produced, count, n_u8, cells_remaining);
    }

    // C. Join the left-neighbor's polyomino (skip if it's the same as up's).
    if left_label != 0 && left_label != up_label {
        let produced = apply_place(state, left_label, is_sliding, window_size_before);
        try_accept_local(next, produced, count, n_u8, cells_remaining);
    }

    // D. Merge up and left.
    if up_label != 0 && left_label != 0 && up_label != left_label {
        let keep = up_label.min(left_label);
        let drop_ = up_label.max(left_label);
        let merged = state.merge_labels(drop_, keep);
        let produced = apply_place(merged, keep, is_sliding, window_size_before);
        try_accept_local(next, produced, count, n_u8, cells_remaining);
    }
}

fn apply_place(
    state: State,
    new_label: u8,
    is_sliding: bool,
    window_size_before: usize,
) -> State {
    let old_closed = state.closed();
    if is_sliding {
        let (shifted, _dropped, was_closed) = state.shift_in(window_size_before, new_label);
        let new_closed = old_closed + if was_closed { 1 } else { 0 };
        shifted.with_closed(new_closed).canonicalize()
    } else {
        state
            .with_slot(window_size_before, new_label)
            .canonicalize()
    }
}

fn try_accept_local(
    next: &mut FxHashMap<State, Count>,
    produced: State,
    count: &Count,
    n_u8: u8,
    cells_remaining: usize,
) {
    if produced.closed() > n_u8 {
        return;
    }
    let total = (produced.active() as isize) + (produced.closed() as isize);
    let gap = (total - n_u8 as isize).unsigned_abs();
    if gap > cells_remaining {
        return;
    }
    match next.get_mut(&produced) {
        Some(existing) => add_into(existing, count),
        None => {
            next.insert(produced, count.clone());
        }
    }
}

/// Fold each state in the layer to the `min(s, H(s))` representative and
/// accumulate counts. `H(s)` is `s.mirror(n)`.
///
/// The collapse is correct regardless of whether `H(s)` happens to be in the
/// layer: a lone state `s` (no mirror partner present) still maps to its
/// representative `min(s, H(s))`, and the DP from the representative has the
/// same completion count as the DP from `s` by the horizontal-symmetry
/// bijection. Collapsing this way after the first row boundary is necessary
/// because subsequent layers are derived from representatives and therefore
/// lack the mirror partners that existed in the original state space.
fn collapse_mirror(layer: FxHashMap<State, Count>, n: usize) -> FxHashMap<State, Count> {
    let mut out: FxHashMap<State, Count> =
        FxHashMap::with_capacity_and_hasher(layer.len() / 2 + 8, Default::default());
    for (s, c) in layer.iter() {
        let m = s.mirror(n);
        let rep = if *s <= m { *s } else { m };
        match out.get_mut(&rep) {
            Some(existing) => add_into(existing, c),
            None => {
                out.insert(rep, c.clone());
            }
        }
    }
    out
}

/// Resident set size peak, in bytes. Falls back to `0` on unsupported
/// platforms. Uses `getrusage(RUSAGE_SELF)` — on macOS `ru_maxrss` is bytes,
/// on Linux it's kilobytes.
pub fn peak_rss_bytes() -> u64 {
    use libc::{getrusage, rusage, RUSAGE_SELF};
    let mut u: rusage = unsafe { std::mem::zeroed() };
    if unsafe { getrusage(RUSAGE_SELF, &mut u) } != 0 {
        return 0;
    }
    let raw = u.ru_maxrss as u64;
    if cfg!(target_os = "macos") {
        raw
    } else {
        // Linux (and most other Unixes) report kilobytes.
        raw.saturating_mul(1024)
    }
}

fn format_bytes(bytes: u64) -> String {
    const KI: u64 = 1024;
    const MI: u64 = KI * 1024;
    const GI: u64 = MI * 1024;
    if bytes >= GI {
        format!("{:.2} GiB", bytes as f64 / GI as f64)
    } else if bytes >= MI {
        format!("{:.2} MiB", bytes as f64 / MI as f64)
    } else if bytes >= KI {
        format!("{:.2} KiB", bytes as f64 / KI as f64)
    } else {
        format!("{} B", bytes)
    }
}

fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    let ms = d.subsec_millis();
    if h > 0 {
        format!("{:02}:{:02}:{:02}", h, m, s)
    } else {
        format!("{:02}:{:02}.{:03}", m, s, ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(n: usize) -> BigUint {
        count_partitions(&CountOptions::new(n), None)
            .unwrap()
            .count
    }

    #[test]
    fn n1_has_one_tiling() {
        assert_eq!(c(1), BigUint::from(1u32));
    }

    #[test]
    fn n2_by_hand() {
        // 2×2 board, exactly 2 fixed polyominoes. Enumerating unordered
        // pairs of connected pieces that cover the board gives 4 of
        // sizes (1, 3) plus 2 of sizes (2, 2) = 6.
        assert_eq!(c(2), BigUint::from(6u32));
    }

    #[test]
    fn peak_layer_is_reported() {
        let report = count_partitions(&CountOptions::new(3), None).unwrap();
        assert!(report.peak_layer_states >= 1);
    }

    #[test]
    fn small_n_counts_are_positive() {
        for n in 1..=4 {
            let v = c(n);
            assert!(v > BigUint::from(0u32), "n={}: {}", n, v);
        }
    }
}
