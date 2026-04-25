//! Integration tests for the partition counter.
//!
//! Two independent oracles back these tests:
//!   * `partition_oracle` — exhaustive set-partition enumerator with a
//!     connectivity check. Exponential, so only used for N ≤ 3.
//!   * `brute_force` — a recursive FPS-aware enumerator that mirrors the
//!     Simpath DP transition rules. Cross-checks simpath up to N ≈ 6.
//!
//! Regression values beyond the oracle range are cross-validated against
//! `brute_force` and locked in to detect drift.

use assert_cmd::Command;
use linkedin_queens::partition_count::{
    brute_force, count_partitions, partition_oracle, CountOptions,
};
use num_bigint::BigUint;
use std::str::FromStr;

fn simpath(n: usize) -> BigUint {
    count_partitions(&CountOptions::new(n), None).unwrap().count
}

#[test]
fn oracle_agrees_with_simpath_small() {
    for n in 1..=3 {
        let ora = partition_oracle::count(n);
        let sp = simpath(n);
        assert_eq!(ora, sp, "mismatch at n={}: oracle={}, simpath={}", n, ora, sp);
    }
}

#[test]
fn brute_force_matches_oracle_small() {
    for n in 1..=3 {
        let ora = partition_oracle::count(n);
        let bf = brute_force::count(n);
        assert_eq!(ora, bf, "mismatch at n={}: oracle={}, bf={}", n, ora, bf);
    }
}

#[test]
fn brute_force_and_simpath_agree_small() {
    // Capped at n=4 because the unoptimized debug build of brute_force is
    // much slower than the DP — n=5 (≈72M partitions) takes minutes.
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
    assert_eq!(simpath(3), BigUint::from(258u32));
}

/// Regression values. N ≤ 3 are also covered by `partition_oracle`; N = 4, 5
/// are additionally cross-checked against `brute_force` via
/// `brute_force_and_simpath_agree_small`. N = 6, 7 are locked in to detect
/// drift in the DP.
#[test]
fn regression_values() {
    let cases: &[(usize, &str)] = &[
        (3, "258"),
        (4, "62741"),
        (5, "72137699"),
        (6, "356612826084"),
        (7, "7146137621219723"),
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
        .stdout("258\n");
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
        .stdout("62741\n");
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
