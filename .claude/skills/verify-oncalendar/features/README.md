# oncalendar verification map

This directory is the maintained source for verifying what a user of the `oncalendar` crate can observe. Read this index before driving the library, then use the matching feature file as the recipe.

## Baseline preconditions

- `verify` below means `scripts/verify`.
- `verify doctor` exits 0.
- The default base time is `2026-01-01 00:00:00 UTC` and the default local zone is `UTC`, unless a recipe says otherwise.
- The oracle is systemd 257 in the pinned image. Line 1 of every `oracle.tsv` names the exact build.

## Driving conventions

- Treat every command as literal. Keep quotes unchanged. Expressions contain `*`, so always single-quote them.
- Drive single behaviors with `verify one`, whole sections with `verify compare`.
- Never edit `tests/fixtures/expected.tsv` by hand. Regenerate it from the oracle.
- Do not put a tab inside an expression. The case format is tab-separated.

## Proof and skip reporting

- A proof is the `summary.txt` line `cases: N, differing: 0, ...` plus the evidence directory.
- Report a difference with the two rows side by side, as `verify one` prints them.
- Report an unreachable path with the command you ran and what failed (`doctor` line, Docker error).
- Do not report a behavior as verified through a different feature's recipe.

## Feature entry contract

Each feature file starts with an H1 title and one paragraph describing the user-visible behavior. It then uses exactly four H2 sections in this order: `Sub-features`, `How to get to it (user POV)`, `Driving it with verify`, `Gotchas`.

## Features

- [Parse and normalize](./parse-and-normalize.md) covers accepted syntax and the canonical string.
- [Reject invalid input](./reject-invalid-input.md) covers errors, input limits, and hostile input.
- [Next elapse](./next-elapse.md) covers the iterator of upcoming times, `never`, and sub-second results.
- [Time zones and daylight saving](./time-zones-and-dst.md) covers the local zone, zones named in the expression, gaps, and folds.
- [Cron conversion](./cron-conversion.md) covers `CalendarEvent::from_cron`: the cron fields, the day of month and day of week rule, steps, and rejected cron dialects.
- [Command-line tool](./command-line-tool.md) covers the `oncalendar` executable: output, error position, exit status, standard input, and JSON.
- [Weight and security gates](./gates.md) covers dependency count, build cost, lints, advisories, and robustness tests.
