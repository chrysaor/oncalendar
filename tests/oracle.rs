//! Differential test against real systemd.
//!
//! `tests/fixtures/cases.tsv` lists the inputs. `oracle/run.sh` runs them through
//! `systemd-analyze calendar` and writes `tests/fixtures/expected.tsv`. This test runs every row
//! through the public API, prints `passed N/M` and every mismatch, and fails if any row differs.
//!
//! systemd accepts a row when the expression parses and every time zone in it exists. This crate
//! checks the time zone when `upcoming` runs, so a row counts as accepted when both `parse` and
//! `upcoming` succeed.
//!
//! systemd sometimes parses an expression and then gives up searching for the next elapse
//! (status `unresolved`). This crate always finds an answer, so for those rows only the parse
//! result and the normalized form are compared.
#![cfg(feature = "jiff")]
#![allow(
    clippy::panic,
    clippy::arithmetic_side_effects,
    reason = "test code: a panic is a test failure"
)]

use std::fmt::Write as _;
use std::panic::{AssertUnwindSafe, catch_unwind};

use jiff::Timestamp;
use jiff::tz::TimeZoneDatabase;
use oncalendar::CalendarEvent;

const CASES: &str = include_str!("fixtures/cases.tsv");
const EXPECTED: &str = include_str!("fixtures/expected.tsv");

/// What systemd printed for one row.
#[derive(Debug)]
enum Expected {
    Rejected,
    Accepted {
        normalized: String,
        /// UTC seconds. Empty means "never".
        elapses: Vec<i64>,
    },
    /// Parsed, then systemd gave up on the elapses. It prints the normalized form only sometimes.
    Unresolved {
        normalized: Option<String>,
    },
}

/// What this crate does with one row.
#[derive(Debug)]
enum Actual {
    ParseRejected,
    /// Parsed, but `upcoming` reported an unknown time zone.
    ZoneRejected {
        normalized: String,
    },
    /// Parsed. `elapses` is `None` when the row did not ask for them.
    Accepted {
        normalized: String,
        elapses: Option<Vec<i64>>,
    },
}

impl Expected {
    fn agrees_with(&self, actual: &Actual) -> bool {
        match (self, actual) {
            (Expected::Rejected, Actual::ParseRejected | Actual::ZoneRejected { .. }) => true,
            (
                Expected::Accepted {
                    normalized,
                    elapses,
                },
                Actual::Accepted {
                    normalized: got,
                    elapses: Some(got_elapses),
                },
            ) => normalized == got && elapses == got_elapses,
            (
                Expected::Unresolved { normalized },
                Actual::Accepted {
                    normalized: got, ..
                },
            ) => normalized.as_ref().is_none_or(|n| n == got),
            _ => false,
        }
    }
}

struct Row<'a> {
    /// Line number in `cases.tsv`.
    line: usize,
    tz: &'a str,
    base: &'a str,
    expr: &'a str,
    expected: Expected,
}

#[test]
fn matches_systemd() {
    let (iterations, rows) = match load() {
        Ok(loaded) => loaded,
        Err(problem) => panic!("cannot read the fixtures: {problem}"),
    };
    let db = TimeZoneDatabase::bundled();

    let mut report = String::new();
    let mut passed = 0;
    for row in &rows {
        let actual = run(row, iterations, &db);
        if actual
            .as_ref()
            .is_ok_and(|actual| row.expected.agrees_with(actual))
        {
            passed += 1;
            continue;
        }
        let actual = match actual {
            Ok(actual) => describe_actual(&actual),
            Err(problem) => problem,
        };
        let _ = writeln!(
            report,
            "cases.tsv:{} [{}, {}] {}\n    expected: {}\n    actual:   {}",
            row.line,
            row.tz,
            row.base,
            shorten(row.expr),
            describe_expected(&row.expected),
            actual,
        );
    }

    println!("passed {passed}/{}", rows.len());
    if passed != rows.len() {
        print!("\nmismatches:\n{report}");
        panic!(
            "{} of {} oracle rows differ",
            rows.len() - passed,
            rows.len()
        );
    }
}

/// Runs one row through the crate. `Err` describes a panic or a row the harness cannot run.
fn run(row: &Row<'_>, iterations: usize, db: &TimeZoneDatabase) -> Result<Actual, String> {
    let local = db
        .get(row.tz)
        .map_err(|e| format!("harness: local time zone {:?}: {e}", row.tz))?;
    let base = parse_base(row.base)?;
    let wants_elapses = !matches!(row.expected, Expected::Unresolved { .. });
    catch_unwind(AssertUnwindSafe(|| {
        let Ok(event) = row.expr.parse::<CalendarEvent>() else {
            return Actual::ParseRejected;
        };
        let normalized = event.to_string();
        let Ok(upcoming) = event.upcoming(base, &local) else {
            return Actual::ZoneRejected { normalized };
        };
        let elapses = wants_elapses.then(|| {
            upcoming
                .take(iterations)
                .map(Timestamp::as_second)
                .collect()
        });
        Actual::Accepted {
            normalized,
            elapses,
        }
    }))
    .map_err(|_| "panicked".to_owned())
}

