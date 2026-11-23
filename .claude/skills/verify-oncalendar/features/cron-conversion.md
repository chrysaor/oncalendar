# Cron conversion

A user who moves a cron job to a systemd timer calls `CalendarEvent::from_cron` with the five time fields of a crontab line and gets one or two calendar events, one `OnCalendar=` line each. The events elapse at the same minutes as cron, and each event prints in the normalized form that systemd prints. No real cron runs in this repository, so the cron side of the proof is a reference calculator written from the crontab manual, not an oracle.

## Sub-features

- `cron-fields` reads minute, hour, day of month, month, and day of week with `*`, values, ranges, lists, and steps after `*` or a range.
- `cron-names` reads three-letter month and weekday names in any case, and both 0 and 7 as Sunday.
- `cron-special` expands `@yearly`, `@annually`, `@monthly`, `@weekly`, `@daily`, `@midnight`, and `@hourly`. `@weekly` is Sunday, unlike systemd's `weekly`.
- `cron-steps` turns a step that does not divide its field, such as `*/45` or `0 */5`, into a stepped range, so it restarts at the start of every hour or day as cron does.
- `cron-days` returns two events when both the day of month and the day of week are restricted, because cron runs when either matches.
- `cron-reject` rejects seconds and year fields, `L`, `W`, `#`, `?`, `@reboot`, `5/15`, and input over 1024 bytes with a fixed message and a byte offset.

## How to get to it (user POV)

- `oncalendar = { version = "0.1", features = ["cron"] }`. The feature adds no dependency and works with `default-features = false`.
- `CalendarEvent::from_cron("*/15 9-17 * * mon-fri")` returns `Ok(vec![event])`, and `event.to_string()` is the `OnCalendar=` value.
- An error is a `CronError` with `offset()` and a message that never repeats the input.

## Driving it with verify

Preconditions:

- `verify doctor` exits 0.
- The probe reads calendar expressions, not cron expressions, so `verify` checks the converted events. The cron side is checked by `cargo test --all-features --test cron`.

- **Conversion table.** Run `cargo test --all-features --test cron fixture_rows_convert_as_listed`. Every row of `tests/fixtures/cron.tsv` converts to the events or the error it lists.
- **systemd accepts every event.** Run `cargo test --all-features --test cron every_converted_event_is_a_systemd_fixture_row`. Each event in `cron.tsv` is a fixture row that systemd accepted with the same normalized form.
- **Same minutes as cron.** Run `cargo test --all-features --test cron -- --nocapture elapse_when_cron_runs`. It prints how many cases and elapses it compared with the reference calculator, in UTC, over 2024 to 2031 for the fixture rows and over four years for 1,500 random expressions.
- **One step.** Run `verify one UTC '2026-01-31 23:50:00 UTC' '*-*-* *:00..45/45:00'`. Both rows start with `2026-02-01T00:00:00Z,2026-02-01T00:45:00Z`.
- **Random events against systemd.** Run `cargo test --all-features --test cron -- --ignored --nocapture print_random_conversions_for_oracle | grep '^UTC' > random.tsv`, then `verify compare random.tsv`. `summary.txt` reads `differing: 0`.
- **Proof.** Run `verify compare`. `summary.txt` reads `differing: 0`, and the cron section of `tests/fixtures/cases.tsv` is part of it.

## Gotchas

- `*:00/45` is not `*/45`. When the minutes run out at the end of a day, systemd carries the repetition on past the hour, so after 23:45 it next elapses at 00:45 and skips 00:00 (`verify one UTC '2026-01-31 23:50:00 UTC' '*-*-* *:00/45:00'`). The conversion writes `00..45/45`, which restarts at 00:00. It keeps `v/r` only where the step reaches the end of the field exactly, as in `00/15`, where the two agree.
- A day field that starts with `*`, such as `*/2`, counts as unrestricted, so `0 0 */2 * sun` is one event, Sundays on odd days. POSIX has no steps and does not decide this case.
- The month always narrows the day fields. `0 0 * 1 1` is Mondays in January, and `0 0 1 1 1` is January 1 and every Monday in January.
- `fri-sun` is rejected as a reversed range, because `sun` is 0. Write `fri-7` or `5-7`.
- The comparison runs in UTC only. Daylight saving time follows systemd's rules, not those of any cron daemon.
