//! Packed representation of a Simpath frontier state.
//!
//! A state describes the labeling of a sliding window of up to `MAX_N` cells —
//! the most recent `N` placed cells in row-major order, which is a superset of
//! the true frontier (placed cells with at least one unplaced neighbor). Any
//! slot whose cell is off the frontier carries label 0 ("empty").
//!
//! Labels use restricted-growth canonical form: scanning slots from index 0
//! upward, the first non-empty label is 1, the next new label is 2, and so on.
//! Two states are considered equal iff their canonical forms are equal.
//!
//! Bit layout of the packed `labels` `u128`:
//!   `[0 .. 75)`   — 15 slots × 5 bits. Slot `i` occupies bits `[5i, 5i+5)`.
//!                   Value 0 = empty; values 1..=N are polyomino labels.
//!   `[75 .. 80)`  — `closed_polyominoes` count (0..=N).
//!   `[80 .. 128)` — reserved.
//!
//! The state also carries a **Forbidden-Pair Set (FPS)**: a symmetric
//! adjacency bit matrix over labels that records which pairs of components
//! are forbidden from ever merging. This is what makes each partition count
//! exactly once — see `simpath.rs` and Graphillion's
//! `GraphRangePartitionSpec.h` for the derivation. Pair `(i, j)` with
//! `1 ≤ i < j ≤ MAX_N + 1` is encoded at bit index
//! `(j - 1) * (j - 2) / 2 + (i - 1)`. With `MAX_N = 15` the transient label
//! ceiling is 16 (a fresh label created during a transition can briefly
//! reach `MAX_N + 1` before canonicalize renumbers it), giving
//! `16 * 15 / 2 = 120` pairs — fits in a single `u128`.

pub const MAX_N: usize = 15;

const SLOT_BITS: u32 = 5;
const SLOT_MASK: u128 = (1u128 << SLOT_BITS) - 1;
const LABELS_BITS: u32 = MAX_N as u32 * SLOT_BITS;
const LABELS_MASK: u128 = (1u128 << LABELS_BITS) - 1;

const CLOSED_SHIFT: u32 = LABELS_BITS;
const CLOSED_BITS: u32 = 5;
const CLOSED_MASK: u128 = ((1u128 << CLOSED_BITS) - 1) << CLOSED_SHIFT;

/// Max distinct label value the FPS matrix can index. Transiently one
/// larger than MAX_N.
const FPS_MAX_LABEL: usize = MAX_N + 1;

/// Total triangular pair count. Each bit in the FPS `u128` buffer represents
/// one of these pairs. `FPS_MAX_LABEL = 16` → 120 bits.
const FPS_NUM_PAIRS: usize = FPS_MAX_LABEL * (FPS_MAX_LABEL - 1) / 2;

/// Bit-index ↔ (lo, hi) pair lookup. Lets hot paths iterate only set bits
/// via `trailing_zeros` instead of scanning all 120 possible label pairs.
const INDEX_TO_PAIR: [(u8, u8); FPS_NUM_PAIRS] = {
    let mut arr = [(0u8, 0u8); FPS_NUM_PAIRS];
    let mut j: usize = 2;
    while j <= FPS_MAX_LABEL {
        let mut i: usize = 1;
        while i < j {
            let idx = (j - 1) * (j - 2) / 2 + (i - 1);
            arr[idx] = (i as u8, j as u8);
            i += 1;
        }
        j += 1;
    }
    arr
};

/// Per-label "row mask": `ROW_MASKS[a]` has bits set for every pair `(a, b)`
/// with `b != a`, `1 ≤ b ≤ FPS_MAX_LABEL`. Enables one-instruction row
/// clear / row extract.
const ROW_MASKS: [u128; FPS_MAX_LABEL + 1] = {
    let mut masks = [0u128; FPS_MAX_LABEL + 1];
    let mut a: usize = 1;
    while a <= FPS_MAX_LABEL {
        let mut b: usize = 1;
        while b <= FPS_MAX_LABEL {
            if b != a {
                let (lo, hi) = if a < b { (a, b) } else { (b, a) };
                let idx = (hi - 1) * (hi - 2) / 2 + (lo - 1);
                masks[a] |= 1u128 << idx;
            }
            b += 1;
        }
        a += 1;
    }
    masks
};

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, PartialOrd, Ord)]
pub struct State {
    pub labels: u128,
    pub fps: u128,
}

