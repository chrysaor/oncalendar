# Security policy

## Report a vulnerability

Report a vulnerability privately through GitHub. Open the **Security** tab of this repository, select **Report a vulnerability**, and describe the problem. Include the input that triggers it, the crate version, the enabled features, and what happens.

Do not open a public issue for a vulnerability. A result that differs from systemd is a normal bug, so report it with the "Different result from systemd" issue template.

## Supported versions

Security fixes go into the latest release of the newest minor version. While the crate is at 0.x, a minor version is a version such as 0.1 or 0.2.

| Version | Supported |
| --- | --- |
| 0.1.x | Yes |

## Threat model

The library treats the expression text as untrusted. It can come from a web form, a configuration file that another user wrote, or a fuzzer. The caller controls the base time, the local time zone, and the time zone database that it passes to `upcoming_with`.

The library promises the following, and the checks in this repository test each promise:

- No `unsafe` code. `Cargo.toml` sets `unsafe_code = "forbid"`, and `scripts/check-security.sh` fails when that line is missing.
- No panic on any input. Clippy denies the common panicking constructs, such as `unwrap`, indexing, and unchecked arithmetic. The lints do not catch every panic, so `tests/robustness.rs` feeds arbitrary strings, mutated expressions, huge numbers, and extreme base times and time zones, and CI fuzzes the parser and the elapse search. `scripts/check-security.sh` fails when `src/` turns a lint off.
- Every parse and every step of the elapse iterator ends. The robustness tests run pathological inputs of up to 1,000,000 bytes and the worst known search cases under a time limit. One step of the iterator has a fixed step limit.
- Memory does not grow with the input. Each numeric field holds at most 241 list items, and the parser stops at item 242.
- Error messages never contain the input. A `ParseError` message is a fixed phrase and a byte offset, so it cannot carry newlines or terminal escape sequences into a log. The robustness tests check this.
- A time zone name in an expression cannot name a path. The parser accepts only ASCII letters, digits, `_`, `+`, `-`, and `/`, at most 64 bytes, with no leading `/` and no empty part. The robustness tests check names such as `../../etc/passwd`.
- The crate's own code reads no file and no environment variable. The crate is `#![no_std]`, and `clippy.toml` forbids the functions that read the environment or the system time zone.

These are outside the library's control:

- `upcoming` and `upcoming_with` return an iterator that can be very long. `*:*:0/0.000001` elapses every microsecond. Bound the iterator with `take`.
- With a named time zone, `upcoming` reads the time zone database through jiff. On Unix, jiff reads the directory in `TZDIR` when it is set, and otherwise a system directory such as `/usr/share/zoneinfo`. Whoever controls those files controls the answers. To avoid the file system, pass `jiff::tz::TimeZoneDatabase::bundled()` to `upcoming_with`.
- A vulnerability in `jiff` belongs to that project. Report it to jiff as well. CI checks the dependencies against the RustSec advisory database with `cargo deny` on every change and once a week.
