//! Manual throughput check against a large synthetic capture.
//!
//! Stable Rust has no built-in bench harness (`#[bench]` needs nightly
//! and the `test` crate), so this is an example binary instead: build a
//! capture in memory, run it through `Records` and `lint`, and report
//! throughput. Release mode matters here, debug builds are much slower
//! and not representative of anything.
//!
//!     cargo run --release --example bench_synthetic
//!     cargo run --release --example bench_synthetic -- 2000000
//!
//! The optional argument is the record count (default 500_000).

use std::env;
use std::io::Cursor;
use std::time::Instant;

use cachelint::{lint, Records};

fn main() {
    let record_count: usize = env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(500_000);

    let capture = synthetic_capture(record_count);
    let bytes = capture.len();
    println!(
        "generated {record_count} records ({:.1} MiB)",
        bytes as f64 / (1024.0 * 1024.0)
    );

    let start = Instant::now();
    let mut records_seen = 0usize;
    let mut findings_seen = 0usize;
    for record in Records::new(Cursor::new(capture.as_bytes())) {
        let record = record.expect("synthetic capture is well-formed");
        records_seen += 1;
        findings_seen += lint(&record).len();
    }
    let elapsed = start.elapsed();

    assert_eq!(records_seen, record_count);
    println!(
        "linted {records_seen} records ({findings_seen} findings) in {elapsed:?} \
         ({:.0} records/sec, {:.1} MiB/sec)",
        records_seen as f64 / elapsed.as_secs_f64(),
        (bytes as f64 / (1024.0 * 1024.0)) / elapsed.as_secs_f64()
    );
}

/// Builds a capture with a mix of clean and problematic responses,
/// cycling through a handful of realistic patterns so the lint rules
/// actually run their checks instead of everything hitting the same
/// early branch.
fn synthetic_capture(record_count: usize) -> String {
    let mut out = String::with_capacity(record_count * 160);
    for i in 0..record_count {
        let (status, headers): (&str, &[(&str, &str)]) = match i % 5 {
            0 => (
                "HTTP/1.1 200 OK",
                &[("Cache-Control", "max-age=300"), ("ETag", "\"abc123\"")],
            ),
            1 => (
                "HTTP/1.1 200 OK",
                &[("Cache-Control", "no-store, max-age=60")],
            ),
            2 => (
                "HTTP/1.1 304 Not Modified",
                &[
                    ("Cache-Control", "max-age=300"),
                    ("Vary", "*"),
                    ("ETag", "\"def456\""),
                ],
            ),
            3 => ("HTTP/1.1 200 OK", &[]),
            _ => (
                "HTTP/2 200",
                &[
                    ("cache-control", "no-cache, immutable"),
                    ("last-modified", "Tue, 01 Sep 2026 00:00:00 GMT"),
                ],
            ),
        };

        out.push_str(status);
        out.push_str("\r\n");
        for (name, value) in headers {
            out.push_str(name);
            out.push_str(": ");
            out.push_str(value);
            out.push_str("\r\n");
        }
        out.push_str("\r\n");
    }
    out
}
