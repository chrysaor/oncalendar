# oncalendar-cli

`oncalendar` checks a systemd calendar event expression, the value of `OnCalendar=` in a timer unit, on any machine: macOS, Windows, a container, or CI, with no systemd installed. It tells you whether the expression is valid, prints its normalized form, and lists when it elapses next.

It is a small front end to the [`oncalendar`](https://crates.io/crates/oncalendar) library, whose results are checked against the real `systemd-analyze calendar`. It depends on that library and `jiff` only.

## Install

```sh
cargo install oncalendar-cli
```

This installs an executable named `oncalendar`. It needs Rust 1.85 or later.

## Use

Give it an expression. Quote it when it contains `*`, so that the shell does not expand it:

```console
$ oncalendar 'mon..fri 9:00'
Mon..Fri *-*-* 09:00:00
  Tue 2026-11-24 09:00:00 KST (+09:00)  in 12h 58m
  Wed 2026-11-25 09:00:00 KST (+09:00)  in 1d 12h
  Thu 2026-11-26 09:00:00 KST (+09:00)  in 2d 12h
  Fri 2026-11-27 09:00:00 KST (+09:00)  in 3d 12h
  Mon 2026-11-30 09:00:00 KST (+09:00)  in 6d 12h
```

The first line is the normalized form, the text `systemd-analyze calendar` would print as normalized. You can paste it into a timer unit. The lines below it are the next elapses in your local time zone, with the time left until each.

The words of an expression are joined with single spaces, so `oncalendar Mon..Fri 9:00` works too.

Choose the number of elapses, the starting point, and the local time zone:

```console
$ oncalendar -n 3 --zone America/New_York --after '2026-10-31 12:00' '*-*-* 01:30'
*-*-* 01:30:00
  Sun 2026-11-01 01:30:00 EDT (-04:00)  in 13h 30m
  Mon 2026-11-02 01:30:00 EST (-05:00)  in 1d 14h
  Tue 2026-11-03 01:30:00 EST (-05:00)  in 2d 14h
```

Near a daylight saving change it follows systemd. A time that does not exist is skipped, and a time that occurs twice elapses once:

```console
$ oncalendar -n 4 --zone America/New_York --after '2026-03-08 01:59:57' '*:*:0/7'
*-*-* *:*:00/7
  Sun 2026-03-08 03:00:07 EDT (-04:00)  in 10s
  Sun 2026-03-08 03:00:14 EDT (-04:00)  in 17s
  Sun 2026-03-08 03:00:21 EDT (-04:00)  in 24s
  Sun 2026-03-08 03:00:28 EDT (-04:00)  in 31s
$ oncalendar -n 3 --zone America/New_York --after '2026-11-01 05:15Z' '*:30'
*-*-* *:30:00
  Sun 2026-11-01 01:30:00 EDT (-04:00)  in 15m
  Sun 2026-11-01 02:30:00 EST (-05:00)  in 2h 15m
  Sun 2026-11-01 03:30:00 EST (-05:00)  in 3h 15m
```

An invalid expression gets a message that points at the problem, and the exit status is 1:

```console
$ oncalendar 'Mon *-*-* 24:00'
error: calendar expression: hour out of range at byte 10
  Mon *-*-* 24:00
            ^
```

Control characters and characters outside ASCII are shown escaped, so a dash copied from a web page is easy to spot: `Mon\u{2013}Fri`.

To only check an expression, ask for no elapses:

```console
$ oncalendar -n 0 'Sat,Sun 10:30' && echo valid
Sat,Sun *-*-* 10:30:00
valid
```

### Several expressions

With no expression on the command line, `oncalendar` reads one expression per line from standard input. Empty lines and lines that start with `#` are skipped. An invalid line is reported with its line number, and the rest are still checked:

```console
$ printf '# backups\ndaily\nweekly Europe/Berlin\n*-*-* 25:00\n' | oncalendar -n 2 --zone UTC --after 2026-01-01
*-*-* 00:00:00
  Fri 2026-01-02 00:00:00 UTC  in 1d
  Sat 2026-01-03 00:00:00 UTC  in 2d

Mon *-*-* 00:00:00 Europe/Berlin
  Sun 2026-01-04 23:00:00 UTC  in 3d 23h
  Sun 2026-01-11 23:00:00 UTC  in 10d 23h

error: line 4: calendar expression: hour out of range at byte 6
  *-*-* 25:00
        ^
```

Results go to standard output and errors to standard error.

### Output for scripts

`--json` prints one JSON object per expression, one per line:

```console
$ printf 'daily\n*-*-* 25:00\n' | oncalendar --json -n 2 --zone Asia/Seoul --after '2026-01-01 00:00'
{"line":1,"expression":"daily","normalized":"*-*-* 00:00:00","elapses":["2026-01-02T00:00:00+09:00","2026-01-03T00:00:00+09:00"]}
{"line":2,"expression":"*-*-* 25:00","error":"invalid-expression","message":"calendar expression: hour out of range at byte 6","offset":6}
```

| Field | When | Value |
| --- | --- | --- |
| `line` | reading standard input | The line number, counting skipped lines. |
| `expression` | always | The input as read. |
| `normalized` | the expression parsed | The normalized form. |
| `elapses` | no `error` | RFC 3339 times with the local offset. Empty when the expression never elapses. Shorter than `--count` when the elapses run out. |
| `error` | the expression failed | `invalid-expression` or `unknown-time-zone`. |
| `message` | `error` is present | A fixed message. It never contains the input. |
| `offset` | a byte position is known | The byte in `expression` where the problem starts. |

## Options

| Option | Meaning |
| --- | --- |
| `-n`, `--count N` | List `N` elapses per expression, from 0 to 10,000. The default is 5. With 0 the expression is only checked. |
| `-a`, `--after TIME` | List the elapses strictly after `TIME`. The default is now. |
| `-z`, `--zone NAME` | The local time zone. It applies to an expression that names no zone, and every time is printed in it. The default is the system time zone. |
| `--bundled-tzdb` | Look time zone names up in the copy of the time zone database compiled into `oncalendar`, not in the system copy. |
| `--json` | Print one JSON object per expression. |
| `-h`, `--help` | Print the help. |
| `-V`, `--version` | Print the version. |

`TIME` takes any of these forms:

- `now`
- `@1774939200`, seconds since the Unix epoch
- An RFC 3339 time with an offset, such as `2026-03-31T06:40:00Z` or `2026-03-31 08:40:00+02:00`
- A wall-clock time followed by a zone name, such as `'2026-03-31 08:40 Europe/Berlin'`
- A wall-clock time or a date in the local zone, such as `'2026-03-31 02:40'` or `2026-03-31`

A wall-clock time in a daylight saving gap moves forward by the length of the gap, so `'2026-03-08 02:30'` in `America/New_York` is 03:30 EDT. A wall-clock time in a fold means its first pass.

Put `--` before an expression that starts with `-`.

## Exit status

| Code | Meaning |
| --- | --- |
| 0 | Every expression is valid and elapses, or `--count` is 0. |
| 1 | An expression is invalid. |
| 2 | Usage error: an unknown option or a bad option value. |
| 3 | A time zone name, in an expression or in `--zone`, is not in the time zone database. |
| 4 | An expression never elapses after `TIME`. |
| 5 | Standard input could not be read, or the output could not be written. |

When the expressions on standard input fail in different ways, the lowest code wins. A closed output pipe, as in `oncalendar daily | head -n 1`, is not an error.

## Time zones

The local time zone comes from `TZ` or the system settings, the same place systemd looks. Without either, as in many containers, `oncalendar` uses UTC and says so on standard error. `--zone` overrides it.

Names, in an expression and in `--zone`, are looked up in the system time zone database, usually `/usr/share/zoneinfo`. When the machine has none, as in a minimal container or on Windows, `oncalendar` uses the copy compiled into it, so names always work. `--bundled-tzdb` uses that copy even when the system has one, which makes the results the same on every machine. The two copies can differ when one is older than the other.

A name in an expression must match the database exactly, letter case included, as in systemd. `--zone` ignores case.

## Differences from systemd

`oncalendar` gives the same answers as `systemd-analyze calendar` except where the library lists a difference under [Known differences from systemd](https://github.com/chrysaor/oncalendar#known-differences-from-systemd). The one a user meets most is that an unknown zone name, such as `daily Asia/Seol`, is reported with exit status 3 after it parses, where systemd rejects the expression.

The output format is its own and is not the format of `systemd-analyze calendar`.

## Input limits

Input is treated as untrusted. An expression longer than 1 MiB, or a line of standard input that is not UTF-8, fails on its own and reading goes on. Every search for an elapse ends, so a list of 10,000 elapses of `*:*:0/0.000001` takes milliseconds.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
