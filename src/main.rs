use std::env;
use std::fs::File;
use std::io::{self, Read, Write};
use std::process::ExitCode;

use cachelint::{lint, Records};

fn main() -> ExitCode {
    let path = env::args().nth(1);

    let outcome = match path {
        Some(path) => File::open(&path).map(run),
        None => Ok(run(io::stdin())),
    };

    match outcome {
        Ok(true) => ExitCode::from(1),
        Ok(false) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("cachelint: {e}");
            ExitCode::from(2)
        }
    }
}

/// Lints every record in `input`, printing findings as they're found.
/// Returns true if anything was flagged.
fn run<R: Read>(input: R) -> bool {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut saw_finding = false;
    let mut index = 0usize;

    for record in Records::new(input) {
        let record = match record {
            Ok(r) => r,
            Err(e) => {
                let _ = writeln!(out, "record {index}: read error: {e}");
                saw_finding = true;
                break;
            }
        };

        index += 1;
        let findings = lint(&record);
        if findings.is_empty() {
            continue;
        }

        saw_finding = true;
        let label = record.status_line.as_deref().unwrap_or("(no status line)");
        let _ = writeln!(out, "record {index} [{label}]");
        for finding in &findings {
            let _ = writeln!(out, "  {}: {}", finding.severity.as_str(), finding.message);
        }
    }

    saw_finding
}
