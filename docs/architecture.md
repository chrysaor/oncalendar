# oncalendar architecture

This document is the design for `oncalendar` v0.1, and `src/` implements it. Every section says what is decided and why, so that a contributor can change the code without reopening the decisions.

Behavior claims marked "observed" come from `systemd-analyze calendar` in systemd 257.13, run in the container under `oracle/`. Numbers marked "measured" were measured on small prototype crates during the design. Numbers marked "estimate" were worked out by hand and not measured.

## Goals and scope

The crate does three things with a systemd calendar event expression, the value of `OnCalendar=` in a timer unit:

1. Parse it from untrusted text.
2. Print its normalized form, byte for byte as `systemd-analyze calendar` prints it.
3. List the instants when it elapses after a given instant, in a given local time zone, with the same daylight saving time behavior as systemd.

The crate must stay light. The parser and the normalized output have no dependencies. Hostile input must never make the crate panic, loop forever, or use memory in proportion to the input.

Out of scope for v0.1:

- The rest of the systemd time syntax: timestamps such as `now`, `+3h`, and `@1700000000`, and time spans.
- Timer unit files.
- Local time zone abbreviations in expressions, such as `KST` or `EST`. See [Known differences from systemd](#known-differences-from-systemd).
- A syntax tree that keeps the input spelling, `serde` support, builders, and accessors for the parts of an expression.

## Sources of truth and the clean-room rule

The design uses two sources:

- The manual page `systemd.time(7)`, section "Calendar Events", for systemd v257.
- Black-box runs of `systemd-analyze calendar` in the pinned container under `oracle/`.

Contributors do not open the systemd C source or its test files. They also do not open the source of OnCalendar implementations in other languages. systemd is licensed LGPL-2.1-or-later, and this crate is `MIT OR Apache-2.0`. Code or test tables derived from LGPL source would bring LGPL terms into the crate. Other implementations have their own licenses, and reading them makes it hard to show that this code was written independently.

When the manual leaves a question open, ask systemd:

1. Add a row to `tests/fixtures/cases.tsv`.
2. Run `oracle/run.sh < tests/fixtures/cases.tsv > tests/fixtures/expected.tsv`.
3. Commit both files.

Describe the grammar in your own words in docs and comments. Do not paste paragraphs from the manual.

## The expression in brief

An expression has up to four parts, in this order, separated by spaces:

1. A weekday list, such as `Mon,Fri` or `Mon..Wed`.
2. A date, `year-month-day`. A `~` in place of the last `-` counts days from the end of the month.
3. A time, `hour:minute:second`. Seconds can have a fraction.
4. A time zone, either `UTC` or an IANA name such as `Asia/Seoul`.

Each numeric field holds `*`, or a comma-separated list of items. An item is a value `v`, a range `a..b`, a repetition `v/r`, or a stepped range `a..b/r`. A missing date means every day. A missing time means `00:00:00`. Shorthands such as `daily` and `weekly` stand for whole expressions. The rules under [Normalization and output rules](#normalization-and-output-rules) pin down each detail with an observed example.

## Data model

The model holds only the normalized form. Normalization is not a separate step. It is the invariant of the model constructors, and `Display` prints the model as it is. All model types are `pub(crate)`. The fields of `CalendarEvent` are private.

```rust
/// A parsed calendar event. Always normalized.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CalendarEvent {
	weekdays: WeekdaySet,
	date: DateSpec,
	time: TimeSpec,
	zone: Zone,
}

/// Seven bits, Monday is bit 0. The empty set cannot be built.
/// `WeekdaySet::ALL` means "no weekday condition".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct WeekdaySet(core::num::NonZeroU8);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct DateSpec {
	year: Component<Year>,
	month: Component<Month>,
	day: DaySpec,
}

/// `-` counts days from the start of the month, `~` from the end.
/// One expression never mixes the two.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum DaySpec {
	FromStart(Component<Day>),
	/// No `Any` here. `*-*~*` normalizes to `FromStart(Component::Any)`.
	FromEnd(Items<DayFromEnd>),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct TimeSpec {
	hour: Component<Hour>,
	minute: Component<Minute>,
	second: Component<Second>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Component<F: FieldSpec> {
	Any,
	List(Items<F>),
}

/// Not empty, sorted, no exact duplicates, at most `MAX_ITEMS` (241) items.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Items<F: FieldSpec>(Vec<Item<F>>);

/// No derived order. `Items::new` sorts by the key (start, stop, step).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Item<F: FieldSpec> {
	/// `v`
	Value(Value<F>),
	/// `v/r`. Runs to the end of the field and reaches at least two values.
	/// That means `v + r <= F::MAX`, or `v > r` for days from the end of the month.
	Every { start: Value<F>, step: Step<F> },
	/// `a..b` with `a < b`. It steps by `F::UNIT`, so a seconds range holds whole-second steps.
	Range { start: Value<F>, stop: Value<F> },
	/// `a..b/r` with `a < b`, `b - a` a multiple of `r`, and `r` not `F::UNIT`.
	/// `a..b/1` is stored as `Range`.
	Stepped { start: Value<F>, stop: Value<F>, step: Step<F> },
}

/// A value inside the range of field `F`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Value<F: FieldSpec>(u32, PhantomData<F>);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Step<F: FieldSpec>(core::num::NonZeroU32, PhantomData<F>);

/// Range, unit, and error label of one field. Implemented by zero-sized marker types.
trait FieldSpec: Copy + Ord + core::hash::Hash + core::fmt::Debug {
	const MIN: u32;
	const MAX: u32;
	const FIELD: Field;
	/// Distance between neighboring values: 1, or 1,000,000 µs for seconds.
	const UNIT: u32 = 1;
	/// `v/r` counts down toward `MIN`. True only for days from the end.
	const DESCENDING: bool = false;
	/// A range without a repetition needs two values. True only for seconds.
	const PLAIN_RANGE_NEEDS_TWO: bool = false;
	/// The item `MIN/UNIT` turns the component into `Any`. True only for seconds.
	const EVERY_UNIT_IS_ANY: bool = false;
}

struct Year;       // 1970..=2199
struct Month;      // 1..=12
struct Day;        // 1..=31
struct DayFromEnd; // 1..=28, 1 is the last day of the month
struct Hour;       // 0..=23
struct Minute;     // 0..=59
struct Second;     // 0..=59_999_999, in microseconds

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Zone {
	/// No time zone in the expression. The caller's local zone applies.
	Local,
	/// `UTC` in any case. Always printed as `UTC`.
	Utc,
	/// A name that passed the format check. Printed as written.
	/// Whether the zone exists is checked by `upcoming`, not by the parser.
	Named(Box<str>),
}
```

Seconds are stored as whole microseconds. The manual rounds fractions to six digits, and integer microseconds make `Eq`, `Hash`, and the remainder arithmetic in the search exact. The `Step` of the seconds field is in microseconds too.

The parser expands shorthands such as `daily` into this model. No trace of the shorthand stays in the model.

`Zone::Named` holds the name, not a `jiff::tz::TimeZone`. That keeps the model free of jiff, lets the model derive `Hash` (jiff's `TimeZone` does not implement `Hash`), and keeps parsing away from the file system.

### Illegal states the types rule out

| Illegal state | What prevents it |
| --- | --- |
| A weekday set with no days | `NonZeroU8` |
| Two spellings of "every weekday", as no set and as all seven days | No `Option`. `WeekdaySet::ALL` is the only form. |
| A value outside its field, such as hour 25, month 13, or year 2200 | The `Value<F>` constructor checks `F::MIN..=F::MAX` |
| A minute component in the hour slot | `Component<Hour>` and `Component<Minute>` are different types |
| A repetition of 0 | `NonZeroU32` |
| An empty, unsorted, or duplicated list | `Items<F>` has one constructor, which sorts and removes exact duplicates |
| More than 241 items in one numeric field | The `Items<F>` constructor. The parser stops at item 242. |
| `*` mixed with values | `Component` is an enum |
| A reversed range, a one-value range, or an end value the step never reaches | The `Item` constructor rejects reversed ranges, folds one-value ranges into `Value`, and shrinks the end value |
| Days counted from both ends in one expression, or `~*` | `DaySpec` is an enum, and `FromEnd` has no `Any` |
| A model that is not yet normalized | No such value exists |

The types allow dates that are not on the calendar, such as `*-02-30`. systemd parses that expression and reports that it never elapses (observed), so it is a valid state. The rule `v + r <= F::MAX` for `Every` is checked by the constructor and is not visible in the type. The same holds for the limit on lists of days from the end, which `DaySpec::from_end` checks (see [Dates](#dates)).

## Public API

This is the whole public API.

```rust
pub struct CalendarEvent { /* private */ }      // Clone, Debug, PartialEq, Eq, Hash
impl core::str::FromStr for CalendarEvent { type Err = ParseError; }
impl core::fmt::Display for CalendarEvent { /* normalized form */ }

pub struct ParseError { /* private */ }          // Clone, Debug, PartialEq, Eq
impl ParseError {
	/// Byte offset in the input where the problem starts.
	pub fn offset(&self) -> usize;
}
impl core::fmt::Display for ParseError { /* fixed text and the offset */ }
impl core::error::Error for ParseError {}

#[cfg(feature = "jiff")]
impl CalendarEvent {
	/// Instants strictly after `after` when the event elapses, in order.
	/// `local` applies when the expression names no time zone.
	/// A named zone is looked up in jiff's global database, `jiff::tz::db()`.
	pub fn upcoming(
		&self,
		after: jiff::Timestamp,
		local: &jiff::tz::TimeZone,
	) -> Result<Upcoming<'_>, UnknownTimeZone>;

	/// The same, with a named zone looked up in `database`.
	pub fn upcoming_with(
		&self,
		after: jiff::Timestamp,
		local: &jiff::tz::TimeZone,
		database: &jiff::tz::TimeZoneDatabase,
	) -> Result<Upcoming<'_>, UnknownTimeZone>;
}

#[cfg(feature = "jiff")]
pub struct Upcoming<'a> { /* private */ }        // Clone, Debug
#[cfg(feature = "jiff")]
impl Iterator for Upcoming<'_> { type Item = jiff::Timestamp; }
#[cfg(feature = "jiff")]
impl core::iter::FusedIterator for Upcoming<'_> {}

#[cfg(feature = "jiff")]
pub struct UnknownTimeZone { /* private */ }     // Clone, Debug, PartialEq, Eq
#[cfg(feature = "jiff")]
impl core::fmt::Display for UnknownTimeZone {}
#[cfg(feature = "jiff")]
impl core::error::Error for UnknownTimeZone {}
```

Each task has exactly one way to do it:

| Task | The one way |
| --- | --- |
| Parse | `s.parse::<CalendarEvent>()` |
| Check validity only | `s.parse::<CalendarEvent>().is_ok()` |
| Print the normalized form | `event.to_string()` or `{}` |
| Next elapse | `event.upcoming(after, &local)?.next()` |
| Several elapses | `event.upcoming(after, &local)?.take(n)` |
| Elapses with a chosen zone database | `event.upcoming_with(after, &local, &database)?` |

Left out on purpose:

- `next_after`. It would be a second way to get the first item of `upcoming`.
- `CalendarEvent::parse` and `TryFrom<&str>`. They would duplicate `FromStr`.
- `normalize()`. It would duplicate `Display`.
- Any function that reads the system time zone. The caller passes `&jiff::tz::TimeZone::system()` when it wants systemd's behavior.
- A public error kind enum and a public field enum. systemd reports only success or failure, so there is nothing to compare a kind against. A kind can be added later without breaking callers.
- `pub use jiff`. A re-export hides which jiff version the caller is linking against.
- Structural accessors. They would make the four `Item` shapes part of semver while some normalization questions are still open.

`upcoming` returns `Err(UnknownTimeZone)` only when the expression names a zone that the time zone database does not have. Once it returns `Ok`, the search cannot fail. Years are bounded to 1970..=2199 on the wall clock of the effective zone, so a UTC instant can lie a few hours outside them. "No further elapse" is `None`.

`Upcoming` borrows only the event. `local` and the database are read once, when the call resolves the effective zone, and the iterator keeps its own copy of that zone.

`upcoming` is `upcoming_with` with jiff's global database. `upcoming_with` is for two cases:

- A host without time zone files, such as a minimal container. With jiff's `tzdb-bundle-always` feature on, `&jiff::tz::TimeZoneDatabase::bundled()` holds the database compiled into the binary. Without that feature it is empty, and every named zone fails.
- The same answers on every host. The tests, the oracle comparison, and the verification probe pass the bundled database, so their results do not depend on the host's tzdata.

A name must match a name in the database exactly, letter case included, in both functions. `TimeZoneDatabase::get` itself ignores case, so the crate also compares the canonical name of the zone it returns.

`jiff::Timestamp` has nanosecond resolution, and this crate works in microseconds. `upcoming` rounds `after` down to a whole microsecond and then looks for instants strictly greater. Every candidate is a whole microsecond, so rounding down never changes which candidate is "strictly after". Rounding up would be wrong. With `after` at `.4999995` s, rounding up to `.500000` s would skip an elapse at `.5` s.

`ParseError` does not wrap any jiff type, so no external type leaks through the error.

## Dependencies and features

The core has no dependencies. It holds the parser, the model, and the normalized output. The parser is a pure function of the input string. It reads neither the file system nor the environment.

The feature `jiff` adds `upcoming`, `Upcoming`, and `UnknownTimeZone`. It is on by default. With `default-features = false` the crate parses and normalizes, and compiles no other crate.

Manifest entries:

```toml
[features]
default = ["jiff"]
jiff = ["dep:jiff"]

[dependencies]
jiff = { version = "1", optional = true, default-features = false, features = ["std", "tzdb-zoneinfo", "tzdb-bundle-platform"] }
```

- `tz-system` stays off. The crate never calls `TimeZone::system()`. A caller that wants the system zone turns `tz-system` on in its own jiff dependency.
- `tzdb-bundle-platform` bundles the time zone database only on Windows and wasm, where no system copy exists. On Unix the database comes from the system, usually `/usr/share/zoneinfo`.
- jiff appears in the public API through `jiff::Timestamp` and `jiff::tz::TimeZone`. A new jiff major version therefore means a new `oncalendar` minor version while the crate is at 0.x.

MSRV is 1.85 with edition 2024. jiff 1.0.2 declares `rust-version = "1.85"`.

The requirement `jiff = "1"` stays as it is. Its lowest version, jiff 1.0.0, builds and passes every test on 1.85 and on stable (measured with `scripts/check-minimal-versions.sh`).

### `no_std`

The crate is `#![no_std]` and links `alloc` in every configuration, with no `std` or `alloc` feature. It needs a global allocator, because a parsed expression holds its lists in `Vec` and a zone name in `Box<str>`. The error types implement `core::error::Error`, which is stable since Rust 1.81, below the MSRV.

Without default features the crate builds for `thumbv7em-none-eabihf`, a target with no `std`. With the `jiff` feature the crate itself is still `no_std`, but jiff's `tzdb-zoneinfo` feature turns on jiff's `std` feature, so that build needs a target with `std`. With jiff's features reduced to `alloc`, the `jiff` feature also builds for `thumbv7em-none-eabihf` on the current toolchain and on 1.85 (measured once, not gated). That configuration would have no system time zone database, so `upcoming` would find no named zone. It is not offered.

The unit test in `next` declares `extern crate std` inside its test module to print its report. `#![cfg_attr(not(test), no_std)]` would do the same at the crate root, but `scripts/check-security.sh` rejects `cfg_attr` in `src/`.

Reasons for the split:

- A user who only validates or normalizes expressions, such as a unit file linter, compiles zero extra crates.
- Every line of code that touches the untrusted string is in this crate, under this crate's lints.
- Parse results do not depend on the host's tzdata or on which features other crates in the build turned on.

The cost is one known difference from systemd. systemd rejects an unknown time zone name at parse time, and this crate reports it from `upcoming`. The fixture comparison treats "parse succeeds and `upcoming` succeeds" as systemd's success.

### Measurements

Measured on prototype executables, release build, stripped, five alternating runs on a machine with load average 32 to 33. The jiff prototypes look up a zone by name and convert one civil time. The baseline prototype prints the length of its argument.

| Configuration | Stripped executable | Cold release build | Crates compiled on the macOS host |
| --- | --- | --- | --- |
| No dependencies | 343,360 bytes | 0.3 s | 0 |
| jiff 1.0.2, `std` and `tzdb-zoneinfo` only | 569,616 bytes (+226,256) | within noise of the row below | not measured |
| jiff 1.0.2, default features | 670,224 bytes (+326,864) | 2.6 s (2.6 to 2.7) | 2 (`jiff`, `jiff-core`) |

With default features, jiff resolves to 12 crates summed over all platforms, including the Windows and wasm ones. The reduced row did not include `tzdb-bundle-platform`. That feature adds a crate only on Windows and wasm, so the macOS number should not change (estimate).

### Rejected alternatives

| Alternative | Why it lost |
| --- | --- |
| jiff as a required dependency | Users who only parse would pay for date and time zone code they never call, and parsing would read the host's zoneinfo. |
| `chrono` with `chrono-tz` | Two crates, and the time zone database is always compiled into the binary. It does not read the system zoneinfo, so results can drift from the host's systemd and tzdata. |
| `time` | No IANA time zones. |
| Own TZif reader | The crate would own path handling for untrusted names, symbolic links, `TZDIR`, file size limits, a binary format, and the POSIX TZ rule strings that decide every result after 2037. A bug there gives silently wrong instants, not a panic. Windows would need a bundled database and its updates. |
| A time zone trait in the core with adapter features | Ten or more public types, one calculation path per adapter, and a test matrix per adapter. The current shape can still move there later, because the search needs only four operations from jiff. |
| Raw integer instants and a zone name string in the API | Each call would look the name up again or need a cache. Callers could not pass a zone they already hold, such as a fixed offset. |
| A syntax tree plus a separate `normalize` step | Two sets of types, and a path to compute on a model that is not normalized. None of the three tasks needs the input spelling. |

## Module boundaries

```
src/lib.rs     crate docs and re-exports, no logic
src/error.rs   ParseError and its private kinds
src/model.rs   the types above, their constructors, pure queries such as Component::next_at_or_after
src/parse.rs   &str to CalendarEvent, the FromStr impl
src/format.rs  CalendarEvent to the normalized string, the Display impl
src/zone.rs    (feature jiff) Zone to jiff::tz::TimeZone, UnknownTimeZone
src/next.rs    (feature jiff) upcoming, Upcoming, the search
```

| Module | Owns | Does not know |
| --- | --- | --- |
| `model` | That only valid, normalized values exist. End value shrinking, folding, sorting, deduplication, the item cap. Pure queries such as "the first value at or after `x`". | Text, jiff, time zones |
| `parse` | All grammar. Whitespace rules, weekday names, two-digit years, shorthands, fraction rounding, filling the date from the right, splitting off the zone token, the zone name format check. Byte offsets for errors. | jiff, the search, `format` |
| `format` | All output rules. Zero padding, weekday runs, fraction printing, the zone suffix. | Grammar, jiff, `parse` |
| `zone` | Turning `Zone` into a `jiff::tz::TimeZone`. The exact-case name check. | Text other than the zone name, the search |
| `next` | The civil time search and the daylight saving time rules. | Text |

Rules at the boundaries:

- Validation happens once, in `parse` and the `model` constructors. `format` and `next` trust the model and do not check it again.
- `parse` and `format` never call each other. The model is the only thing they share.
- `model`, `parse`, and `format` never import jiff. `zone` and `next` are the only modules that do, so the zero-dependency core is enforced by the module graph.

## Normalization and output rules

Each rule below comes with the observed input and the output systemd printed. The fixture rows in `tests/fixtures/expected.tsv` repeat many of them.

### Shorthands

Shorthands match in any case.

| Input | Normalized |
| --- | --- |
| `minutely` | `*-*-* *:*:00` |
| `hourly` | `*-*-* *:00:00` |
| `daily` | `*-*-* 00:00:00` |
| `weekly` | `Mon *-*-* 00:00:00` |
| `monthly` | `*-*-01 00:00:00` |
| `yearly`, `annually` | `*-01-01 00:00:00` |
| `quarterly` | `*-01,04,07,10-01 00:00:00` |
| `semiannually` | `*-01,07-01 00:00:00` |
| `daily UTC` | `*-*-* 00:00:00 UTC` |
| `weekly Pacific/Auckland` | `Mon *-*-* 00:00:00 Pacific/Auckland` |

`Mon weekly` is rejected.

### Weekdays

The weekday list is a set. The output starts at Monday. A run of three or more consecutive days prints as `A..B`, and shorter runs print as a comma list. All seven days print as nothing.

| Input | Normalized |
| --- | --- |
| `Sat,Thu,Mon..Wed,Sat..Sun` | `Mon..Thu,Sat,Sun *-*-* 00:00:00` |
| `Mon,Tue,Wed` | `Mon..Wed *-*-* 00:00:00` |
| `Mon,Tue` | `Mon,Tue *-*-* 00:00:00` |
| `Mon..Sun` | `*-*-* 00:00:00` |
| `Mon-Fri` | `Mon..Fri *-*-* 00:00:00` |
| `monday *-12-* 17:00` | `Mon *-12-* 17:00:00` |
| `Wed..Wed,Wed *-1` | `Wed *-*-01 00:00:00` |
| `Mon,` | `Mon *-*-* 00:00:00` |
| `Wed, 17:48` | `Wed *-*-* 17:48:00` |

Rejected: ranges that wrap past Sunday (`Fri..Mon`, `Tue..Mon`, `Sun..Tue`), `Mon,,Fri`, `Xyz *-*-*`, and `Wedn`. A weekday list of 257 items is accepted, so the weekday list has no item cap.

- A weekday is its three-letter abbreviation or its full English name, in any case (`tHu`, `THURSDAY`). `Tues`, `Thur`, `Thurs`, `Wedn`, and `Mo` are rejected.
- `-` and `..` both separate a range, also between full names (`Monday..Friday`, `mon-FRI`). A one-day range such as `Mon-Mon` is that day. A chain such as `Mon-Wed-Fri` or `Mon..Wed..Fri` is rejected, and so are `Mon.Tue`, `Mon..`, and a repetition such as `Mon..Fri/2`.
- The weekday word may end in one comma (`Mon,`, `Mon,Tue,`). The list cannot continue in the next word, so `Mon, Tue 10:00` is rejected, and the next part must follow after a space, so `Mon,*-*-*` is rejected while `Mon, *-*-*` and `Mon, UTC` are accepted. `Mon,,` and `,Mon` are rejected.

### Dates

The parser fills a short date from the right: day, then month, then year.

| Input | Normalized |
| --- | --- |
| `10-15` | `*-10-15 00:00:00` |
| `Wed *-1` | `Wed *-*-01 00:00:00` |
| `*-*` | `*-*-* 00:00:00` |
| `*~01` | `*-*~01 00:00:00` |
| `*-*~*` | `*-*-* 00:00:00` |
| `12-10-15 1:2:3` with weekdays `Wed..Sat,Tue` | `Tue..Sat 2012-10-15 01:02:03` |
| `69-01-01` | `2069-01-01 00:00:00` |
| `70-01-01` | `1970-01-01 00:00:00` |
| `2026..2028-01-01` | `2026..2028-01-01 00:00:00` |

- `*` alone is rejected.
- Two-digit years 00 to 69 mean 20xx, and 70 to 99 mean 19xx. Three-digit years such as `100` are rejected.
- Years run from 1970 to 2199. `1969` and `2200` are rejected.
- Days counted from the end (`~`) run from 1 to 28. `~0`, `~29`, `~31`, and `*-02~29` are rejected.
- A repetition counted from the end moves toward the end of the month. `*-*~03/2` elapses on January 29 and 31. `~v/r` needs `v > r`, so `~01/1`, `~02/2`, and `~1/2` are rejected, and `~28/27` is accepted. A stepped range counted from the end, `~a..b/r`, holds `a, a + r, ...` up to `b`.
- A list of days from the end has a limit that falls with each item. Sort the list, remove duplicates, and shrink the range ends. Item `i`, counted from 0, may reach at most `28 - 3i`. The value an item reaches is `v` for `v` and `v/r`, and `b` for `a..b` and `a..b/r`. So one item may reach 28, two items 25, three items 22, and a list of eight distinct days is always rejected. `~1,25` and `~1..26,2` (sorted `01..26,02`) are accepted. `~1,26`, `~1,1..26`, and `~1..28,1` (sorted `01,01..28`) are rejected. `~03..26/2,1` is accepted because the end shrinks to 25 before the check, and `~1,2,22,22` is accepted because the duplicate goes first. The month and year fields do not change the limit (`*-01~1,26` is rejected). Observed over all 378 pairs and all 3,276 triples of distinct values.
- A token is a date when it holds no `:`. It needs a `-` or a `~`, so `12` and `2026` alone are rejected. `~01` and `2026~01` are rejected, the second because 2026 is read as the month. `*~*` is `*-*-*`.
- Two-digit years expand by value in every value position: list items, range ends, and the start of a repetition, never the step. `12..14` is 2012..2014, `0..1` is 2000..2001, `99..2000` is 1999..2000, and `12/187` is 2012/187. `69..70` becomes 2069..1970 and is rejected as reversed.

### Times and seconds

| Input | Normalized |
| --- | --- |
| `05:40` | `*-*-* 05:40:00` |
| `12,14,13,12:20,10,30` | `*-*-* 12,13,14:10,20,30:00` |
| `*:*:*` | `*-*-* *:*:*` |
| `*:*:0/1` | `*-*-* *:*:*` |
| `*:0/1` | `*-*-* *:00/1:00` |
| `*:*:59.9999994` | `*-*-* *:*:59.999999` |
| `*:*:0.5/0.25` | `*-*-* *:*:00.500000/0.250000` |
| `05:40:23.4200004/3.1700005` | `*-*-* 05:40:23.420000/3.170001` |
| `00:00:10..20/5.5` | `*-*-* 00:00:10..15.500000/5.500000` |
| `*:*:00/0.0000005` | `*-*-* *:*:00/0.000001` |

- Fractions round to six digits, and the range check runs after rounding. `59.9999999` rounds to 60 and is rejected. `00/0.0000001` rounds the step to 0 and is rejected. A fraction needs digits on both sides of the `.`: `1.` and `.5` are rejected. Only the seconds field takes fractions (`*:5.5` is rejected).
- `0/1` becomes `*` in the seconds field only, and it turns the whole component into `*`: `*:*:0/1,5` prints as `*:*:*`. In the minute, hour, day, month, and year fields it stays (`*:00/1`, `00/1:00`, `*-*-01/1`, `*-01/1-01`, `1970/1-01-01`). `*:*:1/1` stays too.
- Out of range: `24:00`, `23:59:60`, `*:60`, `*-*-* 25:00`.
- A seconds range without a repetition steps by whole seconds and must reach at least two values, so its end must be at least one second after its start. That is why `*:*:0..0.5` is rejected. `*:*:1..1`, `*:*:0.5..1`, and `*:*:1.5..2.4` are rejected too, while `*:*:0..1`, `*:*:1.5..2.5`, and `*:*:0.5..2` (printed `00.500000..01.500000`) are accepted. With an explicit repetition, one value is fine: `*:*:1..1/1` and `*:*:1..2/1.5` print as `*:*:01`. The minute and other fields have no such rule: `3..3` is `03`.
- `v/r` in seconds needs `v + r` below 60 s: `58.5/1.4` is accepted, and `58.5/1.5` and `0.5/59.5` are rejected.
- A repetition of exactly one unit prints as a plain range in every field: `*-*-1..31/1` prints as `*-*-01..31`, and `*:*:0..59/1` as `*:*:00..59`.

### Lists, ranges, and repetitions

| Input | Normalized |
| --- | --- |
| `*:5,3,5,1,3` | `*-*-* *:01,03,05:00` |
| `*-*-1,1` | `*-*-01 00:00:00` |
| `*-*-3..5,1` | `*-*-01,03..05 00:00:00` |
| `*:1..3,2..4` | `*-*-* *:01..03,02..04:00` |
| `*:0/20,0/30` | `*-*-* *:00/20,00/30:00` |
| `*-*-1..5/2,1..5/3` | `*-*-01..04/3,01..05/2 00:00:00` |
| `*-*-1..5/3` | `*-*-01..04/3 00:00:00` |
| `*-*-1..3/5` | `*-*-01 00:00:00` |
| `3..3` in the minutes field | `03` |
| `*:2/3` | `*-*-* *:02/3:00` |
| `Mon *-05~07/1` | `Mon *-05~07/1 00:00:00` |

- Lists are sorted, and exact duplicates are removed. Overlapping items are not merged.
- A range end shrinks to the last value the step reaches. A range with one value becomes that value. A reversed range such as `5..3` is rejected.
- Only the end after shrinking must lie inside the field (observed). `*:10..65/20` prints as `*:10..50/20`, `*-*-1..32/30` as `*-*-01..31/30`, and `*:*:0.5..60` as `*:*:00.500000..59.500000`. `*:10..70/20` and `*:*:0.5..61` are rejected, because their steps reach 70 and 60.5.
- `v/r` needs `r >= 1` and `v + r <= max`. `*:0/59` and `*-*-1/30` are accepted. `*:30/59`, `*:0/60`, `*-*-1/31`, `2026/200`, and any `/0` are rejected.
- `*` cannot take a repetition or join a list. `*/5`, `*/1`, `*-*/2-01`, and `*,1` are rejected.
- `*:0..59` stays `00..59`. It does not become `*`.

### Time zones

| Input | Result |
| --- | --- |
| `daily utc` | `*-*-* 00:00:00 UTC` |
| `daily Etc/GMT+5` | `*-*-* 00:00:00 Etc/GMT+5` |
| `daily GMT` | accepted, printed as `GMT` |
| `daily asia/seoul`, `daily Z`, `daily Nowhere/Bogus` | systemd rejects. This crate parses and `upcoming` returns `UnknownTimeZone`. |
| `daily +09:00` | rejected by both. `:` is not allowed in a zone name. |

Only `UTC` matches in any case. Other names keep the case as written, and `zone` accepts a name only when jiff's canonical name for the zone is exactly the token.

The parser finds the zone this way. It first reads the whole input as an expression with no zone. When that fails, and the input has a last word after a single space that passes the [name check](#time-zone-names), it reads the rest as the expression and the last word as the zone. The first attempt matters because a date such as `10-15` also has the form of a zone name. The rest must still be a valid expression, so `UTC` alone, `weekly  UTC` (a shorthand followed by a space), and `*-*-* 12:00 UTC UTC` are rejected. See [Whitespace](#whitespace) for spaces before the zone. A last word that only looks like a zone, as in `Mon Mon`, `daily Mon`, or `10:00 Mon`, parses as a zone name, and `upcoming` then returns `UnknownTimeZone`. systemd rejects these at parse time, which is the known difference for unknown names.

Shorthands stand alone. `daily 10:00` and `Mon daily` are rejected, and only a zone may follow a shorthand.

### Whitespace

| Input | Result |
| --- | --- |
| `Mon   *-*-*   10:00` | `Mon *-*-* 10:00:00` |
| `Mon  10:00` | `Mon *-*-* 10:00:00` |
| ` Mon *-*-* 10:00` | rejected, leading space |
| `Mon *-*-* 10:00 `, `daily `, `*-*-* UTC ` | rejected, trailing space after a time, a shorthand, or a zone |
| `*-*-* 10:00  UTC`, `daily  UTC` | rejected, more than one space between a time or a shorthand and the zone |
| `*-*-* `, `Mon `, `Mon, `, `2026-01-01  ` | accepted, trailing spaces after a date or weekdays with no time |
| `*-*-*  UTC`, `Mon  UTC` | accepted, a run of spaces between a date or weekdays with no time and the zone |
| a `T` between the date and the time | rejected |
| empty string | rejected |

### Output rules

`format` follows these rules and nothing else:

- Parts are separated by one space. A weekday set of all seven days prints as nothing.
- Weekdays start at Monday. Runs of three or more days print as `A..B`, and other days print as a comma list.
- The year has four digits. Month, day, hour, and minute have two digits, padded with zeros. Repetition values are not padded (`00/15`, `01/2`, `02/3`).
- The integer part of a second has two digits. When the microsecond part is not zero, a `.` and six digits follow. A repetition of seconds uses the same rule with an unpadded integer part (`0.250000`, `3.170001`, `5.500000`). Each value in a range gets its own fraction (`10..15.500000`).
- The day separator is `-` for days from the start of the month and `~` for days from the end.
- `Zone::Local` prints nothing, `Zone::Utc` prints ` UTC`, and `Zone::Named` prints a space and the name.

## Next elapse algorithm

The search runs on civil time in the effective zone `Z`. `Z` is the zone named in the expression, or `local` when the expression names none. Weekdays and dates are evaluated in `Z`. For example, `weekly Pacific/Auckland` elapses on Sundays at 11:00 UTC, which is Monday 00:00 in Auckland (observed).

```
next(after):
	after  = after rounded down to a whole microsecond
	cursor = civil(after + 1 µs, Z)
	loop:
		c = first_match_at_or_after(cursor)     # None when the years run out
		match Z.to_ambiguous_timestamp(c):
			unambiguous t -> return t
			fold (t1, t2) -> return the earliest of t1, t2 that is later than after
			gap           -> cursor = civil time at the end of the gap; keep the floor; continue
```

`Upcoming` calls `next` again with the previous result as `after`. After the first `None`, it keeps returning `None`.

Edge cases:

- When `after` is before 1970, the cursor starts at 1970-01-01 00:00:00.
- The last possible elapse is 2199-12-31 23:59:59.999999. systemd elapses at 2199-12-31 23:59:59 (observed). Later than that, `next` returns `None`.
- Checked arithmetic that overflows near the ends of `jiff::Timestamp` returns `None`.

### Civil search

`first_match_at_or_after(cursor)` walks from the largest field to the smallest and carries upward. Its state is one tuple: year, month, day, hour, minute, microsecond.

1. Year. Take the first year at or after the current one. If none, return `None`. If the year moved, reset every smaller field to its minimum.
2. Month. Same rule. If none, add one year and go back to step 1.
3. Day. Build the matching days of that month as a 31-bit mask, with no allocation. Days from the start keep values up to the month length `L`. Days from the end map `k` to `L - k + 1`. Drop days whose weekday is not in the weekday set. Take the first day at or after the current one. If none, add one month and go back to step 2.
4. Hour, minute, second. Same rule. If none, add one to the next larger field and go back to its step.

One exception in step 4 follows systemd (observed). When a time field runs out at a value `x` inside its range and the carry also runs past the end of the next larger field, the new period does not start at the field's minimum. It starts at a floor:

- The seconds field runs out in minute 59 (the carry wraps the hour), the minute field runs out in hour 23 (the carry wraps the day), or the hour field runs out on the last day of the month (the carry wraps the month).
- For each `v/r` item, take the first `v + k·r` at or after `x` with no upper bound. The smallest of them, minus the field length (60 or 24), is the floor. With no `v/r` item there is no floor.
- The period starts at the first value at or after the floor. The floor holds until a larger field changes in the search.

| Expression | Base (UTC) | First elapse |
| --- | --- | --- |
| `*:0/24` | 2026-01-10 23:50 | 2026-01-11 00:24, not 00:00 (72 − 60 = 12) |
| `*:0/24` | 2026-01-10 22:50 | 23:00. The carry does not wrap the day. |
| `*:0/24,10/25` | 2026-01-10 23:50 | 00:00 (60 − 60 = 0) |
| `*:*:0/7` | 10:59:57 | 11:00:07 |
| `*:*:0/7` | 10:20:57 | 10:21:00 |
| `0/5:00` | 2026-01-31 20:30 | 2026-02-01 05:00 |

The day and month fields have no such floor: `*-*-5/12` from 2026-12-30 elapses at 2027-01-05.

Two more rules apply when the zone has daylight saving time. Both were fitted to the oracle and then checked on 15,000 further random cases, nine in ten of them in one of 15 zones with daylight saving time and most of those with a base time near a transition. The differences that remain are listed in [Known differences](#known-differences-from-systemd).

- The floor holds when the search restarts at the end of a gap. In `America/New_York`, `*:*:0/7` from 2026-03-08 01:59:57 EST elapses at 03:00:07 EDT.
- A seconds floor set by the search is dropped when the clock is set back after `after` and no later than the instant found. The search then restarts at the start of that minute. In `America/New_York`, `*:*:0/7` from 2026-11-01 01:59:57 EDT elapses at 02:00:00 EST, and `*:*:3/11` at 01:59:58 EDT and then 02:00:03 EST.

`Component::next_at_or_after(x)` computes the answer for each item with arithmetic and takes the minimum. Every field uses this one function. For seconds, `Any` means "the next whole second". `*:*:*` elapses on whole seconds (observed).

The weekday list and the date both have to match. `Wed *-1` elapses only on the first of a month that falls on a Wednesday: 2026-04-01, 2026-07-01, 2027-09-01 (observed).

### Daylight saving time

The rule is the same whether `Z` came from the expression or from `local`.

A civil time that does not exist, in a gap, is skipped. The search does not shift it forward. The cursor jumps to the civil time at the end of the gap, which is the transition instant read in `Z`. A floor from the [civil search](#civil-search) stays in force there. The transition instant comes from `TimeZone::preceding`. Observed with `TZ=America/New_York`:

| Expression | Base | Elapses |
| --- | --- | --- |
| `*-03-08 02:30` | 2026-01-01 00:00 UTC | 2027-03-08 07:30 UTC first, so 2026-03-08 is skipped |
| `*-*-* 02:30` | 2026-03-07 | 2026-03-09 02:30 EDT, so 2026-03-08 is skipped |
| `*:30` | spring transition | 01:30 EST, then 03:30 EDT |

A civil time that occurs twice, in a fold, elapses once, at the earliest instant that is later than `after`. Observed with `TZ=America/New_York`:

| Expression | Base | Elapses |
| --- | --- | --- |
| `*-11-01 01:30` | 2026-01-01 00:00 UTC | 2026-11-01 05:30 UTC (EDT) only |
| `*-11-01 *:30` | 2026-01-01 00:00 UTC | 04:30, 05:30, 07:30 UTC. 06:30 UTC is missing. |
| `*:30` | 2026-11-01 05:15 UTC (01:15 EDT) | 05:30, 07:30, 08:30 UTC |
| `*:30` | 2026-11-01 06:15 UTC (01:15 EST, second pass) | 06:30, 07:30, 08:30 UTC |

The last row shows why the fold rule picks "the earliest instant later than `after`" and does not move the cursor to the end of the fold. Moving the cursor would skip 06:30 UTC, which systemd prints.

The fold rule always returns an instant later than `after`. Every candidate `c` is at or after `civil(after + 1 µs)`. If `after + 1 µs` is in the second pass of the fold, the later instant of `c` is at or after it. Otherwise the earlier instant of `c` is.

The cursor starts at the civil time of `after + 1 µs`, not at the civil time of `after` plus one microsecond. The two differ only when `after` is the last microsecond before a fold, where the clock jumps back. systemd keeps searching inside the repeated hour in that case (observed):

| Zone | Expression | Base | First elapse |
| --- | --- | --- | --- |
| `Australia/Lord_Howe` | `*:*:31` | 2026-04-04 14:59:59.999999 UTC | 15:00:31 UTC |
| `Europe/London` | `*:*:22` | 2026-10-25 00:59:59.999999 UTC | 01:00:22 UTC |
| `Europe/London` | `*:*:22` | 2026-10-25 00:59:59 UTC | 02:00:22 UTC |
| `America/New_York` | `*:*:00` | 2026-11-01 05:59:59.999999 UTC | 06:00:00 UTC |

### Termination

Each `next` call ends, for every input:

- Inside one call, the cursor strictly increases at every step. A gap moves it to the end of the gap, which is later than the candidate. A carry moves it to the next value of a larger field. Dropping a seconds floor after the clock was set back moves it to the start of the minute that was carried into, which is later than the cursor, and the search from there sets no seconds floor, so this happens at most once in a row.
- The year never passes 2199, so the cursor cannot increase forever.
- The search never steps through a gap one microsecond at a time. That matters for `*:*:0/0.000001`, which systemd accepts. Stepping through a one-hour gap at 1 µs would take 3.6 billion steps in one call.

The iterator as a whole is finite, because its results strictly increase and stop at 2199. It can still be very long. `*:*:0/0.000001` yields one million instants per second, about 5.5 × 10^15 before 2199 (estimate). Callers must bound it with `take`, and tests always do.

### Step bound

Hand-computed bounds for one `next` call, counting carries and gap restarts (estimate):

- Year carries: at most 230.
- Month steps: at most 230 × 12 = 2,760, counted from 1970.
- Day steps: one per month visited.
- Gap restarts: at most 6 × G, where G is the number of gaps in `Z` between `after` and 2199. A zone with one spring transition a year has G of at most 230.

The total is about 3,000 + 6 × G, under 5,000. Each step evaluates at most 241 items per field, so the worst case is about 1.2 million item evaluations per call.

Worst inputs found so far:

| Input | Why it is slow | Steps |
| --- | --- | --- |
| `*-2,4,6,9,11-31` | Never elapses. Five months a year have no day 31, so each year costs five month carries and one year carry. | about 1,050 |
| `Sun *-03-08..14 02:30 America/New_York` | Never elapses. Every candidate falls in the spring gap. systemd gives up on this input (see Known differences). | 173 gap restarts of about 6 steps |
| `*-02-30` | Never elapses. February never has day 30. | about 174 years of carries |

`next` also carries a fixed fuel limit of 20,000 steps per call. When the fuel runs out, `next` returns `None`. The proof above says the limit is never reached. The fuel only turns a future bug in the gap end computation into a wrong `None` instead of an endless loop. A unit test in `src/next.rs` runs every fixture expression from three base times (2026, 2190, and 1 µs before the New York fall transition) and asserts that no search runs out of fuel. The most steps one call took there was 698 (measured).

### Lints and impossible branches

The lints in `Cargo.toml` deny `unwrap`, `expect`, `panic!`, `unreachable!`, indexing, string slicing, unchecked arithmetic, and lossy `as` casts. The search follows three rules to fit:

- Search functions return `Option` from the start and chain with `?`.
- A branch that cannot happen, such as a failed conversion of a month that is already in range, returns `None`. It is never a panic.
- Every `start + k * step` uses `checked_mul` and `checked_add`. Conversions between jiff's integer types and `u32` use `try_from`.

## Security model

The expression is untrusted. It can come from a web form, a configuration file written by another tenant, or a fuzzer. The caller controls `after` and `local`, but the tests still cover their extreme values.

The crate promises:

- No panic on any input. `unsafe_code` is `forbid`, and the panic lints are `deny`. The lints catch the common panics and not all of them (`assert!` and `split_at` pass), so the robustness tests carry the rest. `scripts/check-security.sh` fails when `src/` switches a lint off.
- Every `parse` and every `next` ends. Parse time is linear in the input length.
- Memory does not grow with the input. Each numeric field keeps at most 241 items.
- Parsing reads no file, no environment variable, and no other process state.
- Error messages never repeat the input.

### Limits

| Limit | Value | Reason |
| --- | --- | --- |
| Items in one numeric field | 241 | systemd accepts 241 and rejects 242 (observed for minutes and days). The parser stops at item 242 and does not collect the rest first. Items count before deduplication: 240 copies of `0..59` are accepted, and 250 are rejected (observed). |
| Items in the weekday list | none | The weekday set is seven bits, so more items cost no memory. systemd accepts 257 items (observed). |
| Digits in a number | none | Accumulation is checked. A value above 2,147,483,647 (2^31 − 1) is rejected, as systemd rejects it (observed): `*:0..10/2147483647` is accepted and `*:0..10/2147483648` is rejected. A seconds value counts in microseconds for this limit, so `2147.483647` is accepted and `2147.483648` is rejected. |
| Digits in a fraction | none | Only the first seven digits feed the rounding. The rest are checked to be digits. |
| Time zone name length | 64 bytes | Longer than any IANA name. Stops a large token from reaching the lookup. |
| Steps in one `next` call | 20,000 | A safety net above the proven bound of about 5,000. |

There is no limit on the input length. With the item cap, memory is bounded, and the parser is linear in time. systemd shows no length limit either: it accepts 1,000 weekday items, 3,000 spaces between parts, and 3,000 leading zeros (observed).

The item cap applies per field. An expression with 300 items split between the minutes and seconds fields, each field under the cap, is accepted by systemd (observed).

### Error messages

`ParseError`'s `Display` prints a fixed message for the error kind and the byte offset, for example `calendar expression: hour out of range at byte 6`. It never includes the input text. `UnknownTimeZone` prints a fixed message that also says local abbreviations such as `KST` are not supported, so a user who migrates a timer unit sees the cause.

The reason is log injection. Callers often log `err.to_string()`. If the message carried the input, an attacker could put newlines that forge log lines, or ANSI escape sequences that rewrite a terminal, into the caller's logs. systemd itself echoes the input in its error message (observed), and this crate does not copy that. A caller that wants to show the input can use `offset()` and apply its own escaping.

Messages are printable ASCII with a fixed maximum length. A list of days from the end that breaks the limit under [Dates](#dates) has its own message. `offset()` is at most the input length and always on a character boundary.

### Time zone names

The parser checks the zone token before any lookup can see it:

- Only ASCII letters, digits, `_`, `+`, `-`, and `/` are allowed. Since `.` is not allowed, a `.` or `..` path component cannot appear.
- At most 64 bytes.
- No leading `/` and no empty component, so `Asia//Seoul` and `Asia/Seoul/` are rejected.

jiff itself rejects `../../etc/passwd` and `/etc/passwd` as names that are not in the database (measured). The crate's own check is the first layer. It keeps the protection inside this crate if a future jiff version changes, and it costs a few comparisons.

`upcoming` turns `Zone::Named` into a `TimeZone` through `jiff::tz::TimeZone::get`. jiff's lookup ignores case, so `zone` compares the zone's canonical name from `iana_name()` with the token and rejects a mismatch such as `asia/seoul`.

`upcoming` uses jiff's global database. On Unix that database reads the directory in the `TZDIR` environment variable when it is set, and otherwise a system directory such as `/usr/share/zoneinfo`. So `upcoming` with a named zone reads the file system, through jiff. `upcoming_with` uses the database the caller passes, and with `TimeZoneDatabase::bundled()` it reads no file. `parse`, and both functions with `Zone::Local` or `Zone::Utc`, do not touch a database.

The crate's own code never reads the environment. `clippy.toml` lists `std::env::var`, `std::env::var_os`, `std::env::vars`, `std::env::vars_os`, `TimeZone::system`, and `TimeZone::try_system` under `disallowed-methods`, and `scripts/check-security.sh` runs clippy with warnings denied, so a call to any of them fails the gate. The two `TimeZone` entries only matter when jiff's `tz-system` feature is on somewhere in the build. A test cannot check this, because `std::env::set_var` is `unsafe` in edition 2024 and the tests forbid `unsafe` code too.

## Weight budget

| Build | Budget |
| --- | --- |
| `default-features = false` | 0 extra crates. About 0.3 s cold release build (measured on the baseline prototype). |
| Default features | Only the crates that jiff pulls in with `std`, `tzdb-zoneinfo`, and `tzdb-bundle-platform`. About 226 KB of stripped executable over the baseline (measured with `std` and `tzdb-zoneinfo`). |

`scripts/check-weight.sh` gates the number of crates in the normal dependency graph and reports the cold build time and the rlib size. Build time and size are reported, not gated, because they depend on the machine. The script counts two configurations. The limit is 0 without default features and 2 with them (`jiff` and `jiff-core`, as resolved on a Unix host). A failing `cargo tree` fails the gate.

## Verification

### The oracle

`oracle/` runs the real `systemd-analyze calendar` in a Docker image built from `debian:trixie-slim`. The base image is pinned by digest and the `systemd` and `tzdata` packages by version, so a rebuild gets the same builds or fails. `oracle/check.sh` compares the build line of `expected.tsv` too.

- `oracle/run.sh [ITERATIONS] < cases.tsv > expected.tsv` builds the image and runs it with `--network none`. The cases go in on standard input, so the script works where Docker cannot mount volumes.
- `oracle/query.pl` runs one `systemd-analyze calendar --iterations=N --base-time=<base>` per case, with `TZ` set to the case's time zone.
- `oracle/check.sh` regenerates the expectations and fails when they differ from the committed `tests/fixtures/expected.tsv`. It ignores the first line, which names the systemd build.

`cargo test` does not need Docker. It reads only the committed files.

### Fixture format

`tests/fixtures/cases.tsv` has three tab-separated columns:

```
<timezone>	<base time>	<expression>
```

`tests/fixtures/expected.tsv` repeats them and adds three:

```
<timezone>	<base time>	<expression>	<status>	<normalized>	<elapses>
```

- `status` is `ok`, `error`, or `unresolved`. `error` means systemd rejected the expression. `unresolved` means systemd parsed it and then gave up computing an elapse.
- `normalized` is the normalized form, or `-` when systemd printed none.
- `elapses` is a comma list of UTC instants in RFC 3339 at second precision, `never`, or `-` when the status is not `ok`.

Tabs separate the columns because an expression cannot contain a tab. Empty lines and lines that start with `#` pass through. The first line of `expected.tsv` names the systemd build and the iteration count, for example `# oracle: systemd 257 (257.13-1~deb13u1), iterations=5`.

### Comparing against the fixtures

`tests/oracle.rs` reads `expected.tsv` and checks each row through the public API only:

- `status` is `ok` exactly when `parse` succeeds and `upcoming_with(base, &local, &database)` returns `Ok`, where `database` is jiff's bundled database and the test reads `local` from it.
- `normalized` equals `event.to_string()`.
- `never` means the first `next()` is `None`. Otherwise the first N instants, cut down to whole seconds, equal the listed instants.
- An `unresolved` row requires only that `parse` succeeds. Its elapses are not compared, because the crate answers where systemd gives up.

systemd prints instants at second precision and drops the fraction. `05:40:23.4200004/3.1700005` elapses at 05:40:23.42, and systemd prints `05:40:23` (observed). To pin sub-second results anyway, fixture rows move the base time instead. systemd accepts a base time such as `2026-01-01 00:00:00.499999 UTC` (observed).

```
*:*:0.5 with base 00:00:00.499999  ->  00:00:00  (the elapse is at 00:00:00.5)
*:*:0.5 with base 00:00:00.5       ->  00:01:00  (00:00:00.5 is no longer after the base)
```

The two rows squeeze the elapse to exactly `.500000` from both sides. The slow reference implementation in the tests works in whole seconds, so sub-second behavior rests on these fixture rows and on property 19.

Fixture inputs use only `UTC` and `Area/City` names. Rows with local abbreviations such as `KST` stay out, because the crate rejects them on purpose.

Error rows check only success or failure, because systemd reports only an error code. A few hand-written tests check `offset()` and the messages.

### Robustness properties

`tests/robustness.rs` uses only the public API and holds these 20 properties. Time limits are generous, about 5 s, so that correct code finishes in milliseconds and a broken search fails by minutes.

`tests/normalize.rs` needs no feature. It checks that every row systemd accepted parses and prints the same normalized form, so the build without dependencies is compared with systemd too.

Parsing:

1. Any UTF-8 string, including multibyte characters at every position, NUL, control characters, and combining characters, gives `Ok` or `Err` without a panic.
2. The same holds for mutations of valid expressions (insert, delete, replace a character) and for random strings over `0-9*,./~:- A-Za-z`.
3. `offset()` is at most the input length and on a character boundary.
4. Error messages are printable ASCII, shorter than a fixed length, and do not contain the input.
5. `4294967296`, 100-digit numbers, 1,000-digit fractions, runs of leading zeros, and huge repetitions are judged without a panic.
6. Pathological inputs of length 1,000, 10,000, 100,000, and 1,000,000 finish inside the time limit. The set covers `,,,,`, `....`, `////`, digits only, long weekday lists, long zone tokens, and `0,0,0,`.
7. 241 items pass and 242 fail, for single values, ranges, and duplicates. 257 weekday items pass.
8. For every accepted input, `to_string().len()` is at most a fixed bound derived from the limits.
9. For every accepted `s`, parsing `s.parse()?.to_string()` gives the same event and the same string.
10. Normalized strings contain only characters from the output alphabet.

Time zone names:

11. `../../etc/passwd`, `/etc/passwd`, `Asia/../../etc/passwd`, `Asia//Seoul`, `./UTC`, `Asia/Seoul/`, `:/etc/localtime`, backslashes, NUL, a 10,000-character name, and Unicode look-alike letters are all rejected.
12. `utc` and `UTC` work. `asia/seoul`, `Z`, and `Nowhere/Bogus` make `upcoming` return `UnknownTimeZone`. `+09:00` fails to parse.

Computation:

13. No panic for the expression corpus crossed with base times (`Timestamp::MIN`, `Timestamp::MAX`, 1970-01-01 ± 1 µs, 2199-12-31T23:59:59.999999 ± 1 µs, 2200-01-01) and zones (UTC, the largest and smallest fixed offsets, `America/New_York`, `Australia/Lord_Howe`, `Pacific/Apia`, `Europe/Dublin`, `Africa/Casablanca`).
14. Results strictly increase, and all are later than the base.
15. Each result, read as civil time in the effective zone, matches every field of the expression. A separate checker in the test decides this.
16. In short windows, results equal those of a slow reference that tests every grid point of the expression. The windows include gaps and folds.
17. Every worst input in [Step bound](#step-bound) finishes inside the time limit, plus `2027-03-14 02:*:0/0.000001 America/New_York`, `Sun *-03-08..14 02:*:0/0.000001 America/New_York`, `*:*:0/0.000001` with a base in the second pass of a fold, and an expression with 241 items in every numeric field.
18. No result falls in a gap. A folded civil time appears once. A base in the second pass of a fold produces no past instant and no duplicate.
19. `*:*:0/0.000001` with `take(10_000)` finishes, and the results are 1 µs apart.
20. After the first `None`, the iterator keeps returning `None`. An expression that never elapses returns `None` first.

### Scripts and CI

- `scripts/check-security.sh` checks that `Cargo.toml` still forbids `unsafe` code, runs clippy with the deny lints, runs `cargo deny check`, and runs `tests/robustness.rs`. It also fails when `src/` switches a lint off, and through `clippy.toml` when library code calls a method that reads the environment or the system time zone.
- `scripts/check-weight.sh` is described under [Weight budget](#weight-budget).
- `scripts/check-no-std.sh` builds the crate without default features for `thumbv7em-none-eabihf`. It fails, rather than skips, when that target is not installed for the active toolchain. Set `RUSTUP_TOOLCHAIN` to run it on another toolchain.
- `scripts/check-features.sh` runs clippy with warnings denied on every feature combination (`cargo hack --feature-powerset`).
- `scripts/check-minimal-versions.sh` resolves every dependency to its lowest allowed version (`cargo minimal-versions`) and runs the tests with all features. `Cargo.lock` is restored afterwards.
- `scripts/check-package.sh` compares `cargo package --list` with an allow list in the script and then builds the packaged crate.
- `scripts/check-workflows.sh` runs `actionlint` and `zizmor` on `.github/`.
- `scripts/fuzz.sh <seconds> [target...]` runs each `cargo-fuzz` target in `fuzz/` for the given time, seeded with the expressions in `tests/fixtures/cases.tsv`.
- Each script fails, rather than skips, when the tool it needs is missing, and prints how to install it.
- `deny.toml` allows only the MIT license, which every dependency offers, and bans duplicate crate versions.
- `.github/workflows/ci.yml` runs on every push to `main` and every pull request:
  - `lint` (Linux): format, the comment check, the feature powerset, docs, the `no_std` build, and the package check.
  - `test`: the tests in both feature configurations on Linux, macOS, and Windows.
  - `msrv` (Linux): check, the `no_std` build, and the minimal-versions tests on 1.85.
  - `security`, `weight`, `oracle`, and `workflows` (Linux).
  - `fuzz` (Linux): 60 seconds per fuzz target.
- `.github/workflows/weekly.yml` runs on Monday mornings (UTC) and on demand, even when no code changed: `cargo deny check advisories`, a fresh build of the oracle image with `oracle/check.sh`, and 30 minutes per fuzz target.
- `.github/dependabot.yml` asks for weekly updates of `Cargo.lock` and of the pinned actions.

What runs where is decided by running time. `verify gates` holds the checks that finish in seconds on a warm cache: the feature powerset, minimal versions, the package check, and the workflow check each took between 0.2 and 1.9 s (measured), and the whole of `verify gates` took about 15 s. The operating system matrix needs other machines, and fuzzing needs minutes to be worth running, so both are in CI only. Run `scripts/fuzz.sh 60` by hand after changing the parser or the search.

### Tooling notes

The repository carries no code comments (see `CLAUDE.md`), so the reasons behind the tooling live here.

- `oracle/Dockerfile` pins the base image by digest and `systemd` and `tzdata` by version. When Debian retires those versions the build fails. Raise the versions and regenerate `tests/fixtures/expected.tsv` in the same change.
- `oracle/query.pl` treats the two systemd failures differently and stops on any failure text it does not recognize, so an unknown failure cannot pass as `error` or `unresolved`.
- `Cargo.toml` uses an `include` allow list, so a stray file in a checkout cannot reach crates.io.
- The dev-dependency on jiff turns on `tzdb-bundle-always`. The tests, the oracle comparison, and the verification probe read their zones from the bundled database and pass it to `upcoming_with`, so a zone named inside an expression is looked up there too. Their results do not depend on the host's tzdata.
- `scripts/check-weight.sh` stops when `cargo tree` fails, so an empty graph cannot read as zero dependencies. Its default-feature limit of 2 is for a Unix host. Other targets pull jiff's bundled database crates.
- CI pins actions by commit so that a moved tag cannot change what runs. The pins are `actions/checkout` v7.0.1 and `EmbarkStudios/cargo-deny-action` v2.1.3. The `msrv` job must use the `rust-version` from `Cargo.toml`. The security job passes `--skip-deny` because the cargo-deny action runs there. A Dependabot update of a pin must update this list too.
- The pins carry no version comment, because configuration carries no comments. `zizmor --persona pedantic` reports that as `ref-version-mismatch`. The default persona reports nothing.
- CI installs its tools at fixed versions instead of through another action: `cargo-hack` 0.6.46, `cargo-minimal-versions` 0.1.39, and `cargo-fuzz` 0.13.2 with `cargo install --locked`, `actionlint` 1.7.12 with `go install`, and `zizmor` 1.33.1 with `pipx`. These are the versions the checks were first run with. The `workflows` job passes the job token to `zizmor` so that its online audits run.
- Shell scripts, Docker, and the weight limit stay on Linux. The weight limit of 2 holds only for a Unix host, and the oracle needs Docker. On Windows, jiff finds no system zoneinfo and uses the database that `tzdb-bundle-platform` compiles in. The tests do not notice, because they read the bundled database. They also pass with every fixture and source file converted to CRLF line endings, as a Windows checkout may do (measured on macOS).
- `fuzz/` is a separate crate with its own `[workspace]`, so it is not a member of this crate's build, `cargo tree` does not count its dependencies, and the `include` list keeps it out of the package. `fuzz/Cargo.lock` is committed so that CI fuzzes with the same `libfuzzer-sys`. Dependabot does not update it.
- The fuzz targets: `parse_roundtrip` parses any UTF-8 input and, when it is accepted, checks that the normalized form parses to the same event and prints the same text. `upcoming` takes a base time, a local zone from a short list of zones with daylight saving changes plus the extreme fixed offsets, and an expression, calls `upcoming_with` with the bundled database, and checks that up to 16 results each lie strictly after the one before, starting from the base time. A base time of zero bytes means 2026-01-01 UTC, so the seeds from the fixtures start in the years the fixtures use. Each input has 10 s before libFuzzer reports it as a hang.
- A failing fuzz input is saved under `fuzz/artifacts/<target>/`. libFuzzer also prints it in base64 in the log, which is how a CI failure can be reproduced: decode it to a file and run `cargo +nightly fuzz run <target> <file>`.
- The slow reference in `tests/robustness.rs` can be checked against systemd. Run `cargo test --test robustness -- --ignored --nocapture print_generated > gen.tsv`, then `grep -v '^[a-z]' gen.tsv | cut -f1-3 | oracle/run.sh > gen.expected.tsv`. Column 4 of `gen.tsv` must be a prefix of column 6 of `gen.expected.tsv`, and column 4 of `gen.expected.tsv` must be `ok`. The reference gives a day counted from the end one item, because longer lists hit the limit under [Dates](#dates).
- The slow reference in `tests/robustness.rs` tests every grid point, so it cannot express the wrap floor of the [civil search](#civil-search). Its generator gives `v/r` items in the hour, minute, and second fields only steps that end exactly at the field length (`(24 - v) % r == 0` or `(60 - v) % r == 0`), where the floor equals the minimum. The fixtures and the oracle cover the floor.
- In `tests/robustness.rs`, the list-cap test leaves out lists of distinct days from the end. Under the limit in [Dates](#dates), at most seven distinct days from the end fit in one list, so such a list never reaches the item cap.
- The search queries in `model`, such as `Component::next_at_or_after` and `DaySpec::mask`, are compiled only with the `jiff` feature, because only `next` calls them.

## Known differences from systemd

- **Unknown time zone names fail later.** systemd rejects `daily Nowhere/Bogus` at parse time. This crate parses it, and `upcoming` returns `UnknownTimeZone`. A user who only parses sees the expression accepted.
- **No local time zone abbreviations.** systemd accepts `daily KST` only when the process time zone uses `KST`, so its result depends on the environment. This crate cannot match that with `FromStr`. It parses the token as a name, and `upcoming` returns `UnknownTimeZone`.
- **The local zone is an argument.** systemd reads `TZ` and `/etc/localtime`. This crate uses the `local` argument. Pass `&jiff::tz::TimeZone::system()` to match systemd.
- **Named zones depend on the zone database.** Names are looked up in jiff's database, which on Unix is the system copy. In a minimal container without tzdata, every named zone fails with `UnknownTimeZone`. To bundle the database, turn on jiff's `tzdb-bundle-always` feature in your own `Cargo.toml`. Cargo merges the features of one jiff version. `upcoming_with` takes the database as an argument. The oracle image has no `EST`, `UCT`, `Zulu`, or `W-SU` file (measured), so systemd there rejects `*-*-* 10:00 EST`, `UCT`, `Zulu`, and `W-SU`. This crate accepts all four with jiff's bundled database (measured: `EST` at UTC−5, `UCT` and `Zulu` at UTC, `W-SU` at Moscow time), and with a system database that has those files, such as the one on macOS. `MST` and `EST5EDT` behave the same way. In lower case, `zulu` fails in both.

- **The crate answers where systemd gives up.** systemd appears to bound its search (inferred from the failure, not from its code). When every candidate for many years falls in a daylight-saving gap, it fails with "Failed to determine next elapse" instead of answering. `Sun *-03-08..14 02:30 America/New_York` from a 2026 base time does this (observed). From a 2190 base time systemd answers `never` for the same expression (observed), so `never` is the true answer. This crate skips each gap in one step and returns `never` from any base time. systemd also gives up on expressions whose answer is months away and whose candidates never fall in a gap: `*-5/6-21,12 7,20..21:41,11/18:26` in `Europe/London`, `Europe/Dublin`, and `America/New_York` from 2026-12-09 01:58:19 UTC, and `Sun *-7,2/9-27..28/1,14..24` in `Europe/London` from 2026-03-26 04:11:29 UTC (observed). From 2027-03-01 for the first, and in `UTC` for the second, it answers, and the answers equal this crate's (observed). The oracle reports such rows as `unresolved`.

- **A gap that crosses an hour boundary.** `Pacific/Chatham` moves its clock from 02:45 to 03:45. On that day systemd, searching from before the gap, leaves out wall-clock times that exist: `*:0/5` from 02:28 elapses at 02:30, 02:35, 02:40 and then at 04:00, without 03:45, 03:50, and 03:55 (observed). `*-*-* 03:50` skips the day unless the base time is already past the gap (observed). `*:45`, `*:50`, and `*-*-* 02:45` fail with "Failed to determine next elapse" (observed). This crate applies the same rule as in every other zone: it skips the missing times and elapses at every time that exists. The fixtures hold the Chatham rows where both agree and the rows where systemd gives up. The gaps in `America/New_York` and `Australia/Lord_Howe` stay inside one hour, and there the two agree.

- **Wrap floors next to a gap at midnight.** In `America/Havana`, where the clock moves from 00:00 to 01:00 on 2026-03-08, `9/14,1:4/22` from 2026-03-08 02:59:11 UTC elapses in systemd at 23:04, 23:26, and 23:48 on 03-07 and then at 01:26, without 01:04 (observed). This crate elapses at 01:04. The minute floor of the [civil search](#civil-search) is 10 there, and the hour change from 00 to 01 drops it, which is the rule that holds without a gap. In `Pacific/Chatham`, `*:15,50` from 2026-09-26 13:40 UTC elapses in systemd at 04:15 and in this crate at 03:50, as in the entry above. No rule that covers these rows and every other observation was found.

- **Other daylight saving rows where the two differ.** The same random comparison found these, unrelated to the wrap floor (observed):
  - `Europe/Dublin`, where the clock moves from 01:00 to 02:00 on 2026-03-29. When a candidate falls in that gap, systemd moves on to a later day: `1/15:*` from 2026-03-29 00:59:55 UTC first elapses at 2026-03-30 01:00 IST, and `Tue..Sun 1/21:*:*` from 00:57:33 UTC at 2026-03-31 01:00. This crate elapses at 16:00 and 22:00 on 2026-03-29. The gap in `Europe/London` at the same instant agrees for both expressions (observed).
  - Folds where systemd takes the second pass. `1/9:37/18:*` in `Asia/Gaza` from 2026-10-23 20:59:47.806665 UTC elapses in systemd at 01:37:00 +02 and in this crate at 01:37:00 +03, the first pass. `*-*~1..3 *:*:39..45/14,27,26..59/23` in `Africa/Cairo` from 2026-10-29 20:59:32 UTC elapses in systemd at 21:59:39 UTC and in this crate at 20:59:39 UTC.

- **Some fractional repetitions in seconds.** With `TZ=UTC`, `*:*:0/40.9` elapses in systemd at 40.9 s of every minute and never at 0 s (observed), and so do `0/40.1`, `0/40.25`, `0/40.7`, `0/40.99`, `0/39.9`, `0/29.9`, `0/20.9`, and `0/10.9`. `0/40.5`, `0.5/40.4`, `5/40.9`, `0/30.5`, `0/45.5`, and the whole-second steps tried (`0/7`, `0/40`, `0/41`) elapse at their start value too (observed). This crate elapses at the start value in all of them. No rule was found.

## Open questions

Every question the design raised has an answer. This section keeps the answers and how each one was found. A new question starts as new fixture rows.

Answered during the implementation, all observed and all in the fixtures:

- Lists of days from the end: item `i` of the sorted list may reach at most `28 - 3i`. See [Dates](#dates).
- Repetitions on days from the end: `~v/r` needs `v > r`, so it reaches `v - r` at least. See [Dates](#dates).
- `*:*:0..0.5` is rejected because a seconds range without a repetition needs two whole-second values. See [Times and seconds](#times-and-seconds).
- `0/1` becomes `*` in the seconds field only. It stays in the hour, day, month, and year fields.
- A comma after weekdays, `-` in weekday ranges, and weekday spellings. See [Weekdays](#weekdays).
- How the parser tells a zone from the rest. See [Time zones](#time-zones).
- A leading `+` on a number is rejected (`*-*-+1`, `*:+5`), and so is a full-width space between parts.
- With `TZ=UTC`, systemd rejects `daily EST` because the oracle image has no `EST` file. See [Known differences](#known-differences-from-systemd).

Answered since the first draft, all observed and all in the fixtures:

- Items sort by start, then by end with a missing end first, then by step. `*-*-1/2,1..3` prints as `01/2,01..03`.
- `*:*:0..59/1` prints as `*:*:00..59`.
- `*-*~03..06/2` prints as `~03..05/2`.
- A stepped range has no `start + step <= max` check. `*:50..59/20` prints as `*:50`.
- Year expansion goes by value, not by digit count. `0070-01-01` is 1970, and `12/2-01-01` prints as `2012/2-01-01`.

Answered with a time zone library as a calculator for transition times and the oracle for the answers, all in the fixtures:

- Zones with a half-hour transition. `Australia/Lord_Howe` agrees with the search rule minute by minute around its gap and its fold.
- Several candidates inside one gap. `*-*-* 02,03:0/30 America/New_York` on the transition day elapses at 03:00 and 03:30 only.
- A day that does not exist. `Pacific/Apia` skipped 2011-12-30. `*-12-30` first elapses in 2012, and `2011-12-30 12:00` never elapses.
- Weekdays follow the zone of the expression, also when the local zone has daylight saving time of its own. `Mon *-*-* 00:30 Pacific/Auckland` with `TZ=America/New_York` elapses on Sundays in UTC.
- The end of 2199. The last elapse is 2199-12-31 23:59:59.999999 in the effective zone. A local time in 2199 elapses even when its UTC instant is in 2200, such as `*-12-31 23:00` in `America/New_York`.
- Input length. systemd has no length limit that was found. It accepts 1,000 weekday items, 3,000 spaces between parts, and 3,000 leading zeros.
- Base times before 1970. systemd rejects such a base time before it reads the expression, so the oracle cannot be asked. The crate starts the search at 1970-01-01.

Answered by calling systemd directly, outside the fixture format:

- Tabs. systemd rejects a tab anywhere in the expression: between parts, before the zone, at either end (observed). The crate does the same, and `tests/robustness.rs` checks it, because a tab-separated fixture cannot hold such a row.
- tzdata versions. The first line of the oracle output names the tzdata build, and `oracle/check.sh` compares that line. A fixture row that fails on one machine only points to a different tzdata there first.
- Names such as `EST`, `MST`, `HST`, `UCT`, and `Zulu`. systemd has no list of its own. It accepts a name when the time zone database has a file for it, in exact letter case. With Debian's `tzdata-legacy` package installed it accepts all of them (observed), and without it, it rejects them. The crate follows the same rule through jiff. The oracle image does not install that package, so the fixtures hold no such row.
- Local abbreviations. systemd accepts the abbreviations of the process time zone and elapses only while that abbreviation is in effect: with `TZ=America/New_York`, `daily EDT` never elapses in January and `daily EST` never elapses in July (observed). The abbreviation wins over a database file of the same name. This depends on the process environment, which the crate never reads, so v0.1 does not support it. This is final for v0.1 and listed under known differences.

No question is open.
