//! Frontier DP that counts partitions of an N×N board into exactly N
//! connected components (equivalently, tilings by exactly N fixed
//! polyominoes).
//!
//! Cells are placed in row-major order. A `FxHashMap<State, Count>` holds the
//! current layer; after each cell is placed the old layer is replaced by the
//! newly-built one. At any point during the scan, the state describes the
//! labeling of the last `min(k, N)` placed cells — a superset of the true
//! frontier — plus a **Forbidden-Pair Set (FPS)** recording pairs of
//! components that are forbidden from ever merging. Labels are kept in
//! restricted-growth canonical form; the FPS is permuted by the same
//! renumbering, so two states are equal iff their labeled-and-forbidden
//! structure is equal.
//!
//! **Why the FPS.** Without it, "start a new component at X and merge it
//! with U later" produces the same final partition as "join U directly
//! at X", and both paths would be counted. Following Graphillion's
//! `GraphRangePartitionSpec::{takable, doTake, doNotTake}`, each placement
//! of cell X commits the two incident grid edges (X, up) and (X, left): each
//! edge is either *taken* (X joins that neighbor's component) or *not taken*
//! (the component pair is added to FPS). Each partition then corresponds to
//! exactly one sequence of edge decisions — take every within-block edge,
//! don't-take every between-block edge — so it is counted once.
//!
//! For each cell k the up-neighbor U (slot 0 in the pre-place window) and
//! the left-neighbor L (last occupied slot when c > 0) drive the transition
//! choice. The combos and the FPS effects are:
//!
//!   | up | left | transitions |
//!   |----|------|-------------|
//!   | –  | –    | new |
//!   | U  | –    | join U;  new + FPS(new, U) |
//!   | –  | L    | join L;  new + FPS(new, L) |
//!   | U==L | U==L | join;   new + FPS(new, U) |
//!   | U≠L | U≠L  | merge U,L (iff ¬FPS(U,L));  join U + FPS(U,L);  join L + FPS(L,U);  new + FPS(new, U) + FPS(new, L) |
//!
//! The reachability prune `|total − N| ≤ cells_remaining` drops any state
//! that can no longer hit exactly N components by the final cell.
//!
//! After placement the window slides when `k ≥ N`: slot 0 drops and may
//! close a polyomino (its label is incremented in `closed`, and its FPS
//! row is cleared). `closed` is monotone non-decreasing, so `closed > N`
//! is a hard prune.
//!
//! Layer advance is parallelized with Rayon in four phases:
//!   1. **Emit.** Each worker iterates its slice of the current layer and
//!      pushes the produced `(State, Count)` transitions onto a thread-local
//!      `Vec` — no deduplication at this stage, just raw emissions.
//!   2. **Bucket.** Each worker redistributes its emissions into 256
//!      shards keyed by `shard_idx(state)`; per-shard batches are flushed
//!      into shared `Mutex<Vec<(State, Count)>>` shards (one lock
//!      acquisition per shard per worker).
//!   3. **Sort + fold.** Each shard is sorted by `State` in parallel with
//!      `sort_unstable_by_key`, then a linear scan folds consecutive equal
//!      states by summing their counts.
//!   4. **Concat.** Shard results are concatenated into the next layer's
//!      `Vec`. The vec is not globally sorted — shard order is by hash —
//!      but the invariant "each `State` appears exactly once" holds.
//!
//! This replaces an earlier design that used `FxHashMap<State, Count>`
//! per shard; random-access hashmap operations were the top profile item
//! once canonicalize got optimized, and sort+fold is measurably faster in
//! sequential-memory terms.
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
    // Fibonacci-hash of the xor-folded state words. The low slot bits of
    // `labels` are not uniformly distributed (slot 0 is always label 1 when
    // occupied in canonical form), so we xor-fold and multiply to spread.
    let labels_lo = state.labels as u64;
    let labels_hi = (state.labels >> 64) as u64;
    let fps_lo = state.fps as u64;
    let fps_hi = (state.fps >> 64) as u64;
    let folded = labels_lo ^ labels_hi ^ fps_lo ^ fps_hi;
    let mixed = folded.wrapping_mul(0x9E3779B97F4A7C15);
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

    let mut layer: Vec<(State, Count)> = vec![(State::EMPTY, Count::ONE)];
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

        // Phases 1 & 2 fused: each rayon chunk emits, sort+folds its own
        // partial locally, then buckets it into the 256 shared shards — all
        // in one streaming pipeline. By running the bucket+flush as a
        // for_each stage on the fold's chunk accumulators (rather than
        // collecting them into a Vec<Vec> first), only O(num_threads)
        // partials are alive simultaneously instead of all of them. At
        // N=10 this drops the transient materialized-partials peak from
        // ~400 MB to tens of MB.
        //
        // We also take ownership of `layer` here so we can drop it
        // explicitly once phase 1 completes — no sense keeping the entire
        // prior layer alive while phase 3 allocates space for the next.
        let old_layer = std::mem::take(&mut layer);
        let shards: Vec<Mutex<Vec<(State, Count)>>> =
            (0..NUM_SHARDS).map(|_| Mutex::new(Vec::new())).collect();
        {
            let shards_ref: &[Mutex<Vec<(State, Count)>>] = &shards;
            old_layer
                .par_iter()
                .fold(
                    || Vec::with_capacity(1024),
                    |mut local, (state, count)| {
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
                    },
                )
                .for_each(|mut partial| {
                    partial.sort_unstable_by_key(|(s, _)| *s);
                    fold_sorted_in_place(&mut partial);
                    let mut batches: Vec<Vec<(State, Count)>> =
                        (0..NUM_SHARDS).map(|_| Vec::new()).collect();
                    for entry in partial {
                        let idx = shard_idx(entry.0);
                        batches[idx].push(entry);
                    }
                    for (idx, batch) in batches.into_iter().enumerate() {
                        if batch.is_empty() {
                            continue;
                        }
                        let mut shard = shards_ref[idx].lock();
                        shard.extend(batch);
                    }
                });
        }
        drop(old_layer);

        // Phase 3: sort + linear fold per shard, in parallel. Reclaim
        // the extend-growth over-allocation before sorting so we don't
        // hold 2× buffers across the parallel phase.
        let deduped: Vec<Vec<(State, Count)>> = shards
            .into_par_iter()
            .map(|mutex| {
                let mut buf = mutex.into_inner();
                buf.shrink_to_fit();
                buf.sort_unstable_by_key(|(s, _)| *s);
                fold_sorted_in_place(&mut buf);
                buf.shrink_to_fit();
                buf
            })
            .collect();

        // Phase 4: concatenate shard results into the next layer.
        let total_entries: usize = deduped.iter().map(|v| v.len()).sum();
        let mut next: Vec<(State, Count)> = Vec::with_capacity(total_entries);
        for v in deduped {
            next.extend(v);
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
    next: &mut Vec<(State, Count)>,
) {
    let up_label = if is_sliding { state.slot(0) } else { 0 };
    let left_label = if c > 0 {
        state.slot(window_size_before - 1)
    } else {
        0
    };
    let active = state.active();
    let new_fresh_label = active + 1;

    match (up_label, left_label) {
        (0, 0) => {
            // No placed neighbors: X must start a new component.
            emit_new(
                state, new_fresh_label, &[], is_sliding, window_size_before,
                count, n_u8, cells_remaining, next,
            );
        }
        (u, 0) => {
            emit_join(
                state, u, is_sliding, window_size_before,
                count, n_u8, cells_remaining, next,
            );
            emit_new(
                state, new_fresh_label, &[u], is_sliding, window_size_before,
                count, n_u8, cells_remaining, next,
            );
        }
        (0, l) => {
            emit_join(
                state, l, is_sliding, window_size_before,
                count, n_u8, cells_remaining, next,
            );
            emit_new(
                state, new_fresh_label, &[l], is_sliding, window_size_before,
                count, n_u8, cells_remaining, next,
            );
        }
        (u, l) if u == l => {
            // Both neighbors share a component: join, or start new + FPS(new, U).
            emit_join(
                state, u, is_sliding, window_size_before,
                count, n_u8, cells_remaining, next,
            );
            emit_new(
                state, new_fresh_label, &[u], is_sliding, window_size_before,
                count, n_u8, cells_remaining, next,
            );
        }
        (u, l) => {
            // Distinct components: four combos.
            // (T,T): merge U and L, X joins merged component. Blocked if
            // U and L are already in FPS.
            if !state.fps_bit(u, l) {
                let keep = u.min(l);
                let drop_ = u.max(l);
                let merged = state.merge_labels(drop_, keep);
                let produced = apply_place(merged, keep, is_sliding, window_size_before);
                try_accept_emit(next, produced, count, n_u8, cells_remaining);
            }
            // (T,D): X joins U, edge (X,L) not-taken → FPS(U, L).
            emit_join_with_forbid(
                state, u, l, is_sliding, window_size_before,
                count, n_u8, cells_remaining, next,
            );
            // (D,T): X joins L, edge (X,U) not-taken → FPS(L, U).
            emit_join_with_forbid(
                state, l, u, is_sliding, window_size_before,
                count, n_u8, cells_remaining, next,
            );
            // (D,D): X starts new, both edges not-taken → FPS(new, U) and FPS(new, L).
            emit_new(
                state, new_fresh_label, &[u, l], is_sliding, window_size_before,
                count, n_u8, cells_remaining, next,
            );
        }
    }
}

