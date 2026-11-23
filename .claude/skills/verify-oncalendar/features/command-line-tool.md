# Command-line tool

A user without systemd runs `oncalendar` in a terminal to check an expression. One run says whether the expression is valid, prints its normalized form, and lists when it elapses next. A failure names its position and has its own exit status.

## Sub-features

- `cli-check` prints the normalized form on the first line and exits 0.
- `cli-elapses` lists the next elapses in the local zone with weekday, abbreviation, offset, and time left.
- `cli-words` joins the argument words with single spaces, so an expression needs no quotes.
- `cli-stdin` reads one expression per line from standard input and skips empty lines and `#` lines.
- `cli-error-position` prints the library message, the input, and a `^` under the byte the error names.
- `cli-escape` prints every byte outside printable ASCII as an escape, so input cannot drive the terminal.
- `cli-json` prints one JSON object per expression with `--json`.
- `cli-exit-status` exits 1 for an invalid expression, 2 for a usage error, 3 for an unknown time zone, 4 when nothing elapses, and 5 for an I/O error.
- `cli-bundled-tzdb` resolves zone names without time zone files on the host.
- `cli-matches-systemd` gives the systemd result for every fixture row.

## How to get to it (user POV)

- `cargo install oncalendar-cli` installs the executable `oncalendar`.
- From a checkout, `cargo build --release --locked --manifest-path cli/Cargo.toml` builds `cli/target/release/oncalendar`.
- `oncalendar --help` lists the options and the exit statuses.

## Driving it with verify

Preconditions:

- `verify doctor` exits 0.
- The executable is built from the checkout, as above. `oncalendar` below means `cli/target/release/oncalendar`.
- Every recipe passes `--zone` and `--after`, so the output does not depend on the host or the clock.

- **Check and list.** Run `oncalendar -n 3 --zone Asia/Seoul --after '2026-01-01 00:00' mon..fri 9:00`. The first line is `Mon..Fri *-*-* 09:00:00`, three `KST (+09:00)` lines follow, and the exit status is 0.
- **Error position.** Run `oncalendar 'Mon *-*-* 24:00'`. The message ends with `at byte 10`, the `^` is under `24:00`, and the exit status is 1.
- **Unknown zone.** Run `oncalendar -n 1 'daily Asia/Seol'`. The exit status is 3 and the output has a `parsed as` line.
- **No elapse.** Run `oncalendar -n 2 --zone UTC '2020-01-01'`. The output says `never elapses after` and the exit status is 4.
- **Usage error.** Run `oncalendar --bogus`. The exit status is 2.
- **Escaping.** Run `oncalendar -n 0 "$(printf 'daily\033[31mX')"`. The input line shows `\u{1b}` and the terminal color does not change.
- **Standard input and JSON.** Run `printf 'daily\n*-*-* 25:00\n' | oncalendar --json -n 2 --zone Asia/Seoul --after '2026-01-01 00:00'`. Line 1 has `normalized` and two `elapses`. Line 2 has `"error":"invalid-expression"` and `"offset":6`. The exit status is 1.
- **Daylight saving gap.** Run `oncalendar -n 4 --zone America/New_York --after '2026-03-08 01:59:57' '*:*:0/7'`. The first line after the normalized form is `03:00:07 EDT`.
- **Closed pipe.** Run `oncalendar -n 10000 --zone UTC '*:*:*' | head -2`. Two lines print and no error follows.
- **Proof.** Run `scripts/check-cli.sh`. It ends with `cli: PASS`. Its test step runs the tool on every row of `tests/fixtures/expected.tsv`, as an argument and on standard input.

## Gotchas

- Without `--zone`, the tool uses the system time zone. When it cannot find one, it warns on standard error and uses UTC.
- `--zone` ignores letter case. A zone named inside the expression must match the database exactly.
- With several expressions, the exit status is the lowest failure status among them.
- `scripts/check-cli.sh` builds the packaged tool against the `oncalendar` release on crates.io, so it needs network access, and a tool change that needs a new library API fails until that library version is published.
- The tool has its own `cli/clippy.toml`, because the library's `clippy.toml` forbids the system time zone lookup that the tool needs.
- `verify one` and `verify compare` drive the library through the probe, not the tool. A claim about the tool needs a recipe from this file.
