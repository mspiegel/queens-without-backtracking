use std::io::{Read, Write};
use std::process::ExitCode;

use linkedin_queens::{board, counters::Counters, output, rules, state::State};

fn main() -> ExitCode {
    let mut input = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut input) {
        eprintln!("failed to read stdin: {e}");
        return ExitCode::from(2);
    }

    let parsed = match board::parse(&input) {
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
