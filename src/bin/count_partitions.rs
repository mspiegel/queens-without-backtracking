//! `count-partitions <N>` — count tilings of an N×N board by exactly N fixed
//! polyominoes. Final count prints to stdout as a decimal. Progress and
//! diagnostics go to stderr.

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use clap::Parser;
use linkedin_queens::partition_count::{count_partitions, CountError, CountOptions};
use std::io::{self, Write};
use std::process::ExitCode;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(
    name = "count-partitions",
    about = "Count tilings of an N×N board by exactly N fixed polyominoes"
)]
struct Args {
    /// Board side length N (1..=16).
    n: usize,

    /// Seconds between progress updates on stderr. 0 disables progress.
    #[arg(long, default_value_t = 2.0)]
    progress_interval: f64,

    /// Abort if peak RSS exceeds this many GB. 0 disables the check.
    #[arg(long, default_value_t = 30)]
    max_memory_gb: u64,
}

fn main() -> ExitCode {
    let args = Args::parse();

    let opts = CountOptions {
        n: args.n,
        max_memory_bytes: args.max_memory_gb.saturating_mul(1024 * 1024 * 1024),
        progress_interval: if args.progress_interval <= 0.0 {
            Duration::ZERO
        } else {
            Duration::from_secs_f64(args.progress_interval)
        },
    };

    let mut stderr = io::stderr();
    let progress: Option<&mut dyn Write> = if opts.progress_interval.is_zero() {
        None
    } else {
        Some(&mut stderr)
    };

    match count_partitions(&opts, progress) {
        Ok(report) => {
            println!("{}", report.count);
            ExitCode::SUCCESS
        }
        Err(CountError::MemoryLimitExceeded {
            peak_rss_bytes,
            limit_bytes,
            layer,
        }) => {
            eprintln!(
                "aborted at layer {}: peak RSS {} B exceeded limit {} B",
                layer, peak_rss_bytes, limit_bytes
            );
            ExitCode::from(2)
        }
        Err(CountError::Io(e)) => {
            eprintln!("io error: {}", e);
            ExitCode::from(3)
        }
    }
}
