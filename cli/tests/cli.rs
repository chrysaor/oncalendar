#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "test code: a panic is a test failure"
)]

mod common;

use std::time::{Duration, Instant};

use common::{parse_json, run};
use jiff::Timestamp;

const FIXED: [&str; 5] = [
    "--bundled-tzdb",
    "--zone",
    "America/New_York",
    "--after",
    "2026-10-31 12:00",
];

fn with_fixed<'a>(extra: &[&'a str]) -> Vec<&'a str> {
    let mut args = FIXED.to_vec();
    args.extend_from_slice(extra);
    args
}

#[test]
fn prints_the_normalized_form_and_the_next_elapses_in_the_local_zone() {
    let result = run(&with_fixed(&["-n", "3", "*-*-* 01:30"]), None);
    assert_eq!(result.code, 0);
    assert_eq!(result.stderr, "");
    assert_eq!(
        result.stdout,
        "*-*-* 01:30:00\n  \
         Sun 2026-11-01 01:30:00 EDT (-04:00)  in 13h 30m\n  \
         Mon 2026-11-02 01:30:00 EST (-05:00)  in 1d 14h\n  \
         Tue 2026-11-03 01:30:00 EST (-05:00)  in 2d 14h\n"
    );
}

#[test]
fn joins_the_words_of_an_unquoted_expression() {
    let quoted = run(&with_fixed(&["Mon..Fri 9:00"]), None);
    let unquoted = run(&with_fixed(&["Mon..Fri", "9:00"]), None);
    assert_eq!(quoted.code, 0);
    assert_eq!(quoted.stdout, unquoted.stdout);
    assert!(quoted.stdout.starts_with("Mon..Fri *-*-* 09:00:00\n"));
}

#[test]
fn an_invalid_expression_points_at_the_problem_and_exits_1() {
    let result = run(&with_fixed(&["Mon *-*-* 24:00"]), None);
    assert_eq!(result.code, 1);
    assert_eq!(result.stdout, "");
    assert_eq!(
        result.stderr,
        "error: calendar expression: hour out of range at byte 10\n  \
         Mon *-*-* 24:00\n  \
         \x20         ^\n"
    );
}

#[test]
fn control_and_non_ascii_characters_are_escaped_in_errors() {
    let result = run(
        &with_fixed(&["daily\u{1b}[2J\nforged Mon\u{2013}Fri"]),
        None,
    );
    assert_eq!(result.code, 1);
    assert!(!result.stderr.contains('\u{1b}'));
    assert!(
        result
            .stderr
            .contains("daily\\u{1b}[2J\\nforged Mon\\u{2013}Fri")
    );
    assert_eq!(result.stderr.lines().count(), 3);
}

#[test]
fn a_long_expression_is_shown_around_the_error() {
    let expression = format!("*:{} 25:00", vec!["1"; 400].join(","));
    let result = run(&with_fixed(&[&expression]), None);
    assert_eq!(result.code, 1);
    let lines: Vec<&str> = result.stderr.lines().collect();
    assert_eq!(lines.len(), 3);
    assert!(lines[1].len() < 90, "{:?}", lines[1]);
    assert!(lines[1].starts_with("  ..."));
}

#[test]
fn an_unknown_zone_in_the_expression_exits_3() {
    let result = run(&with_fixed(&["daily Asia/Seol"]), None);
    assert_eq!(result.code, 3);
    assert_eq!(result.stdout, "");
    assert!(result.stderr.contains("not in the time zone database"));
    assert!(result.stderr.contains("  daily Asia/Seol\n        ^\n"));
    assert!(result.stderr.contains("parsed as *-*-* 00:00:00 Asia/Seol"));
}

#[test]
fn an_unknown_local_zone_exits_3() {
    let result = run(&["--zone", "Mars/Olympus", "daily"], None);
    assert_eq!(result.code, 3);
    assert!(result.stderr.starts_with("error: --zone:"));
}

