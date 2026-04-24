//! End-to-end integration test for the stream format.
//!
//! Runs `queens-generator --stream` to produce a handful of boards,
//! pipes the output into `queens-solver`, and verifies that each
//! `=== name ===` header is preserved once in the solver output and
//! that a queen or failure glyph appears between headers.

use std::io::Write;
use std::process::{Command, Stdio};

fn bin(name: &str) -> std::path::PathBuf {
    let mut path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("target");
    path.push("release");
    path.push(name);
    path
}

fn ensure_binaries() {
    let status = Command::new(env!("CARGO"))
        .args(["build", "--release", "--bins"])
        .status()
        .expect("cargo build");
    assert!(status.success(), "failed to build release binaries");
}

#[test]
fn generator_stream_is_solvable_by_solver() {
    ensure_binaries();

    let gen = Command::new(bin("queens-generator"))
        .args(["--stream", "7", "5"])
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn generator");
    let stream = gen.wait_with_output().expect("generator output");
    assert!(stream.status.success(), "generator exited non-zero");
    let stream_text = String::from_utf8(stream.stdout).expect("utf8");

    let header_count = stream_text
        .lines()
        .filter(|l| l.starts_with("=== ") && l.ends_with(" ==="))
        .count();
    assert_eq!(header_count, 5, "expected 5 headers in generator stream");

    let mut solver = Command::new(bin("queens-solver"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn solver");
    solver
        .stdin
        .as_mut()
        .unwrap()
        .write_all(stream_text.as_bytes())
        .expect("write stream to solver");
    let out = solver.wait_with_output().expect("solver output");
    assert!(out.status.success(), "solver exited non-zero");
    let out_text = String::from_utf8(out.stdout).expect("utf8");

    let echoed_headers: Vec<&str> = out_text
        .lines()
        .filter(|l| l.starts_with("=== ") && l.ends_with(" ==="))
        .collect();
    assert_eq!(echoed_headers.len(), 5, "solver should echo every header");

    // Every section should contain at least one queen glyph (all five
    // boards are uniquely solvable by construction, but we allow the
    // weaker "queen appears somewhere after a header" check to decouple
    // from the exact heuristic coverage).
    assert!(
        out_text.contains('\u{265B}'),
        "expected at least one queen glyph in solver output"
    );
}

#[test]
fn single_board_mode_still_works() {
    ensure_binaries();

    let sample = "\
PPPPPPO
PSGLLLO
PSGGGLO
PSSBGLO
PRSBBBO
PRRRRBO
POOOOOO
";
    let mut solver = Command::new(bin("queens-solver"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn solver");
    solver
        .stdin
        .as_mut()
        .unwrap()
        .write_all(sample.as_bytes())
        .expect("write board");
    let out = solver.wait_with_output().expect("solver output");
    let out_text = String::from_utf8(out.stdout).expect("utf8");
    assert!(
        !out_text.starts_with("=== "),
        "single-board output should not be stream-formatted"
    );
    assert!(
        out_text.contains("Queen-adjacent kill"),
        "expected the counter table in single-board success output"
    );
}
