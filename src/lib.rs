//! Parsing and linting for captured HTTP response headers.
//!
//! The intended input is a dump of response headers, such as what
//! `curl -D -` writes: a status line followed by header lines, with
//! records separated by a blank line. [`Records`] turns a stream of
//! that into one [`HeaderRecord`] at a time; [`lint`] checks a single
//! record for common `Cache-Control` mistakes.

use std::io::{self, BufRead};

pub mod cache_control;

pub use cache_control::{CacheControl, Directive};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

impl Severity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Finding {
    pub severity: Severity,
    pub message: String,
}

impl Finding {
    fn new(severity: Severity, message: impl Into<String>) -> Self {
        Finding {
            severity,
            message: message.into(),
        }
    }
}

/// One response's worth of headers, in the order they appeared.
#[derive(Debug, Clone, Default)]
pub struct HeaderRecord {
    pub status_line: Option<String>,
    pub headers: Vec<(String, String)>,
}

impl HeaderRecord {
    pub fn get(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    fn is_empty(&self) -> bool {
        self.status_line.is_none() && self.headers.is_empty()
    }
}

/// Reads header records out of a stream, one at a time.
///
/// Records are separated by a blank line. Each call to `next` reads
/// only as far as the next blank line (or end of input) and holds
/// just that one record in memory, so a multi-gigabyte capture file
/// can be linted with a small, constant memory footprint.
pub struct Records<R> {
    reader: io::BufReader<R>,
    line: String,
    done: bool,
}

impl<R: io::Read> Records<R> {
    pub fn new(inner: R) -> Self {
        Records {
            reader: io::BufReader::new(inner),
            line: String::new(),
            done: false,
        }
    }
}

impl<R: io::Read> Iterator for Records<R> {
    type Item = io::Result<HeaderRecord>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        let mut record = HeaderRecord::default();

        loop {
            self.line.clear();
            let bytes_read = match self.reader.read_line(&mut self.line) {
                Ok(n) => n,
                Err(e) => return Some(Err(e)),
            };

            if bytes_read == 0 {
                self.done = true;
                break;
            }

            let trimmed = self.line.trim_end_matches(['\r', '\n']);

            if trimmed.is_empty() {
                if record.is_empty() {
                    // Blank lines between records (or a leading one) are
                    // just separators, not an empty record.
                    continue;
                }
                break;
            }

            if trimmed.starts_with("HTTP/") {
                record.status_line = Some(trimmed.to_string());
                continue;
            }

            if let Some((name, value)) = trimmed.split_once(':') {
                record
                    .headers
                    .push((name.trim().to_string(), value.trim().to_string()));
            }
        }

        if record.is_empty() {
            None
        } else {
            Some(Ok(record))
        }
    }
}

/// Runs the built-in checks against one response's headers.
pub fn lint(record: &HeaderRecord) -> Vec<Finding> {
    let mut findings = Vec::new();

    let cache_control = record.get("cache-control").map(CacheControl::parse);

    match &cache_control {
        None => {
            findings.push(Finding::new(
                Severity::Info,
                "no Cache-Control header; caching is left to the client's heuristics",
            ));
        }
        Some(cc) => {
            if cc.has("no-store") && (cc.has("max-age") || cc.has("immutable")) {
                findings.push(Finding::new(
                    Severity::Warning,
                    "no-store combined with a freshness directive; no-store wins, so the \
                     other directive is likely a mistake",
                ));
            }

            if cc.has("no-cache") && cc.has("immutable") {
                findings.push(Finding::new(
                    Severity::Warning,
                    "no-cache combined with immutable; these contradict each other",
                ));
            }

            if cc.has("max-age") && record.get("expires").is_some() {
                findings.push(Finding::new(
                    Severity::Info,
                    "both max-age and Expires are set; max-age takes precedence, so Expires \
                     is dead weight",
                ));
            }
        }
    }

    let revalidatable = record.get("etag").is_some() || record.get("last-modified").is_some();
    let stores = cache_control
        .as_ref()
        .map(|cc| !cc.has("no-store"))
        .unwrap_or(true);

    if !revalidatable && stores {
        findings.push(Finding::new(
            Severity::Info,
            "no ETag or Last-Modified; a conditional request has nothing to send once the \
             response goes stale",
        ));
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(headers: &[(&str, &str)]) -> HeaderRecord {
        HeaderRecord {
            status_line: Some("HTTP/1.1 200 OK".to_string()),
            headers: headers
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }

    #[test]
    fn flags_missing_cache_control() {
        let findings = lint(&record(&[]));
        assert!(findings.iter().any(|f| f.message.contains("no Cache-Control")));
    }

    #[test]
    fn flags_no_store_with_max_age() {
        let findings = lint(&record(&[("Cache-Control", "no-store, max-age=60")]));
        assert!(findings.iter().any(|f| f.message.contains("no-store")));
    }

    #[test]
    fn quiet_when_headers_are_sane() {
        let findings = lint(&record(&[
            ("Cache-Control", "max-age=300"),
            ("ETag", "\"abc\""),
        ]));
        assert!(findings.is_empty());
    }

    #[test]
    fn records_reads_two_blocks_separated_by_blank_line() {
        let input = b"HTTP/1.1 200 OK\r\nCache-Control: no-store\r\n\r\nHTTP/1.1 304 Not Modified\r\nETag: \"x\"\r\n";
        let records: Vec<_> = Records::new(&input[..])
            .collect::<io::Result<_>>()
            .unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].get("cache-control"), Some("no-store"));
        assert_eq!(records[1].status_line.as_deref(), Some("HTTP/1.1 304 Not Modified"));
    }
}
