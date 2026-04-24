//! `queens-generator` binary: produces random N×N boards with exactly
//! one valid solution. Two output modes:
//!
//! * `queens-generator <N> <output_dir> <count>` — one file per board,
//!   matching the archive file format.
//! * `queens-generator --stream <N> <count>` — writes every board to
//!   stdout as a single stream. Each board is preceded by a
//!   `=== NNNNNN.txt ===` header and followed by a blank separator
//!   line, so the output pairs cleanly with `queens-solver` reading
//!   the same stream format from stdin.

use std::fs;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use rand::rngs::StdRng;
use rand::SeedableRng;

use linkedin_queens::generator;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let prog = args
        .first()
        .map(String::as_str)
        .unwrap_or("queens-generator");

    let stream_mode = args.get(1).map(|s| s == "--stream").unwrap_or(false);
    let expected_len = if stream_mode { 4 } else { 4 };

    if args.len() != expected_len {
        if stream_mode {
            eprintln!("usage: {prog} --stream <N> <count>");
        } else {
            eprintln!(
                "usage: {prog} <N> <output_dir> <count>\n       {prog} --stream <N> <count>"
            );
        }
        return ExitCode::from(2);
    }

    let n_arg_idx = if stream_mode { 2 } else { 1 };
    let count_arg_idx = if stream_mode { 3 } else { 3 };

    let n: usize = match args[n_arg_idx].parse() {
        Ok(v) => v,
        Err(_) => {
            eprintln!("N must be a non-negative integer");
            return ExitCode::from(2);
        }
    };
    if n > generator::MAX_N {
        eprintln!(
            "N={n} exceeds the maximum supported size of {}",
            generator::MAX_N
        );
        return ExitCode::from(2);
    }

    let count: usize = match args[count_arg_idx].parse() {
        Ok(v) => v,
        Err(_) => {
            eprintln!("count must be a non-negative integer");
            return ExitCode::from(2);
        }
    };

    let mut rng = StdRng::from_entropy();
    let width = count.to_string().len().max(1);

    if stream_mode {
        let stdout = io::stdout();
        let mut out = BufWriter::new(stdout.lock());
        for i in 1..=count {
            let regions = match generator::generate_board(n, &mut rng) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("generation failed: {e}");
                    return ExitCode::from(1);
                }
            };
            let name = format!("{:0width$}.txt", i, width = width);
            if let Err(e) = writeln!(out, "=== {name} ===") {
                eprintln!("stdout write failed: {e}");
                return ExitCode::from(2);
            }
            let text = generator::format_board(&regions);
            if let Err(e) = out.write_all(text.as_bytes()).and_then(|_| writeln!(out)) {
                eprintln!("stdout write failed: {e}");
                return ExitCode::from(2);
            }
        }
        if let Err(e) = out.flush() {
            eprintln!("stdout flush failed: {e}");
            return ExitCode::from(2);
        }
        return ExitCode::SUCCESS;
    }

    let out_dir = PathBuf::from(&args[2]);
    if let Err(e) = fs::create_dir_all(&out_dir) {
        eprintln!("failed to create {}: {e}", out_dir.display());
        return ExitCode::from(2);
    }
    for i in 1..=count {
        let regions = match generator::generate_board(n, &mut rng) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("generation failed: {e}");
                return ExitCode::from(1);
            }
        };
        let text = generator::format_board(&regions);
        let filename = format!("{:0width$}.txt", i, width = width);
        let path = out_dir.join(&filename);
        if let Err(e) = fs::write(&path, text.as_bytes()) {
            eprintln!("failed to write {}: {e}", path.display());
            return ExitCode::from(2);
        }
    }
    ExitCode::SUCCESS
}
