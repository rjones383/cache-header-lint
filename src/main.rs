use std::env;
use std::fs::File;
use std::io::{self, Read, Write};
use std::process::ExitCode;

use cachelint::{lint, Records};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Format {
    Text,
    Json,
}

struct Args {
    path: Option<String>,
    format: Format,
}

fn parse_args<I: Iterator<Item = String>>(mut argv: I) -> Result<Args, String> {
    let mut path = None;
    let mut format = Format::Text;

    while let Some(arg) = argv.next() {
        if let Some(value) = arg.strip_prefix("--format=") {
            format = parse_format(value)?;
        } else if arg == "--format" {
            let value = argv
                .next()
                .ok_or_else(|| "--format requires a value (text or json)".to_string())?;
            format = parse_format(&value)?;
        } else if path.is_none() {
            path = Some(arg);
        } else {
            return Err(format!("unexpected argument: {arg}"));
        }
    }

    Ok(Args { path, format })
}

fn parse_format(value: &str) -> Result<Format, String> {
    match value {
        "text" => Ok(Format::Text),
        "json" => Ok(Format::Json),
        other => Err(format!("unknown format: {other} (expected text or json)")),
    }
}

fn main() -> ExitCode {
    let args = match parse_args(env::args().skip(1)) {
        Ok(args) => args,
        Err(e) => {
            eprintln!("cachelint: {e}");
            return ExitCode::from(2);
        }
    };

    let outcome = match &args.path {
        Some(path) => File::open(path).map(|f| run(f, args.format)),
        None => Ok(run(io::stdin(), args.format)),
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
fn run<R: Read>(input: R, format: Format) -> bool {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut saw_finding = false;
    let mut index = 0usize;

    for record in Records::new(input) {
        let record = match record {
            Ok(r) => r,
            Err(e) => {
                saw_finding = true;
                match format {
                    Format::Text => {
                        let _ = writeln!(out, "record {index}: read error: {e}");
                    }
                    Format::Json => {
                        let _ = writeln!(
                            out,
                            "{{\"record\":{index},\"error\":{}}}",
                            json_string(&e.to_string())
                        );
                    }
                }
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
        match format {
            Format::Text => {
                let _ = writeln!(out, "record {index} [{label}]");
                for finding in &findings {
                    let _ = writeln!(out, "  {}: {}", finding.severity.as_str(), finding.message);
                }
            }
            Format::Json => {
                let _ = write!(
                    out,
                    "{{\"record\":{index},\"status_line\":{},\"findings\":[",
                    json_string(label)
                );
                for (i, finding) in findings.iter().enumerate() {
                    if i > 0 {
                        let _ = write!(out, ",");
                    }
                    let _ = write!(
                        out,
                        "{{\"severity\":{},\"message\":{}}}",
                        json_string(finding.severity.as_str()),
                        json_string(&finding.message)
                    );
                }
                let _ = writeln!(out, "]}}");
            }
        }
    }

    saw_finding
}

/// Renders `s` as a double-quoted JSON string literal.
fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_string_escapes_quotes_and_backslashes() {
        assert_eq!(json_string("a\"b\\c"), "\"a\\\"b\\\\c\"");
    }

    #[test]
    fn json_string_escapes_control_characters() {
        assert_eq!(json_string("a\nb\tc"), "\"a\\nb\\tc\"");
        assert_eq!(json_string("\u{1}"), "\"\\u0001\"");
    }

    #[test]
    fn parse_args_accepts_format_and_path() {
        let args = parse_args(
            vec!["--format".to_string(), "json".to_string(), "headers.txt".to_string()]
                .into_iter(),
        )
        .unwrap();
        assert!(args.format == Format::Json);
        assert_eq!(args.path.as_deref(), Some("headers.txt"));
    }

    #[test]
    fn parse_args_accepts_format_equals_form() {
        let args = parse_args(vec!["--format=json".to_string()].into_iter()).unwrap();
        assert!(args.format == Format::Json);
        assert_eq!(args.path, None);
    }

    #[test]
    fn parse_args_rejects_unknown_format() {
        assert!(parse_args(vec!["--format".to_string(), "yaml".to_string()].into_iter()).is_err());
    }

    #[test]
    fn parse_args_rejects_second_positional() {
        assert!(parse_args(vec!["a.txt".to_string(), "b.txt".to_string()].into_iter()).is_err());
    }
}