impl State {
    pub const EMPTY: State = State {
        labels: 0,
        fps: 0,
    };

    #[inline]
    pub fn slot(self, i: usize) -> u8 {
        debug_assert!(i < MAX_N);
        ((self.labels >> (i as u32 * SLOT_BITS)) & SLOT_MASK) as u8
    }

    #[inline]
    pub fn with_slot(self, i: usize, value: u8) -> State {
        debug_assert!(i < MAX_N);
        debug_assert!((value as u128) <= SLOT_MASK);
        let shift = i as u32 * SLOT_BITS;
        let cleared = self.labels & !(SLOT_MASK << shift);
        State {
            labels: cleared | ((value as u128) << shift),
            fps: self.fps,
        }
    }

    #[inline]
    pub fn closed(self) -> u8 {
        ((self.labels & CLOSED_MASK) >> CLOSED_SHIFT) as u8
    }

    #[inline]
    pub fn with_closed(self, closed: u8) -> State {
        debug_assert!((closed as u32) < (1u32 << CLOSED_BITS));
        let cleared = self.labels & !CLOSED_MASK;
        State {
            labels: cleared | ((closed as u128) << CLOSED_SHIFT),
            fps: self.fps,
        }
    }

    /// Count of distinct non-empty labels currently in the window.
    pub fn active(self) -> u8 {
        let mut seen: u32 = 0;
        for i in 0..MAX_N {
            let v = self.slot(i);
            if v != 0 {
                seen |= 1u32 << v;
            }
        }
        seen.count_ones() as u8
    }

    /// Slide the window left by one: slot 0 drops out, slots shift down, the
    /// new cell enters at slot `window_len - 1` carrying `new_label`.
    ///
    /// Returns `(new_state, dropped_label, polyomino_closed)` where:
    ///   * `dropped_label` is the label that fell out of slot 0 (0 if empty).
    ///   * `polyomino_closed` is true iff the dropped label no longer appears
    ///     in the new window (its polyomino has fully left the frontier).
    ///
    /// `window_len` must satisfy `1 <= window_len <= MAX_N`. Slots at indices
    /// >= window_len are left as 0. FPS is preserved as-is; callers who care
    /// about closed-polyomino cleanup should call `with_fps_row_cleared`.
    ///
    /// Does not canonicalize or update `closed`; callers do that.
    pub fn shift_in(self, window_len: usize, new_label: u8) -> (State, u8, bool) {
        debug_assert!(window_len >= 1 && window_len <= MAX_N);
        let dropped = self.slot(0);
        let mut out = self.labels & !LABELS_MASK;
        // Shift existing slots [1 .. window_len) down to [0 .. window_len - 1).
        for i in 0..(window_len - 1) {
            let v = self.slot(i + 1);
            out |= (v as u128) << (i as u32 * SLOT_BITS);
        }
        // Place new cell at slot (window_len - 1).
        out |= (new_label as u128) << ((window_len as u32 - 1) * SLOT_BITS);
        let new_state = State {
            labels: out,
            fps: self.fps,
        };
        let closed = dropped != 0 && {
            // Check whether `dropped` still appears in slots [0 .. window_len).
            let mut present = false;
            for i in 0..window_len {
                if new_state.slot(i) == dropped {
                    present = true;
                    break;
                }
            }
            !present
        };
        (new_state, dropped, closed)
    }

    /// Rewrite labels so the first non-empty label (in slot-index order) is 1,
    /// the next new label is 2, etc. Metadata (closed count) is preserved.
    /// FPS bits are permuted by the same label renumbering, and any FPS
    /// entries involving labels that do not appear in the window are dropped.
    pub fn canonicalize(self) -> State {
        // Labels range over 1..=MAX_N+1 inclusive during transient merges.
        let mut remap = [0u8; FPS_MAX_LABEL + 1];
        let mut next_label: u8 = 1;
        let mut out = self.labels & !LABELS_MASK;
        for i in 0..MAX_N {
            let v = self.slot(i);
            let new_v = if v == 0 {
                0
            } else {
                let idx = v as usize;
                debug_assert!(idx < remap.len());
                if remap[idx] == 0 {
                    remap[idx] = next_label;
                    next_label += 1;
                }
                remap[idx]
            };
            out |= (new_v as u128) << (i as u32 * SLOT_BITS);
        }
        let new_fps = permute_fps(self.fps, &remap);
        State {
            labels: out,
            fps: new_fps,
        }
    }

