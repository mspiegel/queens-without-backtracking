//! `queens-solver` binary. Two input modes, auto-detected from stdin:
//!
//! * **Single-board mode** (default): one board on stdin, one result
//!   on stdout. Exit `0` if solved, `1` if the heuristics stalled at a
//!   partial state, `2` on I/O or parse error.
//! * **Stream mode** (triggered when the first non-blank input line is
//!   `=== some_name ===`): zero or more sections separated by blank
//!   lines, each section starting with a `=== name ===` header. For
//!   every section the header is echoed, the existing success or
//!   failure text is written, and a trailing blank line separates it
//!   from the next. Exit `0` on stream completion, `2` on I/O error;
//!   per-section status is visible in the emitted text.

use std::io::{Read, Write};
use std::process::ExitCode;

use linkedin_queens::{board, counters::Counters, output, rules, state::State};

fn main() -> ExitCode {
    let mut input = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut input) {
        eprintln!("failed to read stdin: {e}");
        return ExitCode::from(2);
    }

    if is_stream_input(&input) {
        run_stream(&input)
    } else {
        run_single(&input)
    }
}

fn is_stream_input(input: &str) -> bool {
    input
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(is_header_line)
        .unwrap_or(false)
}

fn is_header_line(line: &str) -> bool {
    let trimmed = line.trim_end();
    trimmed.starts_with("=== ") && trimmed.ends_with(" ===") && trimmed.len() > 8
}

fn run_single(input: &str) -> ExitCode {
    let parsed = match board::parse(input) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("failed to parse board: {e}");
            return ExitCode::from(2);
        }
    };

    let mut state = State::new(&parsed);
    let mut counters = Counters::new();
    rules::run_to_fixed_point(&mut state, &mut counters);

    let stdout = std::io::stdout();
    let mut handle = stdout.lock();

    if state.queen_list.len() == parsed.n {
        let text = output::format_success(&state, &counters);
        let _ = handle.write_all(text.as_bytes());
        ExitCode::SUCCESS
    } else {
        let text = output::format_failure(&state);
        let _ = handle.write_all(text.as_bytes());
        ExitCode::from(1)
    }
}

fn run_stream(input: &str) -> ExitCode {
    let stdout = std::io::stdout();
    let mut out = std::io::BufWriter::new(stdout.lock());

    let mut header: Option<String> = None;
    let mut body: String = String::new();

    let flush = |header: &Option<String>,
                 body: &str,
                 out: &mut std::io::BufWriter<std::io::StdoutLock>|
     -> std::io::Result<()> {
        let Some(name) = header else {
            return Ok(());
        };
        writeln!(out, "=== {name} ===")?;
        if body.trim().is_empty() {
            writeln!(out, "ERROR: empty board section")?;
            writeln!(out)?;
            return Ok(());
        }
        match board::parse(body) {
            Ok(parsed) => {
                let mut state = State::new(&parsed);
                let mut counters = Counters::new();
                rules::run_to_fixed_point(&mut state, &mut counters);
                let text = if state.queen_list.len() == parsed.n {
                    output::format_success(&state, &counters)
                } else {
                    output::format_failure(&state)
                };
                out.write_all(text.as_bytes())?;
                if !text.ends_with('\n') {
                    writeln!(out)?;
                }
                writeln!(out)?;
            }
            Err(e) => {
                writeln!(out, "ERROR: {e}")?;
                writeln!(out)?;
            }
        }
        Ok(())
    };

    for line in input.lines() {
        if is_header_line(line) {
            if let Err(e) = flush(&header, &body, &mut out) {
                eprintln!("stdout write failed: {e}");
                return ExitCode::from(2);
            }
            let trimmed = line.trim_end();
            let name = trimmed[4..trimmed.len() - 4].to_string();
            header = Some(name);
            body.clear();
        } else {
            body.push_str(line);
            body.push('\n');
        }
    }
    if let Err(e) = flush(&header, &body, &mut out) {
        eprintln!("stdout write failed: {e}");
        return ExitCode::from(2);
    }
    if let Err(e) = out.flush() {
        eprintln!("stdout flush failed: {e}");
        return ExitCode::from(2);
    }
    ExitCode::SUCCESS
}
