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
    paths: Vec<String>,
    format: Format,
}

fn parse_args<I: Iterator<Item = String>>(mut argv: I) -> Result<Args, String> {
    let mut paths = Vec::new();
    let mut format = Format::Text;

    while let Some(arg) = argv.next() {
        if let Some(value) = arg.strip_prefix("--format=") {
            format = parse_format(value)?;
        } else if arg == "--format" {
            let value = argv
                .next()
                .ok_or_else(|| "--format requires a value (text or json)".to_string())?;
            format = parse_format(&value)?;
        } else {
            paths.push(arg);
        }
    }

    Ok(Args { paths, format })
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

    if args.paths.is_empty() {
        return match run(io::stdin(), args.format, None) {
            true => ExitCode::from(1),
            false => ExitCode::SUCCESS,
        };
    }

    // With one file there's nothing to disambiguate, so leave the output
    // matching the stdin case; with several, prefix each line with its
    // source so findings from a batch of captures don't get mixed up.
    let label_paths = args.paths.len() > 1;
    let mut saw_finding = false;
    for path in &args.paths {
        let file = match File::open(path) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("cachelint: {path}: {e}");
                return ExitCode::from(2);
            }
        };
        let label = if label_paths { Some(path.as_str()) } else { None };
        if run(file, args.format, label) {
            saw_finding = true;
        }
    }

    if saw_finding {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// Lints every record in `input`, printing findings as they're found.
/// `source` is included in the output when linting one of several files.
/// Returns true if anything was flagged.
fn run<R: Read>(input: R, format: Format, source: Option<&str>) -> bool {
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
                    Format::Text => match source {
                        Some(path) => {
                            let _ = writeln!(out, "{path}: record {index}: read error: {e}");
                        }
                        None => {
                            let _ = writeln!(out, "record {index}: read error: {e}");
                        }
                    },
                    Format::Json => {
                        let _ = writeln!(
                            out,
                            "{{{}\"record\":{index},\"error\":{}}}",
                            file_field(source),
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
        let status = record.status_line.as_deref().unwrap_or("(no status line)");
        match format {
            Format::Text => {
                match source {
                    Some(path) => {
                        let _ = writeln!(out, "{path}: record {index} [{status}]");
                    }
                    None => {
                        let _ = writeln!(out, "record {index} [{status}]");
                    }
                }
                for finding in &findings {
                    let _ = writeln!(out, "  {}: {}", finding.severity.as_str(), finding.message);
                }
            }
            Format::Json => {
                let _ = write!(
                    out,
                    "{{{}\"record\":{index},\"status_line\":{},\"findings\":[",
                    file_field(source),
                    json_string(status)
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

/// Renders a leading `"file":"...",` object field for JSON output, or
/// nothing when there's no source to disambiguate.
fn file_field(source: Option<&str>) -> String {
    match source {
        Some(path) => format!("\"file\":{},", json_string(path)),
        None => String::new(),
    }
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
        assert_eq!(args.paths, vec!["headers.txt".to_string()]);
    }

    #[test]
    fn parse_args_accepts_format_equals_form() {
        let args = parse_args(vec!["--format=json".to_string()].into_iter()).unwrap();
        assert!(args.format == Format::Json);
        assert!(args.paths.is_empty());
    }

    #[test]
    fn parse_args_rejects_unknown_format() {
        assert!(parse_args(vec!["--format".to_string(), "yaml".to_string()].into_iter()).is_err());
    }

    #[test]
    fn parse_args_accepts_multiple_paths() {
        let args = parse_args(
            vec!["a.txt".to_string(), "b.txt".to_string(), "c.txt".to_string()].into_iter(),
        )
        .unwrap();
        assert_eq!(args.paths, vec!["a.txt", "b.txt", "c.txt"]);
    }

    #[test]
    fn parse_args_keeps_paths_in_order_regardless_of_flag_position() {
        let args = parse_args(
            vec![
                "a.txt".to_string(),
                "--format".to_string(),
                "json".to_string(),
                "b.txt".to_string(),
            ]
            .into_iter(),
        )
        .unwrap();
        assert_eq!(args.paths, vec!["a.txt", "b.txt"]);
        assert!(args.format == Format::Json);
    }
}