    /// Horizontally mirror the first `n` slots and return the canonicalized
    /// state: slot `i` swaps with slot `n − 1 − i`, then labels are
    /// restricted-growth renumbered. `closed` is preserved; FPS is permuted
    /// through the canonicalize step.
    pub fn mirror(self, n: usize) -> State {
        debug_assert!(n >= 1 && n <= MAX_N);
        let mut out = self.labels & !LABELS_MASK;
        for i in 0..n {
            let v = self.slot(i);
            let mirror_pos = n - 1 - i;
            out |= (v as u128) << (mirror_pos as u32 * SLOT_BITS);
        }
        State {
            labels: out,
            fps: self.fps,
        }
        .canonicalize()
    }

    /// Merge two labels in-place: every slot carrying `from` becomes `to`,
    /// and the FPS row for `from` is unioned into `to` before being cleared.
    /// Returns the merged (but not yet canonicalized) state.
    pub fn merge_labels(self, from: u8, to: u8) -> State {
        debug_assert!(from != 0 && to != 0 && from != to);
        debug_assert!((from as usize) <= FPS_MAX_LABEL && (to as usize) <= FPS_MAX_LABEL);
        let mut out = self.labels & !LABELS_MASK;
        for i in 0..MAX_N {
            let v = self.slot(i);
            let new_v = if v == from { to } else { v };
            out |= (new_v as u128) << (i as u32 * SLOT_BITS);
        }
        // Bit-iterate the set bits in `from`'s row, redirecting each to the
        // corresponding bit in `to`'s row. Then clear the entire `from` row.
        let mut fps = self.fps;
        let from_mask = ROW_MASKS[from as usize];
        let mut from_bits = fps & from_mask;
        while from_bits != 0 {
            let idx = from_bits.trailing_zeros() as usize;
            from_bits &= from_bits - 1;
            let (a, b) = INDEX_TO_PAIR[idx];
            let other = if a == from { b } else { a };
            // If the other endpoint is `to`, the pair (from, to) drops
            // rather than folds — the components are one now.
            if other != to {
                set_fps_pair(&mut fps, to, other, true);
            }
        }
        fps &= !from_mask;
        State { labels: out, fps }
    }

    /// Test the FPS bit for pair `(i, j)` with `i != j`.
    #[inline]
    pub fn fps_bit(self, i: u8, j: u8) -> bool {
        fps_pair_bit(&self.fps, i, j)
    }

    /// Return a copy with `FPS(i, j)` set.
    #[inline]
    pub fn with_fps_set(self, i: u8, j: u8) -> State {
        let mut fps = self.fps;
        set_fps_pair(&mut fps, i, j, true);
        State {
            labels: self.labels,
            fps,
        }
    }

    /// Return a copy with every FPS bit involving label `a` cleared. Used
    /// when a label's polyomino closes and leaves the frontier.
    pub fn with_fps_row_cleared(self, a: u8) -> State {
        debug_assert!(a != 0 && (a as usize) <= FPS_MAX_LABEL);
        State {
            labels: self.labels,
            fps: self.fps & !ROW_MASKS[a as usize],
        }
    }
}

/// Bit index in the 128-bit FPS buffer for pair `(i, j)` with `i < j`.
#[inline]
fn pair_index(i: u8, j: u8) -> usize {
    debug_assert!(i != 0 && j != 0 && i != j);
    let (lo, hi) = if i < j { (i, j) } else { (j, i) };
    let hi = hi as usize;
    let lo = lo as usize;
    (hi - 1) * (hi - 2) / 2 + (lo - 1)
}

