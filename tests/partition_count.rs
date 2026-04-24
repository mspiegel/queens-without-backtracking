//! Integration tests for the partition counter.
//!
//! The brute-force reference (exposed as
//! `linkedin_queens::partition_count::brute_force::count`) serves as an
//! independent oracle for small N. For larger N where brute force is too
//! slow, hard-coded regression values lock in the current implementation's
//! output; they are not independently sourced and exist to detect drift.

use assert_cmd::Command;
use linkedin_queens::partition_count::brute_force;
use linkedin_queens::partition_count::{count_partitions, CountOptions};
use num_bigint::BigUint;
use std::str::FromStr;

fn simpath(n: usize) -> BigUint {
    count_partitions(&CountOptions::new(n), None).unwrap().count
}

#[test]
fn brute_force_and_simpath_agree_small() {
    for n in 1..=4 {
        let bf = brute_force::count(n);
        let sp = simpath(n);
        assert_eq!(bf, sp, "mismatch at n={}: bf={}, sp={}", n, bf, sp);
    }
}

#[test]
fn hand_computed_values() {
    assert_eq!(simpath(1), BigUint::from(1u32));
    assert_eq!(simpath(2), BigUint::from(6u32));
}

/// Regression values from the first successful run of this implementation.
/// Not independently sourced — locked in to detect drift. Update only if an
/// intentional algorithmic change is made.
#[test]
fn regression_values() {
    let cases: &[(usize, &str)] = &[
        (3, "594"),
        (4, "682349"),
        (5, "8082227271"),
        (6, "937366494881708"),
        (7, "1032451012296991972867"),
    ];
    for (n, expected) in cases {
        let got = simpath(*n);
        let want = BigUint::from_str(expected).unwrap();
        assert_eq!(got, want, "regression failure at n={}", n);
    }
}

#[test]
fn cli_prints_count_for_n3() {
    Command::cargo_bin("count-partitions")
        .unwrap()
        .arg("3")
        .arg("--progress-interval")
        .arg("0")
        .assert()
        .success()
        .stdout("594\n");
}

#[test]
fn cli_prints_count_for_n4() {
    Command::cargo_bin("count-partitions")
        .unwrap()
        .arg("4")
        .arg("--progress-interval")
        .arg("0")
        .assert()
        .success()
        .stdout("682349\n");
}

#[test]
fn cli_accepts_memory_and_progress_flags() {
    // Smoke test: flags parse, the run completes, and stdout carries the
    // exact decimal count. Triggering a real memory abort requires a very
    // small byte-level limit that the CLI does not expose.
    Command::cargo_bin("count-partitions")
        .unwrap()
        .arg("2")
        .arg("--max-memory-gb")
        .arg("0")
        .arg("--progress-interval")
        .arg("0")
        .assert()
        .success()
        .stdout("6\n");
}
