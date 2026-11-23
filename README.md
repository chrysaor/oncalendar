# oncalendar

[![crates.io](https://img.shields.io/crates/v/oncalendar.svg)](https://crates.io/crates/oncalendar)
[![docs.rs](https://docs.rs/oncalendar/badge.svg)](https://docs.rs/oncalendar)
[![CI](https://github.com/chrysaor/oncalendar/actions/workflows/ci.yml/badge.svg)](https://github.com/chrysaor/oncalendar/actions/workflows/ci.yml)

`oncalendar` is a Rust library for systemd calendar event expressions, the values you write in `OnCalendar=` in a timer unit. It parses an expression, prints the normalized form that `systemd-analyze calendar` prints, and lists the instants when the expression elapses after a given time, in a given time zone, with systemd's daylight saving time behavior. The expected results in the tests come from the real `systemd-analyze calendar` in a pinned container. The library matches them on every row where systemd gives an answer.

## Install

```toml
[dependencies]
oncalendar = "0.1"
jiff = "1"
```

`oncalendar` takes and returns `jiff` types, so a program that computes elapses also depends on `jiff`. A program that only parses and normalizes can turn off the default features and leave out `jiff`:

```toml
[dependencies]
oncalendar = { version = "0.1", default-features = false }
```

See [Features](#features) for what each feature adds.

## Command-line tool

To check an expression from a terminal, on a machine with or without systemd, install the command-line tool:

```sh
cargo install oncalendar-cli
```

```console
$ oncalendar -n 2 --zone Europe/Berlin --after '2026-03-28 12:00' '*-*-* 02:30'
*-*-* 02:30:00
  Mon 2026-03-30 02:30:00 CEST (+02:00)  in 1d 13h
  Tue 2026-03-31 02:30:00 CEST (+02:00)  in 2d 13h
```

March 29 has no 02:30 in Berlin, so it is skipped. The tool reads several expressions from standard input, prints JSON with `--json`, and reports an invalid expression with its position and exit status 1. See [cli/README.md](cli/README.md).

## Parse and normalize

`CalendarEvent` implements `FromStr`, and its `Display` output is the normalized form.

```rust
use oncalendar::CalendarEvent;

fn main() -> Result<(), oncalendar::ParseError> {
    let event: CalendarEvent = "mon..fri 9:00".parse()?;
    assert_eq!(event.to_string(), "Mon..Fri *-*-* 09:00:00");

    let error = "*-*-* 25:00".parse::<CalendarEvent>().unwrap_err();
    assert_eq!(error.to_string(), "calendar expression: hour out of range at byte 6");
    assert_eq!(error.offset(), 6);
    Ok(())
}
```

A `ParseError` message never contains the input, so you can write it to a log as it is. Use `offset()` to point at the problem in the input.

## Find the next elapse

`upcoming` returns an iterator over the instants strictly after `after`. The second argument is the time zone that the expression uses when it names none.

```rust
use jiff::Timestamp;
use jiff::tz::TimeZone;
use oncalendar::CalendarEvent;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let event: CalendarEvent = "Mon *-*-* 09:00".parse()?;
    let local = TimeZone::get("Asia/Seoul")?;
    let after: Timestamp = "2026-01-01T00:00:00Z".parse()?;

    let next: Vec<String> = event
        .upcoming(after, &local)?
        .take(2)
        .map(|instant| instant.to_string())
        .collect();
    assert_eq!(next, ["2026-01-05T00:00:00Z", "2026-01-12T00:00:00Z"]);
    Ok(())
}
```

The iterator can be very long. `*:*:0/0.000001` elapses every microsecond, so bound the iterator with `take` before you collect it. It returns `None` when the expression has no further elapse.

The library never reads `TZ` or `/etc/localtime`. To get the same answers as systemd on the host, pass `&TimeZone::system()` as the local zone. That function needs jiff's `tz-system` feature, which `jiff = "1"` turns on.

## Use the bundled time zone database

An expression can name a time zone, such as `*-*-* 09:00 Asia/Seoul`. `upcoming` looks the name up in jiff's global database, which on Unix is the system copy. In a container without tzdata, that lookup fails. `upcoming_with` takes the database as an argument. To use the copy that jiff compiles into your program, turn on jiff's `tzdb-bundle-always` feature:

```toml
[dependencies]
oncalendar = "0.1"
jiff = { version = "1", features = ["tzdb-bundle-always"] }
```

```rust
use jiff::Timestamp;
use jiff::tz::{TimeZone, TimeZoneDatabase};
use oncalendar::CalendarEvent;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database = TimeZoneDatabase::bundled();
    let after: Timestamp = "2026-01-01T00:00:00Z".parse()?;

    let event: CalendarEvent = "*-*-* 09:00 Asia/Seoul".parse()?;
    let next = event.upcoming_with(after, &TimeZone::UTC, &database)?.next();
    assert_eq!(next, Some("2026-01-02T00:00:00Z".parse()?));

    let unknown: CalendarEvent = "daily Nowhere/Bogus".parse()?;
    assert!(unknown.upcoming_with(after, &TimeZone::UTC, &database).is_err());
    Ok(())
}
```

In both functions a name must match a name in the database exactly, letter case included. An unknown name gives `UnknownTimeZone`.

## Use it without `std`

The crate is `#![no_std]` and needs `alloc`, so it runs on targets that have a global allocator. With only the `jiff` feature it computes elapses without `std`:

```toml
[dependencies]
oncalendar = { version = "0.1", default-features = false, features = ["jiff"] }
jiff = { version = "1", default-features = false }
```

Such a target usually has no time zone files. Pass UTC or a fixed offset as the local zone:

```rust
use jiff::Timestamp;
use jiff::tz::{self, TimeZone};
use oncalendar::CalendarEvent;

fn next_alarm(event: &CalendarEvent, now_unix_seconds: i64) -> Option<Timestamp> {
    let now = Timestamp::from_second(now_unix_seconds).ok()?;
    let local = TimeZone::fixed(tz::offset(9));
    event.upcoming(now, &local).ok()?.next()
}

fn main() -> Result<(), oncalendar::ParseError> {
    let event: CalendarEvent = "*-*-* 06:30".parse()?;
    let next = next_alarm(&event, 1_767_225_600);
    assert_eq!(next.map(Timestamp::as_second), Some(1_767_303_000));
    Ok(())
}
```

Without `system-tzdb`, `upcoming` knows no zone name other than `UTC`, so an expression that names another zone gives `UnknownTimeZone`. To support names there, turn on jiff's `tzdb-bundle-always` feature and pass `TimeZoneDatabase::bundled()` to `upcoming_with`. The bundled database makes the program larger.

`scripts/check-no-std.sh` builds the crate for `thumbv7em-none-eabihf` without default features and with only `jiff`. The crate has not been run on a device.

## Try it in a browser

`wasm/` holds a small crate that compiles the library to WebAssembly (`wasm32-unknown-unknown`) and a static demo page in `wasm/demo/`. The page checks an expression as you type, marks where a parse error starts, prints the normalized form, and lists the next elapses in a time zone you pick. It loads only its own files, and what you type stays in the browser.

To build the module and open the page, install the `wasm32-unknown-unknown` target and Node.js, then run these commands:

```sh
scripts/check-wasm.sh
python3 -m http.server --directory wasm/demo 8000
```

Then open `http://localhost:8000/`. Browsers do not load WebAssembly from a `file://` page, so the page needs a local server. Any static file server works.

The module is 545 KB, or 176 KB with gzip. About 280 KB of it is jiff's bundled time zone database and the zone lookups. `scripts/check-wasm.sh` also runs every fixture row through the module in Node.js and compares the results with `tests/fixtures/expected.tsv`.

## Features

| Feature | Default | What it adds |
| --- | --- | --- |
| `jiff` | on, through `system-tzdb` | `CalendarEvent::upcoming`, `CalendarEvent::upcoming_with`, `Upcoming`, and `UnknownTimeZone`. Depends on `jiff` 1 with only its `alloc` feature, so it works without `std`. |
| `system-tzdb` | on | Named time zones from the system time zone database for `upcoming`. Turns on `jiff`, and jiff's `std`, `tzdb-zoneinfo`, and `tzdb-bundle-platform` features. Needs `std`. |

With `default-features = false`, the crate parses and normalizes, and it compiles no other crate. With the default features on a Unix host, it compiles `jiff` and `jiff-core`. `scripts/check-weight.sh` checks both counts.

`tzdb-bundle-platform` compiles the database in only on Windows and wasm, where no system copy exists.

## Speed

On an Apple M3 Max, parsing `Mon..Fri *-*-* 09:00:00` takes about 0.2 µs and finding its next elapse about 0.1 µs. `cargo bench` runs the benchmark, and [docs/architecture.md](docs/architecture.md) has the table and the method.

## Supported syntax

An expression has up to four parts, separated by spaces: weekdays, a date, a time, and a time zone. Each numeric field is `*` or a comma list of values (`5`), ranges (`1..5`), repetitions (`0/15`), and stepped ranges (`9..17/2`). A missing date means every day. A missing time means midnight.

| Expression | Normalized form | Elapses |
| --- | --- | --- |
| `daily` | `*-*-* 00:00:00` | Every day at midnight |
| `Mon..Fri 09:00` | `Mon..Fri *-*-* 09:00:00` | Weekdays at 09:00 |
| `Sat,Sun 10:30` | `Sat,Sun *-*-* 10:30:00` | Weekends at 10:30 |
| `*-*-1,15 8:00` | `*-*-01,15 08:00:00` | The 1st and the 15th of each month at 08:00 |
| `Mon *-*-1..7 10:00` | `Mon *-*-01..07 10:00:00` | The first Monday of each month at 10:00 |
| `*-02~01` | `*-02~01 00:00:00` | The last day of February |
| `*-*~03` | `*-*~03 00:00:00` | The third day before the end of each month |
| `2026-03-15 08:00` | `2026-03-15 08:00:00` | Once |
| `*:0/15` | `*-*-* *:00/15:00` | Every 15 minutes |
| `9..17/2:00` | `*-*-* 09..17/2:00:00` | At 09:00, 11:00, 13:00, 15:00, and 17:00 |
| `*:*:0/0.5` | `*-*-* *:*:00/0.500000` | Every half second |
| `weekly Europe/Berlin` | `Mon *-*-* 00:00:00 Europe/Berlin` | Mondays at midnight in Berlin |
| `Mon *-*-* 09:00 UTC` | `Mon *-*-* 09:00:00 UTC` | Mondays at 09:00 UTC |

More details:

- Weekdays are three-letter or full English names in any case. `-` and `..` both mark a range.
- `~` in place of the last `-` of a date counts days from the end of the month. `~01` is the last day.
- Years run from 1970 to 2199. Two-digit years 00 to 69 mean 20xx, and 70 to 99 mean 19xx.
- Seconds can have a fraction, rounded to microseconds.
- The shorthands are `minutely`, `hourly`, `daily`, `weekly`, `monthly`, `yearly`, `annually`, `quarterly`, and `semiannually`, in any case. Only a time zone may follow a shorthand.
- A time zone is `UTC` in any case, or a name from the time zone database, such as `Asia/Seoul`.
- Each numeric field takes at most 241 list items, as systemd does.

The rest of the systemd time syntax is out of scope: timestamps such as `now` and `@1700000000`, time spans, and timer unit files. [docs/architecture.md](docs/architecture.md#normalization-and-output-rules) has every normalization rule with the systemd output it was checked against.

## Known differences from systemd

- An unknown time zone name, such as `daily Nowhere/Bogus`, parses. `upcoming` then returns `UnknownTimeZone`. systemd rejects it at parse time. The same holds for a last word that only looks like a zone name, such as `daily Mon`.
- Local time zone abbreviations, such as `daily KST`, are not supported. systemd accepts them only when the process time zone uses that abbreviation, and then elapses only while the abbreviation is in effect. A name that is both, such as `EST` under `TZ=America/New_York`, is read by this library as the database zone.
- The local time zone is an argument. Parsing reads no file and no environment variable. `upcoming` with a named zone reads the time zone database through jiff, which honors `TZDIR`.
- The set of names depends on the database. The bundled database has `EST`, `UCT`, `Zulu`, and `W-SU`. A Debian system without the `tzdata-legacy` package has none of them, so systemd there rejects `*-*-* 10:00 EST`, and the library with the bundled database accepts it.
- Where systemd gives up computing an elapse ("Failed to determine next elapse"), the library still answers. `Sun *-03-08..14 02:30 America/New_York` never elapses, and systemd gives up on it from a 2026 base time. systemd also gives up on some expressions whose next elapse is months away and nowhere near a gap, such as `*-5/6-21,12 7,20..21:41,11/18:26` in `Europe/London` from 2026-12-09. From a later base time systemd answers, and the answer matches the library's.
- In `Pacific/Chatham`, where the clock jumps from 02:45 to 03:45, systemd leaves out some times that exist on that day and gives up on others, such as `*:45`. The library elapses at every time that exists.
- Near some daylight saving changes the two give different elapses. In `America/Havana` after the midnight gap, `9/14,1:4/22` elapses in systemd at 01:26 and in the library at 01:04. In `Europe/Dublin`, a candidate in the spring gap makes systemd move on to a later day. In some folds in `Asia/Gaza` and `Africa/Cairo`, systemd takes the second pass where the library takes the first.
- Some fractional repetitions in seconds, such as `*:*:0/40.9`, never elapse at their start value in systemd, while the library elapses there. `*:*:0/40.5` and whole-second steps agree.

Each difference was observed with systemd 257.13. [docs/architecture.md](docs/architecture.md#known-differences-from-systemd) gives the exact base times and outputs.

## How it is verified

The expected results come from systemd, never from this crate. `oracle/` runs `systemd-analyze calendar` from systemd 257.13 in a Debian container, with the base image pinned by digest and the packages pinned by version. It writes the results to `tests/fixtures/expected.tsv`.

- `tests/oracle.rs` checks every fixture row through the public API: whether the expression is accepted, the normalized form, and the first five elapses.
- `tests/normalize.rs` checks the normalized forms again with no feature, so the build without dependencies is covered.
- `tests/robustness.rs` checks that hostile input never panics, never loops, and never appears in an error message, and checks the elapses against a slow reference search.
- `scripts/check-wasm.sh` builds the library for WebAssembly and checks every fixture row again through that build.
- The scripts in `scripts/` check the lints, the licenses of dependencies, the dependency count, the `no_std` build, every feature combination, the lowest allowed dependency versions, the packaged files, and the CI configuration. `scripts/fuzz.sh` runs the `cargo-fuzz` targets in `fuzz/`.
- `cli/tests/` runs the command-line tool against every fixture row, as an argument and on standard input.
- CI runs the tests on Linux, macOS, and Windows. A weekly run checks for new advisories, rebuilds the oracle image, and fuzzes for 30 minutes per target.

To run the tests:

```sh
cargo test --all-features
cargo test --no-default-features
```

`oracle/check.sh` needs Docker. It reruns systemd and fails when `tests/fixtures/expected.tsv` is out of date. [CONTRIBUTING.md](CONTRIBUTING.md) lists the other checks.

## Minimum supported Rust version

The minimum supported Rust version is 1.85, and the crate uses edition 2024. CI checks the crate on 1.85 and runs the tests there with the lowest allowed dependency versions.

## Contributing and security

Read [CONTRIBUTING.md](CONTRIBUTING.md) before you open a pull request. The implementation is clean-room: it is written from the `systemd.time(7)` manual and the output of `systemd-analyze calendar` only. To report a vulnerability, follow [SECURITY.md](SECURITY.md). Changes are listed in [CHANGELOG.md](CHANGELOG.md).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
