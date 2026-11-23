# Time zones and daylight saving

An expression without a zone is read in the caller's local zone. An expression that ends with `UTC` or an IANA name is read in that zone. Around daylight-saving transitions, a wall-clock time that does not exist is skipped, and one that occurs twice fires once.

## Sub-features

- `tz-local` uses the `local` argument when the expression names no zone.
- `tz-utc` accepts `UTC` in any letter case and prints `UTC`.
- `tz-named` accepts an IANA name and prints it unchanged.
- `tz-unknown` reports an unknown zone name as an error.
- `dst-gap` skips a wall-clock time inside a spring-forward gap.
- `dst-fold-first` fires once for a repeated wall-clock time, at the earliest instant after the base time.
- `dst-fold-second` still fires when the base time already sits in the second pass of the repeated hour.
- `dst-gap-forever` answers `never` when every future candidate falls in a gap, where systemd gives up.

## How to get to it (user POV)

- `event.upcoming(after, &local)` where `local` is a `jiff::tz::TimeZone` the caller chose.
- A zone suffix in the expression: `'Mon *-*-* 09:00 Asia/Seoul'`.

## Driving it with verify

Preconditions:

- `verify doctor` exits 0.
- The first argument of `verify one` is the local zone.

- **Local zone.** Run `verify one Asia/Seoul '2026-01-01 00:00:00 UTC' 'daily'`. Both rows start at `2026-01-01T15:00:00Z`.
- **Zone in the expression.** Run `verify one UTC '2026-01-01 00:00:00 UTC' 'daily Asia/Seoul'`. Both rows show `*-*-* 00:00:00 Asia/Seoul` and start at `2026-01-01T15:00:00Z`.
- **UTC letter case.** Run `verify one UTC '2026-01-01 00:00:00 UTC' 'daily utc'`. Both rows show `*-*-* 00:00:00 UTC`.
- **Unknown zone.** Run `verify one UTC '2026-01-01 00:00:00 UTC' 'daily Nowhere/Bogus'`. Both rows show `error`.
- **Gap.** Run `verify one America/New_York '2026-01-01 00:00:00 UTC' '*-03-08 02:30'`. Both rows start at `2027-03-08T07:30:00Z`. The year 2026 is skipped because 02:30 does not exist that day.
- **Fold, base before it.** Run `verify one America/New_York '2026-11-01 05:15:00 UTC' '*:30'`. Both rows read `2026-11-01T05:30:00Z,2026-11-01T07:30:00Z,...`. `06:30Z` is absent.
- **Fold, base inside the second pass.** Run `verify one America/New_York '2026-11-01 06:15:00 UTC' '*:30'`. Both rows start at `2026-11-01T06:30:00Z`.
- **Gap every year.** Run `verify one UTC '2190-01-01 00:00:00 UTC' 'Sun *-03-08..14 02:30 America/New_York'`. Both rows show `never`. With the base time `2026-01-01 00:00:00 UTC` the systemd row shows `unresolved` and the oncalendar row shows `ok` with `never`.
- **Proof.** Run `verify compare`. `summary.txt` reads `differing: 0`.

## Gotchas

- Results depend on the time zone database. The oracle image and the host can carry different tzdata releases. A difference that appears only for far-future dates in one zone points there first. Compare the tzdata versions before changing code.
- The library never reads `TZ` or `/etc/localtime`. The caller passes the local zone. The probe does this with the first argument.
- Transition dates move by year and by zone. The New York dates above are for 2026. Pick new dates from the oracle, not from memory.
- Zones with half-hour transitions (`Australia/Lord_Howe`) and zones that skipped a whole day (`Pacific/Apia`, 2011-12-30) are valid cases and behave differently from New York. Add them when touching transition code.
