//! Parsing and normalization against real systemd, in every feature configuration.
//!
//! `tests/oracle.rs` needs the `jiff` feature. This test does not, so the build without
//! dependencies is also compared with systemd. It reads the rows of
//! `tests/fixtures/expected.tsv` that systemd accepted and checks that each expression parses and
//! prints the same normalized form.
//!
//! Rejected rows are left to `tests/oracle.rs`: systemd also rejects an unknown time zone, and
//! this crate can tell an unknown zone only with the `jiff` feature.
#![allow(
    clippy::panic,
    clippy::arithmetic_side_effects,
    reason = "test code: a panic is a test failure"
)]

use oncalendar::CalendarEvent;

const EXPECTED: &str = include_str!("fixtures/expected.tsv");

#[test]
fn accepted_expressions_normalize_like_systemd() {
    let mut checked = 0_usize;
    let mut failures = Vec::new();
    for (index, line) in EXPECTED.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let columns: Vec<&str> = line.split('\t').collect();
        let [_, _, expr, status, normalized, _] = columns.as_slice() else {
            panic!("expected.tsv:{}: expected 6 columns", index + 1);
        };
        if *status != "ok" {
            continue;
        }
        checked += 1;
        match expr.parse::<CalendarEvent>() {
            Ok(event) if event.to_string() == *normalized => {}
            Ok(event) => failures.push(format!(
                "expected.tsv:{}: printed {event}, systemd prints {normalized}",
                index + 1
            )),
            Err(_) => failures.push(format!(
                "expected.tsv:{}: rejected, systemd prints {normalized}",
                index + 1
            )),
        }
    }
    assert!(
        checked > 0,
        "not exercised: no accepted row in the fixtures"
    );
    assert!(
        failures.is_empty(),
        "{} of {checked} accepted rows differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
