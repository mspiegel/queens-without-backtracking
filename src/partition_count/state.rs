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
//! Bit layout of the packed `u128`:
//!   `[0 .. 80)`   — 16 slots × 5 bits. Slot `i` occupies bits `[5i, 5i+5)`.
//!                   Value 0 = empty; values 1..=N are polyomino labels.
//!   `[80 .. 85)`  — `closed_polyominoes` count (0..=N).
//!   `[85 .. 128)` — reserved.

pub const MAX_N: usize = 16;

const SLOT_BITS: u32 = 5;
const SLOT_MASK: u128 = (1u128 << SLOT_BITS) - 1;
const LABELS_BITS: u32 = MAX_N as u32 * SLOT_BITS;
const LABELS_MASK: u128 = (1u128 << LABELS_BITS) - 1;

const CLOSED_SHIFT: u32 = LABELS_BITS;
const CLOSED_BITS: u32 = 5;
const CLOSED_MASK: u128 = ((1u128 << CLOSED_BITS) - 1) << CLOSED_SHIFT;

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, PartialOrd, Ord)]
pub struct State(pub u128);

impl State {
    pub const EMPTY: State = State(0);

    #[inline]
    pub fn slot(self, i: usize) -> u8 {
        debug_assert!(i < MAX_N);
        ((self.0 >> (i as u32 * SLOT_BITS)) & SLOT_MASK) as u8
    }

    #[inline]
    pub fn with_slot(self, i: usize, value: u8) -> State {
        debug_assert!(i < MAX_N);
        debug_assert!((value as u128) <= SLOT_MASK);
        let shift = i as u32 * SLOT_BITS;
        let cleared = self.0 & !(SLOT_MASK << shift);
        State(cleared | ((value as u128) << shift))
    }

    #[inline]
    pub fn closed(self) -> u8 {
        ((self.0 & CLOSED_MASK) >> CLOSED_SHIFT) as u8
    }

    #[inline]
    pub fn with_closed(self, closed: u8) -> State {
        debug_assert!((closed as u32) < (1u32 << CLOSED_BITS));
        let cleared = self.0 & !CLOSED_MASK;
        State(cleared | ((closed as u128) << CLOSED_SHIFT))
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
    /// >= window_len are left as 0.
    ///
    /// Does not canonicalize or update `closed`; callers do that.
    pub fn shift_in(self, window_len: usize, new_label: u8) -> (State, u8, bool) {
        debug_assert!(window_len >= 1 && window_len <= MAX_N);
        let dropped = self.slot(0);
        let mut out = self.0 & !LABELS_MASK;
        // Shift existing slots [1 .. window_len) down to [0 .. window_len - 1).
        for i in 0..(window_len - 1) {
            let v = self.slot(i + 1);
            out |= (v as u128) << (i as u32 * SLOT_BITS);
        }
        // Place new cell at slot (window_len - 1).
        out |= (new_label as u128) << ((window_len as u32 - 1) * SLOT_BITS);
        let new_state = State(out);
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
    pub fn canonicalize(self) -> State {
        // Labels range over 1..=MAX_N+1 inclusive as a safety margin during merges.
        let mut remap = [0u8; (MAX_N + 2)];
        let mut next_label: u8 = 1;
        let mut out = self.0 & !LABELS_MASK;
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
        State(out)
    }

    /// Horizontally mirror the first `n` slots and return the canonicalized
    /// state: slot `i` swaps with slot `n − 1 − i`, then labels are
    /// restricted-growth renumbered. `closed` is preserved.
    ///
    /// Used at row boundaries in the counting DP: two states whose frontier
    /// labelings are mirror images cover the same number of completions, by
    /// the horizontal symmetry of the board.
    pub fn mirror(self, n: usize) -> State {
        debug_assert!(n >= 1 && n <= MAX_N);
        let mut out = self.0 & !LABELS_MASK;
        for i in 0..n {
            let v = self.slot(i);
            let mirror_pos = n - 1 - i;
            out |= (v as u128) << (mirror_pos as u32 * SLOT_BITS);
        }
        State(out).canonicalize()
    }

    /// Merge two labels in-place: every slot carrying `from` becomes `to`.
    /// Returns the merged (but not yet canonicalized) state.
    pub fn merge_labels(self, from: u8, to: u8) -> State {
        debug_assert!(from != 0 && to != 0 && from != to);
        let mut out = self.0 & !LABELS_MASK;
        for i in 0..MAX_N {
            let v = self.slot(i);
            let new_v = if v == from { to } else { v };
            out |= (new_v as u128) << (i as u32 * SLOT_BITS);
        }
        State(out)
    }
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
        // Labels 7, 3, 7, 5 should become 1, 2, 1, 3.
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
        assert!(closed); // label 1 no longer present.
        assert_eq!(new_state.slot(0), 2);
        assert_eq!(new_state.slot(1), 3);
        assert_eq!(new_state.slot(2), 4);
    }

    #[test]
    fn shift_in_reports_not_closed_when_label_still_present() {
        // Label 1 appears at slot 0 and slot 2, so dropping slot 0 keeps it alive.
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
        // (1, 2, 1) is already palindromic; mirror equals itself.
        let s = State::EMPTY
            .with_slot(0, 1)
            .with_slot(1, 2)
            .with_slot(2, 1);
        assert_eq!(s.mirror(3), s);
    }

    #[test]
    fn mirror_strictly_increasing_normalizes_to_self() {
        // (1, 2, 3) reversed is (3, 2, 1), which canonicalizes back to
        // (1, 2, 3) because the first-seen label becomes 1, etc.
        let s = State::EMPTY
            .with_slot(0, 1)
            .with_slot(1, 2)
            .with_slot(2, 3);
        assert_eq!(s.mirror(3), s);
    }

    #[test]
    fn mirror_distinct_example() {
        // (1, 2, 2) reversed is (2, 2, 1) which canonicalizes to (1, 1, 2).
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
        // For every state we form, mirror twice should return the original
        // (which is stored canonicalized).
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
        // With n = 3, slots 3..16 must stay 0.
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
}
