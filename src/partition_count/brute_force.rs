//! Independent reference enumerator used only in tests.
//!
//! Grows a tiling cell-by-cell in row-major order. At each cell, the cell's
//! polyomino label is either the same as one of the already-placed neighbors
//! (up or left) — possibly merging two polyominoes if both neighbors carry
//! different labels — or a newly started label. At the end we accept
//! tilings with exactly N polyominoes.
//!
//! Canonical-partition counting (Graphillion-style). At each cell X with
//! up-neighbor U and left-neighbor L we simultaneously commit the two grid
//! edges (X,U) and (X,L). Each edge is either **taken** (X joins that
//! neighbor's component) or **not-taken** (X is in a different component
//! forever from that neighbor's). "Not taken" is recorded in a
//! **Forbidden-Pair Set (FPS)** indexed by component label: any future
//! attempt to merge the two components is rejected. This gives a bijection
//! between partitions and edge-decision sequences (take every within-block
//! edge, don't-take every between-block edge), so each partition is counted
//! exactly once.
//!
//! After every transition that disturbs labels (merge / new component) the
//! labels array and FPS are canonicalized to first-seen order so that label
//! indices stay dense 1..=num_labels.

use num_bigint::BigUint;

/// Max number of distinct component labels the FPS bitmask supports. 64 is
/// enough for N ≤ 7 (at most N² ≤ 49 labels at any time).
const MAX_LABELS: usize = 64;

/// Count tilings of an N×N board by exactly N fixed polyominoes.
pub fn count(n: usize) -> BigUint {
    assert!(n >= 1);
    let total = n * n;
    let mut labels = vec![0u8; total];
    let mut fps = [0u64; MAX_LABELS];
    let mut num_labels: u8 = 0;
    let mut acc = BigUint::from(0u32);
    recurse(n, 0, &mut labels, &mut fps, &mut num_labels, &mut acc);
    acc
}

#[derive(Clone)]
struct Snapshot {
    labels_prefix: Vec<u8>,
    fps: [u64; MAX_LABELS],
    num_labels: u8,
}

fn snapshot(labels: &[u8], k: usize, fps: &[u64; MAX_LABELS], num_labels: u8) -> Snapshot {
    Snapshot {
        labels_prefix: labels[..=k].to_vec(),
        fps: *fps,
        num_labels,
    }
}

fn restore(
    labels: &mut [u8],
    snap: &Snapshot,
    fps: &mut [u64; MAX_LABELS],
    num_labels: &mut u8,
) {
    let k_plus_1 = snap.labels_prefix.len();
    labels[..k_plus_1].copy_from_slice(&snap.labels_prefix);
    if k_plus_1 < labels.len() {
        labels[k_plus_1] = 0;
    }
    *fps = snap.fps;
    *num_labels = snap.num_labels;
}

fn recurse(
    n: usize,
    k: usize,
    labels: &mut [u8],
    fps: &mut [u64; MAX_LABELS],
    num_labels: &mut u8,
    acc: &mut BigUint,
) {
    let total = n * n;
    if k == total {
        if *num_labels as usize == n as usize {
            *acc += 1u32;
        }
        return;
    }
    let r = k / n;
    let c = k % n;
    let up_label = if r > 0 { labels[k - n] } else { 0 };
    let left_label = if c > 0 { labels[k - 1] } else { 0 };

    let cells_remaining_after = total - k - 1;

    // Reachability prune: the final label count must equal n, and each
    // remaining cell can swing the total by at most 1, so
    // |new_total − n| ≤ cells_remaining.
    let prune = |new_total: u8| -> bool {
        let gap = (new_total as isize - n as isize).unsigned_abs();
        gap <= cells_remaining_after
    };

    // Enumerate the 1-2-4 edge-decision combos depending on which neighbors
    // are placed.
    match (up_label, left_label) {
        (0, 0) => {
            // No placed neighbors → X must start a new component.
            try_new_component(
                n, k, labels, fps, num_labels, acc, &[], &prune,
            );
        }
        (u, 0) | (0, u) if u != 0 => {
            // One placed neighbor → join it, or start a new component (with
            // FPS(new, u) to forbid a later merge).
            try_join(n, k, labels, fps, num_labels, acc, u, &prune);
            try_new_component(n, k, labels, fps, num_labels, acc, &[u], &prune);
        }
        (u, l) if u == l => {
            // Two placed neighbors in the same component → join (take both
            // edges) or start a new component (don't-take both, FPS(new, u)).
            try_join(n, k, labels, fps, num_labels, acc, u, &prune);
            try_new_component(n, k, labels, fps, num_labels, acc, &[u], &prune);
        }
        (u, l) => {
            // Two placed neighbors in distinct components. Four edge-decision
            // combos:
            //   (T,T): merge u and l into X's component (blocked if FPS(u,l)).
            //   (T,D): X joins u; FPS(u, l) set.
            //   (D,T): X joins l; FPS(l, u) set (same bit).
            //   (D,D): X starts new; FPS(new, u) and FPS(new, l) set.
            try_merge(n, k, labels, fps, num_labels, acc, u, l, &prune);
            try_join_with_forbid(n, k, labels, fps, num_labels, acc, u, l, &prune);
            try_join_with_forbid(n, k, labels, fps, num_labels, acc, l, u, &prune);
            try_new_component(n, k, labels, fps, num_labels, acc, &[u, l], &prune);
        }
    }
}

