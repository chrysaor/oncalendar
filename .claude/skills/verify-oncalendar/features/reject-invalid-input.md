# Reject invalid input

A user passes text the library has never seen, possibly from an attacker. Parsing returns `ParseError` for anything outside the grammar or over a limit. It never panics, never runs long, and never echoes the input in its message.

## Sub-features

- `reject-syntax` rejects malformed structure (empty string, `*`, `Mon Tue`, `*-*-*-*`).
- `reject-range` rejects out-of-range values (`25:00`, `*-13-01`, `1969-01-01`, `2200-01-01`).
- `reject-repetition` rejects a zero step and a step that cannot fit the field (`*:0/0`, `*:0/60`).
- `reject-reversed` rejects a range whose end is below its start (`5..3`, `Fri..Mon`).
- `reject-limit` rejects a numeric field with more than 241 list items.
- `reject-zone-shape` rejects a time zone token that is not a plain zone name (`../../etc/passwd`, `/etc/passwd`).
- `reject-safe-message` keeps error text fixed, printable ASCII, with a byte offset and no input fragment.
- `reject-no-panic` returns `Ok` or `Err` for every string.

## How to get to it (user POV)

- `"<expression>".parse::<oncalendar::CalendarEvent>()` returns `Err(ParseError)`.
- `error.offset()` gives the byte offset of the problem. `error.to_string()` gives the message.

## Driving it with verify

Preconditions:

- `verify doctor` exits 0.

- **Out of range.** Run `verify one UTC '2026-01-01 00:00:00 UTC' '*-*-* 25:00'`. Both rows show `error`.
- **Year bounds.** Run `verify one UTC '2026-01-01 00:00:00 UTC' '2200-01-01'`. Both rows show `error`. Run it with `2199-01-01`. Both rows show `ok`.
- **Reversed range.** Run `verify one UTC '2026-01-01 00:00:00 UTC' '*-*-* 5..3:00'`. Both rows show `error`.
- **Bad step.** Run `verify one UTC '2026-01-01 00:00:00 UTC' '*:0/60'`. Both rows show `error`.
- **List limit.** Build a minute list with 241 items and one with 242: `verify one UTC '2026-01-01 00:00:00 UTC' "*:$(awk 'BEGIN { for (i = 0; i < 241; i++) printf "%s%d", (i ? "," : ""), i % 60 }')"` shows `ok` on both sides, and the same command with `i < 242` shows `error` on both sides.
- **Leading space.** Run `verify one UTC '2026-01-01 00:00:00 UTC' ' daily'`. Both rows show `error`.
- **Hostile input and messages.** Run `cargo test --all-features --test robustness`. Every test passes. These properties have no oracle, so the test suite is the proof.
- **Proof.** Run `verify compare`. The "Rejected input" sections read `error` on both sides and `summary.txt` reads `differing: 0`.

## Gotchas

- An unknown but well-formed time zone name (`daily Nowhere/Bogus`) is accepted by the parser and rejected when computing elapses. systemd rejects it at parse time. The probe reports `error` for both, so the rows still match. See "Known differences" in `README.md`.
- Local zone abbreviations (`daily KST`) are accepted by systemd only when the process zone uses that abbreviation. The library never accepts them. Do not add such cases to the fixtures.
- Weekday lists have no item limit. `Mon,` repeated thousands of times is valid.
- The oracle reports only success or failure for a rejected expression. Error kind and offset are checked by the crate's own tests.
