//! Independent reference enumerator used only in tests.
//!
//! Grows a tiling cell-by-cell in row-major order. At each cell, the cell's
//! polyomino label is either the same as one of the already-placed neighbors
//! (up or left) — possibly merging two polyominoes if both neighbors carry
//! different labels — or a newly started label. At the end we accept
//! tilings with exactly N polyominoes.
//!
//! Does no canonicalization across states; recursion simply explores every
//! legal assignment of polyomino labels and counts leaves.

use num_bigint::BigUint;

/// Count tilings of an N×N board by exactly N fixed polyominoes.
pub fn count(n: usize) -> BigUint {
    assert!(n >= 1);
    let total = n * n;
    let mut labels = vec![0u8; total];
    // Track a union-find-free representative per label by storing whether a
    // label is still active (present in any placed cell whose polyomino could
    // still be extended). A polyomino "closes" implicitly when the scan
    // passes all its cells; we only need the final total count of distinct
    // labels, which equals the number of polyominoes.
    let mut total_polys: u8 = 0;
    let mut acc = BigUint::from(0u32);
    recurse(n, 0, &mut labels, &mut total_polys, &mut acc);
    acc
}

fn recurse(
    n: usize,
    k: usize,
    labels: &mut [u8],
    total_polys: &mut u8,
    acc: &mut BigUint,
) {
    let total = n * n;
    if k == total {
        if *total_polys == n as u8 {
            *acc += BigUint::from(1u32);
        }
        return;
    }
    let r = k / n;
    let c = k % n;
    let up_label = if r > 0 { labels[k - n] } else { 0 };
    let left_label = if c > 0 { labels[k - 1] } else { 0 };

    let cells_remaining_after = total - k - 1;

    // Try: new polyomino. Ungated on `total_polys < n`: a fresh label may be
    // needed even when total == n, because a later cell can merge two labels.
    // Reachability prune: the final total must equal n, and each remaining
    // cell can swing the total by ±1, so `|new_total − n| ≤ cells_remaining`.
    {
        let new_total = (*total_polys as isize) + 1;
        let gap = (new_total - n as isize).unsigned_abs();
        if gap <= cells_remaining_after {
            let my_label = *total_polys + 1;
            labels[k] = my_label;
            *total_polys += 1;
            recurse(n, k + 1, labels, total_polys, acc);
            *total_polys -= 1;
            labels[k] = 0;
        }
    }

    // Try: join up's label.
    if up_label != 0 {
        labels[k] = up_label;
        recurse(n, k + 1, labels, total_polys, acc);
        labels[k] = 0;
    }

    // Try: join left's label (if different from up's).
    if left_label != 0 && left_label != up_label {
        labels[k] = left_label;
        recurse(n, k + 1, labels, total_polys, acc);
        labels[k] = 0;
    }

    // Try: merge up and left (if both present and distinct).
    if up_label != 0 && left_label != 0 && up_label != left_label {
        let keep = up_label.min(left_label);
        let drop_ = up_label.max(left_label);
        // Rename every occurrence of `drop_` to `keep` in cells placed so
        // far. The highest label `*total_polys` shifts down by 1 if
        // `drop_ == *total_polys`; otherwise we need to rename the highest
        // label into the freed slot so the invariant "labels are 1..=total"
        // stays true.
        let old_labels: Vec<u8> = labels[..k].to_vec();
        for v in labels[..k].iter_mut() {
            if *v == drop_ {
                *v = keep;
            }
        }
        // Compact: if drop_ was not the highest active label, rename the
        // current highest (*total_polys) → drop_, so labels remain 1..=(total-1).
        let old_top = *total_polys;
        if drop_ != old_top {
            for v in labels[..k].iter_mut() {
                if *v == old_top {
                    *v = drop_;
                }
            }
        }
        *total_polys -= 1;
        labels[k] = keep;
        recurse(n, k + 1, labels, total_polys, acc);
        labels[k] = 0;
        *total_polys += 1;
        // Restore original labels.
        labels[..k].copy_from_slice(&old_labels);
    }
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
}