fn fps_get(fps: &[u64; MAX_LABELS], a: u8, b: u8) -> bool {
    debug_assert!(a != 0 && b != 0 && a != b);
    (fps[a as usize] >> (b as u32)) & 1 == 1
}

fn fps_set(fps: &mut [u64; MAX_LABELS], a: u8, b: u8) {
    debug_assert!(a != 0 && b != 0 && a != b);
    fps[a as usize] |= 1u64 << (b as u32);
    fps[b as usize] |= 1u64 << (a as u32);
}

fn try_join(
    n: usize,
    k: usize,
    labels: &mut [u8],
    fps: &mut [u64; MAX_LABELS],
    num_labels: &mut u8,
    acc: &mut BigUint,
    join_label: u8,
    prune: &impl Fn(u8) -> bool,
) {
    if !prune(*num_labels) {
        return;
    }
    let snap = snapshot(labels, k, fps, *num_labels);
    labels[k] = join_label;
    recurse(n, k + 1, labels, fps, num_labels, acc);
    restore(labels, &snap, fps, num_labels);
}

fn try_new_component(
    n: usize,
    k: usize,
    labels: &mut [u8],
    fps: &mut [u64; MAX_LABELS],
    num_labels: &mut u8,
    acc: &mut BigUint,
    forbid_with: &[u8],
    prune: &impl Fn(u8) -> bool,
) {
    let new_total = *num_labels + 1;
    if !prune(new_total) {
        return;
    }
    if (new_total as usize) >= MAX_LABELS {
        return;
    }
    let snap = snapshot(labels, k, fps, *num_labels);
    let new_label = new_total;
    labels[k] = new_label;
    *num_labels = new_total;
    for &other in forbid_with {
        fps_set(fps, new_label, other);
    }
    recurse(n, k + 1, labels, fps, num_labels, acc);
    restore(labels, &snap, fps, num_labels);
}

fn try_join_with_forbid(
    n: usize,
    k: usize,
    labels: &mut [u8],
    fps: &mut [u64; MAX_LABELS],
    num_labels: &mut u8,
    acc: &mut BigUint,
    join_label: u8,
    forbid_label: u8,
    prune: &impl Fn(u8) -> bool,
) {
    if !prune(*num_labels) {
        return;
    }
    let snap = snapshot(labels, k, fps, *num_labels);
    labels[k] = join_label;
    fps_set(fps, join_label, forbid_label);
    recurse(n, k + 1, labels, fps, num_labels, acc);
    restore(labels, &snap, fps, num_labels);
}