/// Reads both files and pairs each case with its expectation.
fn load() -> Result<(usize, Vec<Row<'static>>), String> {
    let header = EXPECTED.lines().next().unwrap_or_default();
    let iterations = header
        .rsplit_once("iterations=")
        .and_then(|(_, n)| n.trim().parse().ok())
        .ok_or_else(|| format!("expected.tsv: no iterations=N in the first line {header:?}"))?;

    let cases = data_lines(CASES);
    // The first line of expected.tsv is a comment, so it drops out here.
    let expectations = data_lines(EXPECTED);
    if cases.len() != expectations.len() {
        return Err(format!(
            "cases.tsv has {} rows and expected.tsv has {}. Regenerate expected.tsv.",
            cases.len(),
            expectations.len()
        ));
    }

    let mut rows = Vec::new();
    for ((line, case), (expected_line, expectation)) in cases.into_iter().zip(expectations) {
        let fields: Vec<&str> = expectation.split('\t').collect();
        let [tz, base, expr, status, normalized, elapses] = fields[..] else {
            return Err(format!("expected.tsv:{expected_line}: expected 6 fields"));
        };
        if case != [tz, base, expr].join("\t") {
            return Err(format!(
                "cases.tsv:{line} does not match expected.tsv:{expected_line}. Regenerate expected.tsv."
            ));
        }
        let expected = match status {
            "error" => Expected::Rejected,
            "ok" => Expected::Accepted {
                normalized: normalized.to_owned(),
                elapses: parse_elapses(elapses)
                    .map_err(|e| format!("expected.tsv:{expected_line}: {e}"))?,
            },
            "unresolved" => Expected::Unresolved {
                normalized: (normalized != "-").then(|| normalized.to_owned()),
            },
            other => {
                return Err(format!(
                    "expected.tsv:{expected_line}: unknown status {other:?}"
                ));
            }
        };
        rows.push(Row {
            line,
            tz,
            base,
            expr,
            expected,
        });
    }
    Ok((iterations, rows))
}

/// Rows that are neither empty nor comments, with their 1-based line numbers within `text`.
fn data_lines(text: &str) -> Vec<(usize, &str)> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.is_empty() && !line.starts_with('#'))
        .map(|(i, line)| (i + 1, line))
        .collect()
}

/// `2026-01-01 00:00:00 UTC` or `2026-01-01 00:00:00.5 UTC`, the form systemd's `--base-time` takes.
fn parse_base(base: &str) -> Result<Timestamp, String> {
    let civil = base
        .strip_suffix(" UTC")
        .ok_or_else(|| format!("harness: base time {base:?} must end in \" UTC\""))?;
    format!("{}Z", civil.replacen(' ', "T", 1))
        .parse()
        .map_err(|e| format!("harness: base time {base:?}: {e}"))
}

fn parse_elapses(elapses: &str) -> Result<Vec<i64>, String> {
    if elapses == "never" {
        return Ok(Vec::new());
    }
    elapses
        .split(',')
        .map(|t| {
            t.parse::<Timestamp>()
                .map(Timestamp::as_second)
                .map_err(|e| format!("elapse {t:?}: {e}"))
        })
        .collect()
}

fn describe_expected(expected: &Expected) -> String {
    match expected {
        Expected::Rejected => "rejected".to_owned(),
        Expected::Accepted {
            normalized,
            elapses,
        } => {
            format!("ok {} {}", shorten(normalized), describe_elapses(elapses))
        }
        Expected::Unresolved {
            normalized: Some(normalized),
        } => {
            format!(
                "parsed {} (systemd gave up on the elapses)",
                shorten(normalized)
            )
        }
        Expected::Unresolved { normalized: None } => {
            "parsed (systemd gave up on the elapses)".to_owned()
        }
    }
}

fn describe_actual(actual: &Actual) -> String {
    match actual {
        Actual::ParseRejected => "rejected by parse".to_owned(),
        Actual::ZoneRejected { normalized } => {
            format!(
                "parsed {}, then upcoming: unknown time zone",
                shorten(normalized)
            )
        }
        Actual::Accepted {
            normalized,
            elapses: Some(elapses),
        } => {
            format!("ok {} {}", shorten(normalized), describe_elapses(elapses))
        }
        Actual::Accepted {
            normalized,
            elapses: None,
        } => format!("parsed {}", shorten(normalized)),
    }
}

fn describe_elapses(elapses: &[i64]) -> String {
    if elapses.is_empty() {
        return "never".to_owned();
    }
    elapses
        .iter()
        .map(|&s| Timestamp::from_second(s).map_or_else(|_| s.to_string(), |t| t.to_string()))
        .collect::<Vec<_>>()
        .join(",")
}

/// Keeps the report readable for the 241-item rows.
fn shorten(text: &str) -> String {
    const KEEP: usize = 60;
    if text.chars().count() <= KEEP {
        return format!("{text:?}");
    }
    let head: String = text.chars().take(KEEP).collect();
    format!("{head:?}... ({} bytes)", text.len())
}
