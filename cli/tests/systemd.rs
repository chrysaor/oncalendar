#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::print_stdout,
    reason = "test code: a panic is a test failure"
)]

mod common;

use std::fmt::Write as _;

use common::{Json, parse_json, run};
use jiff::Timestamp;

const EXPECTED: &str = include_str!("../../tests/fixtures/expected.tsv");

#[derive(Debug)]
enum Expected {
    Rejected,
    Accepted {
        normalized: String,
        elapses: Vec<i64>,
    },
    Unresolved {
        normalized: Option<String>,
    },
}

#[derive(Debug)]
enum Actual {
    ParseRejected,
    ZoneRejected,
    Accepted {
        normalized: String,
        elapses: Vec<i64>,
    },
}

impl Expected {
    fn agrees_with(&self, actual: &Actual) -> bool {
        match (self, actual) {
            (Expected::Rejected, Actual::ParseRejected | Actual::ZoneRejected) => true,
            (
                Expected::Accepted {
                    normalized,
                    elapses,
                },
                Actual::Accepted {
                    normalized: got,
                    elapses: got_elapses,
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

    fn allows_exit(&self, code: i32) -> bool {
        match self {
            Expected::Rejected => code == 1 || code == 3,
            Expected::Accepted { elapses, .. } if elapses.is_empty() => code == 4,
            Expected::Accepted { .. } => code == 0,
            Expected::Unresolved { .. } => code == 0 || code == 4,
        }
    }
}

struct Row<'a> {
    line: usize,
    tz: &'a str,
    base: &'a str,
    expr: &'a str,
    expected: Expected,
}

fn load() -> (String, Vec<Row<'static>>) {
    let header = EXPECTED.lines().next().unwrap_or_default();
    let iterations = header
        .rsplit_once("iterations=")
        .map(|(_, n)| n.trim().to_owned())
        .expect("the first line of expected.tsv names the iterations");
    let mut rows = Vec::new();
    for (index, line) in EXPECTED.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        let [tz, base, expr, status, normalized, elapses] = fields[..] else {
            panic!("expected.tsv:{}: expected 6 fields", index + 1);
        };
        let expected = match status {
            "error" => Expected::Rejected,
            "ok" => Expected::Accepted {
                normalized: normalized.to_owned(),
                elapses: seconds(elapses),
            },
            "unresolved" => Expected::Unresolved {
                normalized: (normalized != "-").then(|| normalized.to_owned()),
            },
            other => panic!("expected.tsv:{}: unknown status {other:?}", index + 1),
        };
        rows.push(Row {
            line: index + 1,
            tz,
            base,
            expr,
            expected,
        });
    }
    (iterations, rows)
}

fn seconds(elapses: &str) -> Vec<i64> {
    if elapses == "never" {
        return Vec::new();
    }
    elapses
        .split(',')
        .map(|t| {
            t.parse::<Timestamp>()
                .expect("an RFC 3339 elapse")
                .as_second()
        })
        .collect()
}

fn actual(object: &Json) -> Result<Actual, String> {
    let normalized = object.str("normalized").map(str::to_owned);
    match (object.str("error"), normalized) {
        (Some("invalid-expression"), None) => Ok(Actual::ParseRejected),
        (Some("unknown-time-zone"), Some(_)) => Ok(Actual::ZoneRejected),
        (None, Some(normalized)) => {
            let elapses = object
                .strings("elapses")
                .ok_or("no elapses array")?
                .iter()
                .map(|t| {
                    t.parse::<Timestamp>()
                        .map(Timestamp::as_second)
                        .map_err(|e| e.to_string())
                })
                .collect::<Result<_, _>>()?;
            Ok(Actual::Accepted {
                normalized,
                elapses,
            })
        }
        _ => Err(format!("unexpected object {object:?}")),
    }
}

fn options<'a>(row: &Row<'a>, iterations: &'a str) -> Vec<&'a str> {
    vec![
        "--json",
        "--bundled-tzdb",
        "--zone",
        row.tz,
        "--after",
        row.base,
        "--count",
        iterations,
    ]
}

fn finish(report: &str, failures: usize, total: usize) {
    println!("passed {}/{total}", total - failures);
    if failures > 0 {
        print!("\nmismatches:\n{report}");
        panic!("{failures} of {total} fixture rows differ from systemd");
    }
}

#[test]
fn every_fixture_row_given_as_arguments_matches_systemd() {
    let (iterations, rows) = load();
    let mut report = String::new();
    let mut failures = 0;
    for row in &rows {
        let mut args = options(row, &iterations);
        args.extend(["--", row.expr]);
        let result = run(&args, None);
        let object = result.stdout.strip_suffix('\n').map(parse_json);
        let verdict = match object {
            Some(Ok(object)) => actual(&object),
            Some(Err(problem)) => Err(format!("invalid JSON: {problem}")),
            None => Err(format!("no output line, stderr {:?}", result.stderr)),
        };
        let agrees = verdict
            .as_ref()
            .is_ok_and(|actual| row.expected.agrees_with(actual));
        if !agrees || !row.expected.allows_exit(result.code) || result.stdout.lines().count() != 1 {
            failures += 1;
            let _ = writeln!(
                report,
                "expected.tsv:{} [{}, {}] {:?}\n    expected: {:?}\n    actual:   {:?}, exit {}",
                row.line, row.tz, row.base, row.expr, row.expected, verdict, result.code
            );
        }
    }
    finish(&report, failures, rows.len());
}

#[test]
fn fixture_rows_given_on_standard_input_match_systemd() {
    let (iterations, rows) = load();
    let mut groups: Vec<Vec<&Row<'_>>> = Vec::new();
    for row in rows
        .iter()
        .filter(|row| !row.expr.is_empty() && !row.expr.starts_with('#'))
    {
        match groups.last_mut() {
            Some(group) if group[0].tz == row.tz && group[0].base == row.base => group.push(row),
            _ => groups.push(vec![row]),
        }
    }
    let covered: usize = groups.iter().map(Vec::len).sum();
    assert!(
        covered + 5 >= rows.len(),
        "only {covered} of {} rows fit on standard input",
        rows.len()
    );

    let mut report = String::new();
    let mut failures = 0;
    for group in &groups {
        let input: String = group.iter().map(|row| format!("{}\n", row.expr)).collect();
        let result = run(&options(group[0], &iterations), Some(input.as_bytes()));
        let objects: Vec<Result<Json, String>> = result.stdout.lines().map(parse_json).collect();
        for (index, row) in group.iter().enumerate() {
            let verdict = match objects.get(index) {
                Some(Ok(object)) if object.number("line") == i64::try_from(index + 1).ok() => {
                    actual(object)
                }
                Some(Ok(object)) => Err(format!("out of order: {object:?}")),
                Some(Err(problem)) => Err(format!("invalid JSON: {problem}")),
                None => Err(format!("missing, stderr {:?}", result.stderr)),
            };
            if !verdict
                .as_ref()
                .is_ok_and(|actual| row.expected.agrees_with(actual))
            {
                failures += 1;
                let _ = writeln!(
                    report,
                    "expected.tsv:{} [{}, {}] {:?}\n    expected: {:?}\n    actual:   {:?}",
                    row.line, row.tz, row.base, row.expr, row.expected, verdict
                );
            }
        }
        if objects.len() != group.len() {
            failures += 1;
            let _ = writeln!(
                report,
                "group at expected.tsv:{}: {} objects for {} rows",
                group[0].line,
                objects.len(),
                group.len()
            );
        }
    }
    finish(&report, failures, covered);
}