fn try_merge(
    n: usize,
    k: usize,
    labels: &mut [u8],
    fps: &mut [u64; MAX_LABELS],
    num_labels: &mut u8,
    acc: &mut BigUint,
    u: u8,
    l: u8,
    prune: &impl Fn(u8) -> bool,
) {
    debug_assert!(u != l);
    if fps_get(fps, u, l) {
        return;
    }
    let new_total = *num_labels - 1;
    if !prune(new_total) {
        return;
    }
    let snap = snapshot(labels, k, fps, *num_labels);

    let keep = u.min(l);
    let drop_ = u.max(l);

    // Rename every `drop_` to `keep` in labels[..=k-1]. labels[k] itself gets
    // `keep` below.
    for v in labels[..k].iter_mut() {
        if *v == drop_ {
            *v = keep;
        }
    }
    labels[k] = keep;

    // Union fps rows: fps[keep] |= fps[drop_], then clear fps[drop_]. Also
    // clear the now-stale fps(keep, drop_) bit.
    let drop_row = fps[drop_ as usize];
    fps[keep as usize] |= drop_row;
    // Propagate symmetric updates: any label j with fps(j, drop_)=1 now
    // needs fps(j, keep)=1 and fps(j, drop_)=0.
    for j in 1..MAX_LABELS {
        if j as u8 == keep || j as u8 == drop_ {
            continue;
        }
        if (drop_row >> j) & 1 == 1 {
            fps[j] |= 1u64 << keep as u32;
            fps[j] &= !(1u64 << drop_ as u32);
        }
    }
    fps[drop_ as usize] = 0;
    // Clear the now-stale self-pair fps(keep, drop_).
    fps[keep as usize] &= !(1u64 << drop_ as u32);

    *num_labels -= 1;

    canonicalize(labels, k, fps, num_labels);

    recurse(n, k + 1, labels, fps, num_labels, acc);
    restore(labels, &snap, fps, num_labels);
}

/// Renumber labels in `labels[..=k]` into first-seen order, and permute FPS
/// rows/cols to match. Also updates `*num_labels` to the number of distinct
/// labels observed.
fn canonicalize(labels: &mut [u8], k: usize, fps: &mut [u64; MAX_LABELS], num_labels: &mut u8) {
    let mut remap = [0u8; MAX_LABELS];
    let mut next: u8 = 1;
    for i in 0..=k {
        let v = labels[i];
        if v != 0 && remap[v as usize] == 0 {
            remap[v as usize] = next;
            next += 1;
        }
    }
    for v in labels[..=k].iter_mut() {
        if *v != 0 {
            *v = remap[*v as usize];
        }
    }

    let mut new_fps = [0u64; MAX_LABELS];
    for a in 1..MAX_LABELS {
        let new_a = remap[a] as usize;
        if new_a == 0 {
            continue;
        }
        let bits = fps[a];
        let mut remaining = bits;
        while remaining != 0 {
            let b = remaining.trailing_zeros() as usize;
            remaining &= remaining - 1;
            if b < MAX_LABELS && remap[b] != 0 {
                new_fps[new_a] |= 1u64 << remap[b] as u32;
            }
        }
    }
    *fps = new_fps;
    *num_labels = next - 1;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn n1_is_one() {
        assert_eq!(count(1), BigUint::from(1u32));
    }

    #[test]
    fn n2_is_six() {
        assert_eq!(count(2), BigUint::from(6u32));
    }

    #[test]
    fn n3_is_258() {
        assert_eq!(count(3), BigUint::from(258u32));
    }

#[test]
    fn canonicalize_basic() {
        let mut labels = [0u8; 9];
        labels[0] = 3;
        labels[1] = 1;
        labels[2] = 3;
        labels[3] = 2;
        let mut fps = [0u64; MAX_LABELS];
        fps_set(&mut fps, 1, 3);
        let mut num_labels: u8 = 3;
        canonicalize(&mut labels, 3, &mut fps, &mut num_labels);
        assert_eq!(labels[0], 1);
        assert_eq!(labels[1], 2);
        assert_eq!(labels[2], 1);
        assert_eq!(labels[3], 3);
        assert_eq!(num_labels, 3);
        // Old FPS(1, 3) means original labels 1 and 3. After remap:
        // original 3 → new 1, original 1 → new 2. So new FPS is between
        // labels 1 and 2.
        assert!(fps_get(&fps, 1, 2));
        assert!(!fps_get(&fps, 1, 3));
    }
}