#[inline]
fn fps_pair_bit(fps: &u128, i: u8, j: u8) -> bool {
    (*fps >> pair_index(i, j)) & 1 == 1
}

#[inline]
fn set_fps_pair(fps: &mut u128, i: u8, j: u8, v: bool) {
    let mask = 1u128 << pair_index(i, j);
    if v {
        *fps |= mask;
    } else {
        *fps &= !mask;
    }
}

/// Apply `remap` (indexed by old label, yielding new label or 0 to drop) to
/// the FPS matrix. Any pair whose endpoints remap to 0 is dropped.
fn permute_fps(fps: u128, remap: &[u8]) -> u128 {
    let mut out = 0u128;
    let mut bits = fps;
    while bits != 0 {
        let idx = bits.trailing_zeros() as usize;
        bits &= bits - 1;
        debug_assert!(idx < FPS_NUM_PAIRS);
        let (a, b) = INDEX_TO_PAIR[idx];
        let new_a = remap[a as usize];
        let new_b = remap[b as usize];
        if new_a != 0 && new_b != 0 {
            set_fps_pair(&mut out, new_a, new_b, true);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_round_trip() {
        let mut s = State::EMPTY;
        for i in 0..MAX_N {
            s = s.with_slot(i, (i as u8) + 1);
        }
        for i in 0..MAX_N {
            assert_eq!(s.slot(i), (i as u8) + 1);
        }
    }

    #[test]
    fn closed_round_trip() {
        let s = State::EMPTY.with_closed(7);
        assert_eq!(s.closed(), 7);
        // Setting slots must not disturb `closed`.
        let s2 = s.with_slot(0, 3).with_slot(5, 12);
        assert_eq!(s2.closed(), 7);
        assert_eq!(s2.slot(0), 3);
        assert_eq!(s2.slot(5), 12);
    }

    #[test]
    fn closed_max_value() {
        let s = State::EMPTY.with_closed(MAX_N as u8);
        assert_eq!(s.closed(), MAX_N as u8);
    }

    #[test]
    fn active_counts_distinct_nonzero_labels() {
        let s = State::EMPTY
            .with_slot(0, 1)
            .with_slot(1, 2)
            .with_slot(2, 1)
            .with_slot(3, 3);
        assert_eq!(s.active(), 3);
    }

    #[test]
    fn active_zero_when_all_empty() {
        assert_eq!(State::EMPTY.active(), 0);
    }

    #[test]
    fn canonicalize_is_idempotent() {
        let s = State::EMPTY
            .with_slot(0, 1)
            .with_slot(1, 2)
            .with_slot(2, 1)
            .with_slot(3, 3)
            .with_closed(4);
        let c = s.canonicalize();
        assert_eq!(c, c.canonicalize());
    }

    #[test]
    fn canonicalize_renumbers_out_of_order_labels() {
        let s = State::EMPTY
            .with_slot(0, 7)
            .with_slot(1, 3)
            .with_slot(2, 7)
            .with_slot(3, 5);
        let c = s.canonicalize();
        assert_eq!(c.slot(0), 1);
        assert_eq!(c.slot(1), 2);
        assert_eq!(c.slot(2), 1);
        assert_eq!(c.slot(3), 3);
    }

    #[test]
    fn canonicalize_preserves_closed() {
        let s = State::EMPTY.with_slot(0, 9).with_closed(5);
        assert_eq!(s.canonicalize().closed(), 5);
    }

    #[test]
    fn canonicalize_equates_permuted_labelings() {
        let a = State::EMPTY
            .with_slot(0, 1)
            .with_slot(1, 2)
            .with_slot(2, 1);
        let b = State::EMPTY
            .with_slot(0, 5)
            .with_slot(1, 3)
            .with_slot(2, 5);
        assert_eq!(a.canonicalize(), b.canonicalize());
    }

    #[test]
    fn canonicalize_treats_empty_as_distinct() {
        let s = State::EMPTY.with_slot(0, 0).with_slot(1, 4).with_slot(2, 0);
        let c = s.canonicalize();
        assert_eq!(c.slot(0), 0);
        assert_eq!(c.slot(1), 1);
        assert_eq!(c.slot(2), 0);
    }

    #[test]
    fn shift_in_drops_oldest_and_adds_new() {
        let s = State::EMPTY.with_slot(0, 1).with_slot(1, 2).with_slot(2, 3);
        let (new_state, dropped, closed) = s.shift_in(3, 4);
        assert_eq!(dropped, 1);
        assert!(closed);
        assert_eq!(new_state.slot(0), 2);
        assert_eq!(new_state.slot(1), 3);
        assert_eq!(new_state.slot(2), 4);
    }

    #[test]
    fn shift_in_reports_not_closed_when_label_still_present() {
        let s = State::EMPTY.with_slot(0, 1).with_slot(1, 2).with_slot(2, 1);
        let (_, dropped, closed) = s.shift_in(3, 3);
        assert_eq!(dropped, 1);
        assert!(!closed);
    }

    #[test]
    fn shift_in_empty_slot_is_not_a_close() {
        let s = State::EMPTY.with_slot(1, 2).with_slot(2, 3);
        let (_, dropped, closed) = s.shift_in(3, 4);
        assert_eq!(dropped, 0);
        assert!(!closed);
    }

    #[test]
    fn mirror_palindromic_labeling_is_self() {
        let s = State::EMPTY
            .with_slot(0, 1)
            .with_slot(1, 2)
            .with_slot(2, 1);
        assert_eq!(s.mirror(3), s);
    }

    #[test]
    fn mirror_strictly_increasing_normalizes_to_self() {
        let s = State::EMPTY
            .with_slot(0, 1)
            .with_slot(1, 2)
            .with_slot(2, 3);
        assert_eq!(s.mirror(3), s);
    }

    #[test]
    fn mirror_distinct_example() {
        let s = State::EMPTY
            .with_slot(0, 1)
            .with_slot(1, 2)
            .with_slot(2, 2);
        let expected = State::EMPTY
            .with_slot(0, 1)
            .with_slot(1, 1)
            .with_slot(2, 2);
        assert_eq!(s.mirror(3), expected);
    }

    #[test]
    fn mirror_is_involution() {
        let examples = [
            State::EMPTY
                .with_slot(0, 1)
                .with_slot(1, 2)
                .with_slot(2, 3),
            State::EMPTY
                .with_slot(0, 1)
                .with_slot(1, 2)
                .with_slot(2, 2),
            State::EMPTY
                .with_slot(0, 1)
                .with_slot(1, 2)
                .with_slot(2, 3)
                .with_slot(3, 4)
                .with_slot(4, 1),
            State::EMPTY
                .with_slot(0, 1)
                .with_slot(1, 1)
                .with_slot(2, 2)
                .with_slot(3, 3)
                .with_slot(4, 1),
        ];
        for (idx, s) in examples.iter().enumerate() {
            let n = if idx < 2 { 3 } else { 5 };
            let twice = s.mirror(n).mirror(n);
            assert_eq!(twice, *s, "idx={}", idx);
        }
    }

    #[test]
    fn mirror_preserves_closed() {
        let s = State::EMPTY
            .with_slot(0, 1)
            .with_slot(1, 2)
            .with_slot(2, 2)
            .with_closed(4);
        let m = s.mirror(3);
        assert_eq!(m.closed(), 4);
    }

    #[test]
    fn mirror_only_touches_first_n_slots() {
        let s = State::EMPTY
            .with_slot(0, 1)
            .with_slot(1, 2)
            .with_slot(2, 2);
        let m = s.mirror(3);
        for i in 3..MAX_N {
            assert_eq!(m.slot(i), 0, "slot {}", i);
        }
    }

    #[test]
    fn merge_labels_replaces_all_occurrences() {
        let s = State::EMPTY
            .with_slot(0, 2)
            .with_slot(1, 3)
            .with_slot(2, 2)
            .with_slot(3, 4);
        let m = s.merge_labels(2, 3);
        assert_eq!(m.slot(0), 3);
        assert_eq!(m.slot(1), 3);
        assert_eq!(m.slot(2), 3);
        assert_eq!(m.slot(3), 4);
    }

    #[test]
    fn fps_round_trip() {
        let s = State::EMPTY.with_fps_set(2, 5).with_fps_set(1, 3);
        assert!(s.fps_bit(2, 5));
        assert!(s.fps_bit(5, 2));
        assert!(s.fps_bit(1, 3));
        assert!(!s.fps_bit(2, 3));
        assert!(!s.fps_bit(4, 5));
    }

    #[test]
    fn fps_bits_reach_transient_label() {
        // Label MAX_N + 1 = 16 should be addressable (transient only).
        let s = State::EMPTY.with_fps_set(16, 3);
        assert!(s.fps_bit(16, 3));
        assert!(s.fps_bit(3, 16));
    }

    #[test]
    fn canonicalize_permutes_fps() {
        // Slots carry labels 7, 3, 7, 5. After canonicalize: 1, 2, 1, 3.
        // So remap: 7 → 1, 3 → 2, 5 → 3.
        // Original FPS(7, 5) becomes FPS(1, 3).
        let s = State::EMPTY
            .with_slot(0, 7)
            .with_slot(1, 3)
            .with_slot(2, 7)
            .with_slot(3, 5)
            .with_fps_set(7, 5);
        let c = s.canonicalize();
        assert!(c.fps_bit(1, 3));
        assert!(!c.fps_bit(1, 2));
    }

    #[test]
    fn canonicalize_drops_fps_for_absent_labels() {
        // Label 9 not in any slot; its FPS bit gets dropped.
        let s = State::EMPTY
            .with_slot(0, 1)
            .with_slot(1, 2)
            .with_fps_set(1, 9);
        let c = s.canonicalize();
        assert!(!c.fps_bit(1, 2));
        // Whatever label 9 would have remapped to, it's not there.
        assert!(!c.fps_bit(1, 3));
    }

    #[test]
    fn merge_labels_unions_fps_rows() {
        // Pre-merge: FPS(3, 7)=1, FPS(3, 4)=1. Merge 3 → 2. After merge:
        // FPS(2, 7)=1, FPS(2, 4)=1, row 3 cleared.
        let s = State::EMPTY
            .with_slot(0, 2)
            .with_slot(1, 3)
            .with_slot(2, 2)
            .with_slot(3, 4)
            .with_fps_set(3, 7)
            .with_fps_set(3, 4);
        let m = s.merge_labels(3, 2);
        assert!(m.fps_bit(2, 7));
        assert!(m.fps_bit(2, 4));
        assert!(!m.fps_bit(3, 7));
        assert!(!m.fps_bit(3, 4));
    }

    #[test]
    fn with_fps_row_cleared_drops_all_entries() {
        let s = State::EMPTY
            .with_fps_set(3, 1)
            .with_fps_set(3, 5)
            .with_fps_set(3, 8)
            .with_fps_set(2, 4);
        let cleared = s.with_fps_row_cleared(3);
        assert!(!cleared.fps_bit(3, 1));
        assert!(!cleared.fps_bit(3, 5));
        assert!(!cleared.fps_bit(3, 8));
        // Unrelated pair survives.
        assert!(cleared.fps_bit(2, 4));
    }

    #[test]
    fn mirror_permutes_fps() {
        // (1, 2, 2) reversed canonicalizes to (1, 1, 2) — so remap 1 → 1,
        // 2 → …? Actually mirror(3) of (1,2,2): slot-reverse gives (2,2,1);
        // canonicalize first-seen over slots 0..2 yields: slot0=2→new 1,
        // slot1=2→1, slot2=1→new 2. So labels (1,1,2); remap 2→1, 1→2.
        // Pre-FPS(1, 2) → post-FPS(2, 1) which is the same pair.
        let s = State::EMPTY
            .with_slot(0, 1)
            .with_slot(1, 2)
            .with_slot(2, 2)
            .with_fps_set(1, 2);
        let m = s.mirror(3);
        assert!(m.fps_bit(1, 2));
    }

    #[test]
    fn mirror_fps_involution() {
        let s = State::EMPTY
            .with_slot(0, 1)
            .with_slot(1, 2)
            .with_slot(2, 3)
            .with_fps_set(1, 2)
            .with_fps_set(1, 3);
        let twice = s.mirror(3).mirror(3);
        assert_eq!(twice, s);
    }
}