#[test]
fn an_expression_that_never_elapses_exits_4() {
    let result = run(&with_fixed(&["2020-01-01"]), None);
    assert_eq!(result.code, 4);
    assert_eq!(
        result.stdout,
        "2020-01-01 00:00:00\n  never elapses after Sat 2026-10-31 12:00:00 EDT (-04:00)\n"
    );
}

#[test]
fn the_last_elapses_before_2200_end_the_list() {
    let result = run(
        &[
            "--zone",
            "UTC",
            "--after",
            "2199-12-31 23:59:58",
            "-n",
            "5",
            "*:*:*",
        ],
        None,
    );
    assert_eq!(result.code, 0);
    assert_eq!(
        result.stdout,
        "*-*-* *:*:*\n  Tue 2199-12-31 23:59:59 UTC  in 1s\n  no further elapses\n"
    );
}

#[test]
fn count_zero_only_checks() {
    let valid = run(&with_fixed(&["-n", "0", "2020-01-01"]), None);
    assert_eq!(valid.code, 0);
    assert_eq!(valid.stdout, "2020-01-01 00:00:00\n");
    let unknown = run(&with_fixed(&["-n", "0", "daily Nowhere/Bogus"]), None);
    assert_eq!(unknown.code, 3);
}

#[test]
fn usage_errors_exit_2() {
    for args in [
        vec!["--bogus", "daily"],
        vec!["-n"],
        vec!["-n", "10001", "daily"],
        vec!["-n", "-1", "daily"],
        vec!["--after", "yesterday", "daily"],
        vec!["--json=yes", "daily"],
    ] {
        let result = run(&args, None);
        assert_eq!(result.code, 2, "{args:?}");
        assert_eq!(result.stdout, "", "{args:?}");
        assert!(result.stderr.starts_with("error: "), "{args:?}");
    }
}