#[inline]
fn emit_join(
    state: State,
    join_label: u8,
    is_sliding: bool,
    window_size_before: usize,
    count: &Count,
    n_u8: u8,
    cells_remaining: usize,
    next: &mut Vec<(State, Count)>,
) {
    let produced = apply_place(state, join_label, is_sliding, window_size_before);
    try_accept_emit(next, produced, count, n_u8, cells_remaining);
}

#[inline]
fn emit_join_with_forbid(
    state: State,
    join_label: u8,
    forbid_label: u8,
    is_sliding: bool,
    window_size_before: usize,
    count: &Count,
    n_u8: u8,
    cells_remaining: usize,
    next: &mut Vec<(State, Count)>,
) {
    let with_fps = state.with_fps_set(join_label, forbid_label);
    let produced = apply_place(with_fps, join_label, is_sliding, window_size_before);
    try_accept_emit(next, produced, count, n_u8, cells_remaining);
}

#[inline]
fn emit_new(
    state: State,
    new_label: u8,
    forbid_with: &[u8],
    is_sliding: bool,
    window_size_before: usize,
    count: &Count,
    n_u8: u8,
    cells_remaining: usize,
    next: &mut Vec<(State, Count)>,
) {
    let mut s = state;
    for &other in forbid_with {
        s = s.with_fps_set(new_label, other);
    }
    let produced = apply_place(s, new_label, is_sliding, window_size_before);
    try_accept_emit(next, produced, count, n_u8, cells_remaining);
}

