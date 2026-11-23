# Next elapse

A user asks when an event fires next. `event.upcoming(after, &local)` returns an iterator of timestamps strictly after `after`, in increasing order. An event that can never fire again yields nothing.

## Sub-features

- `next-strict` excludes the base time itself.
- `next-order` yields strictly increasing timestamps.
- `next-weekday-and-date` requires both the weekday and the date to match.
- `next-month-end` resolves `~` days from the end of each month.
- `next-never` yields nothing for a past fixed date and for a date that does not exist (`*-02-30`).
- `next-year-limit` stops at the end of 2199.
- `next-subsecond` computes at microsecond resolution.
- `next-terminates` returns quickly for dense and sparse expressions alike.

## How to get to it (user POV)

- `event.upcoming(after, &local)?` then `.next()` for one time, or `.take(n)` for several.
- Requires the `jiff` feature, which is on by default.

## Driving it with verify

Preconditions:

- `verify doctor` exits 0.

- **Strictly after.** Run `verify one UTC '2026-01-01 00:00:00 UTC' 'daily'`. Both rows start at `2026-01-02T00:00:00Z`.
- **Weekday and date.** Run `verify one UTC '2026-01-01 00:00:00 UTC' 'Mon *-*-01'`. Both rows list only first-of-month Mondays.
- **Month end.** Run `verify one UTC '2026-01-01 00:00:00 UTC' '*-*~1 03:00'`. Both rows start at `2026-01-31T03:00:00Z` and continue with `2026-02-28T03:00:00Z`.
- **Never, impossible date.** Run `verify one UTC '2026-01-01 00:00:00 UTC' '*-02-30'`. Both rows show `never`.
- **Never, past date.** Run `verify one UTC '2026-01-01 00:00:00 UTC' '2003-03-05 05:40'`. Both rows show `never`.
- **Year limit.** Run `verify one UTC '2026-01-01 00:00:00 UTC' '2199-01-01'`. Both rows show the single elapse `2199-01-01T00:00:00Z`.
- **Sub-second, before.** Run `verify one UTC '2026-01-01 00:00:00.499999 UTC' '*:*:0.5'`. Both rows start at `2026-01-01T00:00:00Z`.
- **Sub-second, at.** Run `verify one UTC '2026-01-01 00:00:00.5 UTC' '*:*:0.5'`. Both rows start at `2026-01-01T00:01:00Z`. Together with the previous step this pins the elapse at exactly `.500000`.
- **Dense expression.** Run `verify one UTC '2026-01-01 00:00:00 UTC' '*-*-* *:*:0/0.000001'`. The command returns within seconds and both rows match.
- **Proof.** Run `verify compare`. `summary.txt` reads `differing: 0`.

## Gotchas

- The oracle prints whole seconds. Two elapses inside one second look identical in the row. Use the before-and-at pair above to prove a sub-second value.
- `verify one` prints five elapses. An event with fewer than five remaining shows only those.
- Collecting the iterator without `take` on a dense expression never finishes in practice. The iterator is finite but has about 10^15 items.
- Termination and speed under hostile expressions have no oracle. `cargo test --all-features --test robustness` covers them.
