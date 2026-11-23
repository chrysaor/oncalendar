# Parse and normalize

A user parses an expression with `str::parse::<CalendarEvent>()` and prints the canonical form with `to_string()`. The canonical form is the string `systemd-analyze calendar` shows as "Normalized form".

## Sub-features

- `norm-shorthand` expands `minutely`, `hourly`, `daily`, `weekly`, `monthly`, `quarterly`, `semiannually`, `yearly`, `annually`.
- `norm-defaults` fills an omitted date with `*-*-*`, an omitted time with `00:00:00`, omitted seconds with `:00`.
- `norm-weekdays` sorts weekdays from Monday, joins three or more consecutive days with `..`, and drops the part when all seven are present.
- `norm-lists` sorts list items, removes exact duplicates, and keeps overlapping ranges.
- `norm-ranges` folds `a..a` to `a` and shortens a stepped range to the last value it reaches.
- `norm-seconds` rounds fractional seconds to six digits.
- `norm-year` expands two-digit years (`00`..`69` to 20xx, `70`..`99` to 19xx).
- `norm-idempotent` parsing a canonical string yields the same canonical string.

## How to get to it (user POV)

- `"<expression>".parse::<oncalendar::CalendarEvent>()` then `event.to_string()`.
- Available with `default-features = false`. Parsing and printing need no dependency.

## Driving it with verify

Preconditions:

- `verify doctor` exits 0.

- **Shorthand.** Run `verify one UTC '2026-01-01 00:00:00 UTC' 'weekly'`. Both rows show `Mon *-*-* 00:00:00`.
- **Defaults.** Run `verify one UTC '2026-01-01 00:00:00 UTC' '*:0/15'`. Both rows show `*-*-* *:00/15:00`.
- **Weekdays.** Run `verify one UTC '2026-01-01 00:00:00 UTC' 'Sat,Thu,Mon..Wed,Sat..Sun'`. Both rows show `Mon..Thu,Sat,Sun *-*-* 00:00:00`.
- **Lists.** Run `verify one UTC '2026-01-01 00:00:00 UTC' '*:5,3,5,1,3'`. Both rows show `*-*-* *:01,03,05:00`.
- **Overlapping ranges.** Run `verify one UTC '2026-01-01 00:00:00 UTC' '*:1..3,2..4'`. Both rows show `*-*-* *:01..03,02..04:00`.
- **Fractional seconds.** Run `verify one UTC '2026-01-01 00:00:00 UTC' '05:40:23.4200004/3.1700005'`. Both rows show `*-*-* 05:40:23.420000/3.170001`.
- **Two-digit year.** Run `verify one UTC '2026-01-01 00:00:00 UTC' '70-01-01'`. Both rows show `1970-01-01 00:00:00`.
- **Idempotence.** Take the normalized column of any `ok` row and run it through `verify one` again. The normalized column is unchanged.
- **Proof.** Run `verify compare`. `summary.txt` reads `differing: 0`.

## Gotchas

- `*:0/1` stays `*:00/1` in the minute field. Only seconds collapse `0/1` to `*`.
- Whitespace between the weekday, date, and time parts may repeat. Leading or trailing whitespace is an error, and so is repeated whitespace before the time zone.
- `Mon,` with a trailing comma is accepted. `Mon,,Fri` is not.
- The normalized column also proves acceptance. A row that says `error` on the systemd side belongs to [Reject invalid input](./reject-invalid-input.md).
