---
name: verify-oncalendar
description: "Prove behavior of the oncalendar Rust library (systemd OnCalendar parser) by driving its public API from a separate crate and diffing the result against real systemd-analyze in Docker. Also runs the weight and security gates. Use after changing parsing, normalization, next-elapse calculation, dependencies, or before claiming a change works."
---

# Verify oncalendar

`oncalendar` is a library. Its user is another crate that calls the public API. This skill drives the library from such a crate (the probe) and compares every answer with real `systemd-analyze calendar` (the oracle). Unit tests passing is not proof. A matching diff against the oracle is.

Every command below goes through one helper:

```sh
.claude/skills/verify-oncalendar/bin/verify <command>
```

It works from any directory inside the checkout.

## Launch

There is no server. Launch means building two things, and the helper does both on demand:

- the probe, a crate outside the library that depends on it by path (`.claude/skills/verify-oncalendar/probe/`)
- the oracle image, Debian with systemd, base image pinned by digest and packages by version (`oracle/Dockerfile`)

Requirements: `cargo`, a running Docker daemon, `cargo-deny`, and the `thumbv7em-none-eabihf` target for the `no_std` gate. On macOS with colima, run `colima start` first. Ready means `verify doctor` exits 0.

## Doctor

```sh
.claude/skills/verify-oncalendar/bin/verify doctor
```

Read-only apart from build caches. It checks that cargo and Docker answer, that the library builds with and without default features, that the probe compiles against the current public API, and that the oracle answers a known case. Run it first whenever a result looks wrong. A failing "probe builds" line means the public API changed and `probe/src/main.rs` must follow.

## Drive

One case, when you are working on a specific behavior:

```sh
.claude/skills/verify-oncalendar/bin/verify one <time zone> <base time> <expression>
.claude/skills/verify-oncalendar/bin/verify one UTC '2026-01-01 00:00:00 UTC' 'Mon..Fri *-*-* 09:00'
.claude/skills/verify-oncalendar/bin/verify one America/New_York '2026-11-01 06:15:00 UTC' '*:30'
```

- `<time zone>` is the process-local zone. It applies when the expression names no zone.
- `<base time>` is `YYYY-MM-DD HH:MM:SS[.ffffff] <zone>`. Elapses are strictly after it.
- The command prints the systemd row and the oncalendar row and exits 0 only if they are identical. Exit 1 is a difference. Exit 3 means one side did not answer every case (the oracle failed, the probe crashed, or a case was unreadable), and `summary.txt` starts with `broken:`. Exit 3 is never proof of anything.

A whole file of cases:

```sh
.claude/skills/verify-oncalendar/bin/verify compare                    # tests/fixtures/cases.tsv
.claude/skills/verify-oncalendar/bin/verify compare path/to/cases.tsv
```

A case file has one case per line: `<time zone><TAB><base time><TAB><expression>`. Lines that start with `#` are section titles.

The gates:

```sh
.claude/skills/verify-oncalendar/bin/verify gates
```

It runs formatting, the tests in both feature configurations, `scripts/check-security.sh`, `scripts/check-weight.sh`, `scripts/check-no-std.sh`, and `oracle/check.sh`, and prints PASS or FAIL per gate.

Each row has six tab-separated columns: zone, base time, expression, status, the normalized expression, and up to five elapses in UTC (or `never`). Status `error` means the expression was rejected. Status `unresolved` appears only on the systemd side: systemd parsed the expression and then gave up computing an elapse. For such a row the helper compares acceptance only, and `summary.txt` counts it separately.

When a new behavior is settled, add its cases to `tests/fixtures/cases.tsv` and regenerate the expectations with `oracle/run.sh < tests/fixtures/cases.tsv > tests/fixtures/expected.tsv`. The expectations come from systemd, never from the crate.

## Evidence

Each run writes to `.verify/<UTC timestamp>-<command>/` in the repository root (git-ignored). A second run in the same second gets a process id suffix.

| File | Content |
| --- | --- |
| `cases.tsv` | the input |
| `oracle.tsv` | what systemd printed; line 1 names the systemd build |
| `crate.tsv` | what the library printed through the probe |
| `diff.txt` | unified diff of the two, header lines excluded; empty means identical |
| `summary.txt` | case count, differing count, and how many rows systemd left unresolved, or a `broken:` line |
| `<gate>.txt` | full output of each gate (`gates` only) |

Proof standards:

- Quote `summary.txt` and the evidence path. Do not paraphrase a result you did not run.
- A claim about parsing or normalization needs an `ok` row whose normalized column matches. A claim about scheduling needs matching elapses. A claim about rejection needs `error` on both sides.
- The oracle truncates elapses to whole seconds. To pin a sub-second result, add two cases whose base times sit one microsecond before and exactly at the expected elapse.
- The probe uses only the public API. Do not add test-only hooks to the library to make a case pass.
- A weight or security claim needs the `gates` output, not a reading of `Cargo.toml`.
- An `unresolved` row proves acceptance only. To prove the elapse the library gives for it, move the base time until systemd answers (for a never-elapsing expression, a base time near 2190 works) and compare there.
- A difference listed in `README.md` under "Known differences" is expected. Any other difference is a bug in the library or a missing entry in that list. Say which.

## Cleanup

```sh
.claude/skills/verify-oncalendar/bin/verify clean
```

Removes the probe's build output (`target/verify-probe`). It keeps `.verify/`. Oracle containers are started with `--rm` and leave nothing behind. The oracle image `oncalendar-oracle` stays cached; remove it with `docker image rm oncalendar-oracle` if you need the space.

## Helpers

- `bin/verify` is the only entry point. `verify` with no argument prints its usage.
- `probe/` is the consumer crate. It prints rows in the oracle's format so that the two outputs diff cleanly.
- `oracle/run.sh [ITERATIONS] < cases.tsv` is the raw oracle. `oracle/check.sh` fails when `tests/fixtures/expected.tsv` no longer matches systemd.
- `scripts/check-weight.sh`, `scripts/check-security.sh`, and `scripts/check-no-std.sh` are the gates. Each runs standalone. The `no_std` gate fails when `thumbv7em-none-eabihf` is not installed (`rustup target add thumbv7em-none-eabihf`).

## Rules that bound verification

- Clean room. The only sources of truth are the systemd manual and the oracle's observable output. Do not open systemd source code or another implementation of this grammar to explain a difference. Probe the oracle with more cases instead.
- The feature map in `features/` lists what a user can do. A proof that drives one convenient case is incomplete when the map lists other entry points for the behavior you changed.
