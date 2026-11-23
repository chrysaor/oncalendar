# Changelog

This file lists the notable changes in each release. The format follows [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/), and the version numbers follow [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html).

## Unreleased

### Added

- `oncalendar-cli` 0.1.0, a command-line tool in `cli/`, installed with `cargo install oncalendar-cli`. Its executable, `oncalendar`, checks an expression, prints its normalized form, and lists its next elapses on any machine, with or without systemd. It takes the number of elapses, the base time, and the local time zone as options, reads one expression per line from standard input, prints JSON with `--json`, and can use the time zone database compiled into it, so zone names also work in a container without tzdata. Each kind of failure has its own exit status. The library is unchanged.
- A WebAssembly build of the library in `wasm/`, outside the published crate, and a static demo page in `wasm/demo/`. The page checks an expression, marks where a parse error starts, prints the normalized form, and lists the next elapses in a chosen time zone.
- `scripts/check-wasm.sh`, a gate that builds the library for `wasm32-unknown-unknown` and checks every fixture row through the WebAssembly module in Node.js. It runs in `scripts/verify gates` and in CI, and CI also builds the module with Rust 1.85.

The published crate does not change.

## 0.1.0 - 2026-11-23

The first release.

### Added

- `CalendarEvent`, a parsed systemd calendar event expression, the value of `OnCalendar=`. Parse it with `str::parse`. Its `Display` output is the normalized form that `systemd-analyze calendar` prints. Two expressions with the same normalized form are equal.
- `ParseError`, with a fixed message and the byte offset of the problem in `ParseError::offset`. The message never contains the input.
- `CalendarEvent::upcoming`, an iterator over the instants when the expression elapses after a given instant. The caller passes the local time zone. A time in a daylight saving gap is skipped, and a time in a fold elapses once.
- `CalendarEvent::upcoming_with`, the same with a time zone database that the caller passes, such as jiff's bundled database.
- `Upcoming`, the iterator, and `UnknownTimeZone`, the error for a time zone name that the database does not have.
- The `jiff` feature for `upcoming`, `upcoming_with`, `Upcoming`, and `UnknownTimeZone`. It needs only `alloc`.
- The `system-tzdb` feature, on by default, for named time zones from the system time zone database.
- Support for `no_std` targets with a global allocator. Without default features the crate depends on no other crate.
- A minimum supported Rust version of 1.85.

The differences from systemd 257 that remain are listed under "Known differences from systemd" in `README.md`.