fn apply_place(
    state: State,
    new_label: u8,
    is_sliding: bool,
    window_size_before: usize,
) -> State {
    let old_closed = state.closed();
    if is_sliding {
        let (shifted, dropped, was_closed) = state.shift_in(window_size_before, new_label);
        let new_closed = old_closed + if was_closed { 1 } else { 0 };
        let mut s = shifted.with_closed(new_closed);
        if was_closed {
            // The dropped label's polyomino has fully left the frontier —
            // its FPS entries can never matter again, so drop them.
            s = s.with_fps_row_cleared(dropped);
        }
        s.canonicalize()
    } else {
        state
            .with_slot(window_size_before, new_label)
            .canonicalize()
    }
}

fn try_accept_emit(
    next: &mut Vec<(State, Count)>,
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
    next.push((produced, count.clone()));
}

/// In-place linear fold on a vec that is already sorted by `State`:
/// consecutive equal states get their counts summed and the duplicates
/// are dropped. Reuses the input vec's allocation — no second buffer —
/// to keep peak memory per-shard equal to the pre-dedup size rather than
/// twice that.
fn fold_sorted_in_place(v: &mut Vec<(State, Count)>) {
    let len = v.len();
    if len < 2 {
        return;
    }
    let mut write = 0usize;
    for read in 1..len {
        if v[read].0 == v[write].0 {
            // Move the read-side count into `taken` and accumulate into write.
            let taken = std::mem::replace(&mut v[read].1, Count::ZERO);
            add_into(&mut v[write].1, &taken);
        } else {
            write += 1;
            if write != read {
                v.swap(write, read);
            }
        }
    }
    v.truncate(write + 1);
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
fn collapse_mirror(layer: Vec<(State, Count)>, n: usize) -> Vec<(State, Count)> {
    let mut mapped: Vec<(State, Count)> = layer
        .into_iter()
        .map(|(s, c)| {
            let m = s.mirror(n);
            let rep = if s <= m { s } else { m };
            (rep, c)
        })
        .collect();
    mapped.sort_unstable_by_key(|(s, _)| *s);
    fold_sorted_in_place(&mut mapped);
    mapped.shrink_to_fit();
    mapped
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
