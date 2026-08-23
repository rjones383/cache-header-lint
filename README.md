# cachelint

Cache-Control headers are easy to get subtly wrong: `no-store` next to a
`max-age`, `immutable` on something that also says `no-cache`, an `Expires`
header nobody remembered to delete after adding `max-age`. None of these
are invalid HTTP, they're just contradictions that mean the header was
probably written by hand and not actually thought through. cachelint reads a
dump of response headers and points out that kind of thing.

## Usage

Capture some headers and lint them:

```
curl -D - -o /dev/null https://example.com/ > headers.txt
cachelint headers.txt
```

Or pipe directly:

```
curl -D - -o /dev/null -s https://example.com/ | cachelint
```

Output looks like:

```
record 1 [HTTP/1.1 200 OK]
  info: no ETag or Last-Modified; a conditional request has nothing to send once the response goes stale
```

No arguments means read from stdin. Exit code is 1 if anything was flagged,
0 if the input was clean, 2 on an I/O error.

## Input format

Records are separated by a blank line: a status line, then header lines,
matching what `curl -D -` writes for each response. A file can hold many
records back to back, which is the case this tool is built around — you
point it at a log of headers collected from a crawl, not just one response.

## Library

The CLI is a thin wrapper around the `cachelint` library:

```rust
use cachelint::{lint, Records};
use std::io::stdin;

for record in Records::new(stdin()) {
    let record = record?;
    for finding in lint(&record) {
        println!("{}: {}", finding.severity.as_str(), finding.message);
    }
}
```

`Records` reads one record at a time from anything that implements
`std::io::Read`. It never buffers more than the current record, so linting
a multi-gigabyte capture file costs the same memory as linting one response.

## Why no dependencies

This is a small, single-purpose tool. The standard library's `BufReader`
already gives streaming line-by-line reads, and the header format doesn't
need a real parser generator, so pulling in a header-parsing crate or an
argument-parsing crate would be adding weight without adding much.

## Status

Early. The lint rules so far only cover a handful of `Cache-Control`
contradictions — see the roadmap below for what's missing.

## License

MIT, see LICENSE.