#[test]
fn help_and_version() {
    let help = run(&["--help"], None);
    assert_eq!(help.code, 0);
    assert!(
        help.stdout
            .contains("Usage: oncalendar [OPTIONS] [EXPRESSION...]")
    );
    assert!(help.stdout.contains("Exit status:"));
    let version = run(&["-V"], None);
    assert_eq!(version.code, 0);
    assert_eq!(
        version.stdout,
        format!("oncalendar {}\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn every_form_of_after_names_the_same_instant() {
    let forms = [
        "@1774939200",
        "2026-03-31T06:40:00Z",
        "2026-03-31 08:40:00+02:00",
        "2026-03-31 08:40 Europe/Berlin",
        "2026-03-31T02:40",
    ];
    let outputs: Vec<String> = forms
        .iter()
        .map(|after| {
            let result = run(
                &[
                    "--bundled-tzdb",
                    "--zone",
                    "America/New_York",
                    "--json",
                    "--after",
                    after,
                    "hourly",
                ],
                None,
            );
            assert_eq!(result.code, 0, "{after}");
            result.stdout
        })
        .collect();
    assert!(
        outputs.iter().all(|output| *output == outputs[0]),
        "{outputs:#?}"
    );
    assert!(outputs[0].contains("\"2026-03-31T03:00:00-04:00\""));
}

#[test]
fn standard_input_takes_one_expression_per_line() {
    let input = b"# timers\r\ndaily\r\n\r\n*-*-* 25:00\nweekly\n";
    let result = run(&with_fixed(&["-n", "1", "--json"]), Some(input));
    assert_eq!(result.code, 1);
    let objects: Vec<_> = result
        .stdout
        .lines()
        .map(|line| parse_json(line).expect("each line is JSON"))
        .collect();
    assert_eq!(objects.len(), 3);
    assert_eq!(objects[0].number("line"), Some(2));
    assert_eq!(objects[0].str("normalized"), Some("*-*-* 00:00:00"));
    assert_eq!(
        objects[0].strings("elapses"),
        Some(vec!["2026-11-01T00:00:00-04:00".to_owned()])
    );
    assert_eq!(objects[1].number("line"), Some(4));
    assert_eq!(objects[1].str("error"), Some("invalid-expression"));
    assert_eq!(objects[1].number("offset"), Some(6));
    assert_eq!(objects[2].str("normalized"), Some("Mon *-*-* 00:00:00"));
}

#[test]
fn human_output_on_standard_input_separates_blocks_and_names_lines() {
    let result = run(
        &with_fixed(&["-n", "1"]),
        Some(b"daily\nMon 25:00\nweekly\n"),
    );
    assert_eq!(result.code, 1);
    assert_eq!(
        result.stdout,
        "*-*-* 00:00:00\n  Sun 2026-11-01 00:00:00 EDT (-04:00)  in 12h\n\n\
         Mon *-*-* 00:00:00\n  Mon 2026-11-02 00:00:00 EST (-05:00)  in 1d 13h\n"
    );
    assert!(
        result
            .stderr
            .starts_with("\nerror: line 2: calendar expression: hour out of range at byte 4\n")
    );
}

#[test]
fn the_lowest_failing_exit_code_wins() {
    let never_and_zone = run(
        &with_fixed(&[]),
        Some(b"2020-01-01\ndaily Nowhere/Bogus\ndaily\n"),
    );
    assert_eq!(never_and_zone.code, 3);
    let never_only = run(&with_fixed(&[]), Some(b"daily\n2020-01-01\n"));
    assert_eq!(never_only.code, 4);
}

#[test]
fn hostile_lines_fail_alone_and_reading_goes_on() {
    let mut input = vec![b'0'; 3 << 20];
    input.extend_from_slice(b"\ndai\xffly\ndaily\n");
    let result = run(&with_fixed(&["--json", "-n", "1"]), Some(&input));
    assert_eq!(result.code, 1);
    let objects: Vec<_> = result
        .stdout
        .lines()
        .map(|line| parse_json(line).expect("each line is JSON"))
        .collect();
    assert_eq!(objects.len(), 3);
    assert_eq!(
        objects[0].str("message"),
        Some("calendar expression: longer than 1048576 bytes")
    );
    assert_eq!(
        objects[1].str("message"),
        Some("calendar expression: not valid UTF-8 at byte 3")
    );
    assert_eq!(objects[2].str("normalized"), Some("*-*-* 00:00:00"));
}

#[test]
fn json_escapes_what_it_echoes() {
    let result = run(&with_fixed(&["--json", "\"\\\u{7}\u{2028}"]), None);
    assert_eq!(result.code, 1);
    let line = result.stdout.strip_suffix('\n').expect("one line");
    assert!(!line.contains('\u{7}') && !line.contains('\u{2028}'));
    let object = parse_json(line).expect("valid JSON");
    assert_eq!(object.str("expression"), Some("\"\\\u{7}\u{2028}"));
}

#[test]
fn the_longest_allowed_list_of_a_dense_expression_ends_quickly() {
    let start = Instant::now();
    let result = run(
        &with_fixed(&["--json", "-n", "10000", "*:*:0/0.000001"]),
        None,
    );
    assert!(start.elapsed() < Duration::from_secs(10));
    assert_eq!(result.code, 0);
    let object = parse_json(result.stdout.trim_end()).expect("valid JSON");
    let elapses = object.strings("elapses").expect("elapses");
    assert_eq!(elapses.len(), 10_000);
    let first: Timestamp = elapses[0].parse().expect("a time");
    let last: Timestamp = elapses[9_999].parse().expect("a time");
    assert_eq!(last.as_microsecond() - first.as_microsecond(), 9_999);
}

#[test]
fn an_expression_that_starts_with_a_dash_follows_two_dashes() {
    let result = run(&with_fixed(&["--", "-1"]), None);
    assert_eq!(result.code, 1);
    assert!(result.stderr.starts_with("error: calendar expression:"));
}
