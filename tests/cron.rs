#![allow(
    missing_docs,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "test code: a panic is a test failure"
)]

use std::panic::catch_unwind;
use std::time::{Duration, Instant};

use jiff::civil::Date;
use jiff::tz::TimeZone;
use jiff::{Timestamp, ToSpan as _};
use oncalendar::{CalendarEvent, CronError};

const CRON: &str = include_str!("fixtures/cron.tsv");
const EXPECTED: &str = include_str!("fixtures/expected.tsv");
const TIME_LIMIT: Duration = Duration::from_secs(5);
const MAX_MESSAGE_LEN: usize = 120;

struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: u32) -> u32 {
        (self.next_u64() % u64::from(n)) as u32
    }

    fn range(&mut self, lo: u32, hi: u32) -> u32 {
        lo + self.below(hi - lo + 1)
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len() as u32) as usize]
    }
}

enum Row {
    Converts(Vec<String>),
    Rejected(String),
}

fn fixture_rows() -> Vec<(usize, &'static str, Row)> {
    CRON.lines()
        .enumerate()
        .filter(|(_, line)| !line.is_empty() && !line.starts_with('#'))
        .map(|(index, line)| {
            let mut columns = line.split('\t');
            let cron = columns.next().unwrap();
            let rest: Vec<String> = columns.map(str::to_owned).collect();
            let row = match rest.as_slice() {
                [status, message] if status == "error" => Row::Rejected(message.clone()),
                [] => panic!("cron.tsv:{}: no result column", index + 1),
                events => Row::Converts(events.to_vec()),
            };
            (index + 1, cron, row)
        })
        .collect()
}

fn convert(cron: &str) -> Result<Vec<String>, CronError> {
    CalendarEvent::from_cron(cron).map(|events| events.iter().map(ToString::to_string).collect())
}

#[test]
fn fixture_rows_convert_as_listed() {
    let mut failures = Vec::new();
    let rows = fixture_rows();
    for (line, cron, row) in &rows {
        let got = convert(cron);
        let agrees = match (row, &got) {
            (Row::Converts(events), Ok(got)) => events == got,
            (Row::Rejected(message), Err(error)) => *message == error.to_string(),
            _ => false,
        };
        if !agrees {
            failures.push(format!("cron.tsv:{line}: {cron:?} gives {got:?}"));
        }
    }
    assert!(rows.len() > 50, "not exercised: {} rows", rows.len());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn every_converted_event_is_a_systemd_fixture_row() {
    let accepted: Vec<(&str, &str)> = EXPECTED
        .lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| {
            let columns: Vec<&str> = line.split('\t').collect();
            match columns.as_slice() {
                [_, _, expr, "ok", normalized, _] => Some((*expr, *normalized)),
                _ => None,
            }
        })
        .collect();
    let mut missing = Vec::new();
    for (line, _, row) in fixture_rows() {
        let Row::Converts(events) = row else { continue };
        for event in events {
            if !accepted
                .iter()
                .any(|(expr, normalized)| *expr == event && *normalized == event)
            {
                missing.push(format!("cron.tsv:{line}: {event}"));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "add these events to the cron section of tests/fixtures/cases.tsv and regenerate expected.tsv:\n{}",
        missing.join("\n")
    );
}

struct Reference {
    minutes: [bool; 60],
    hours: [bool; 24],
    days: [bool; 32],
    months: [bool; 13],
    weekdays: [bool; 7],
    days_unrestricted: bool,
    weekdays_unrestricted: bool,
}

impl Reference {
    fn new(cron: &str) -> Reference {
        let expanded = match cron.trim().to_ascii_lowercase().as_str() {
            "@yearly" | "@annually" => "0 0 1 1 *".to_owned(),
            "@monthly" => "0 0 1 * *".to_owned(),
            "@weekly" => "0 0 * * 0".to_owned(),
            "@daily" | "@midnight" => "0 0 * * *".to_owned(),
            "@hourly" => "0 * * * *".to_owned(),
            _ => cron.to_owned(),
        };
        let fields: Vec<&str> = expanded.split_whitespace().collect();
        assert_eq!(fields.len(), 5, "reference: {cron:?} has not 5 fields");
        let mut reference = Reference {
            minutes: [false; 60],
            hours: [false; 24],
            days: [false; 32],
            months: [false; 13],
            weekdays: [false; 7],
            days_unrestricted: fields[2].starts_with('*'),
            weekdays_unrestricted: fields[4].starts_with('*'),
        };
        for v in values(fields[0], 0, 59, &[]) {
            reference.minutes[v as usize] = true;
        }
        for v in values(fields[1], 0, 23, &[]) {
            reference.hours[v as usize] = true;
        }
        for v in values(fields[2], 1, 31, &[]) {
            reference.days[v as usize] = true;
        }
        let months = [
            "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
        ];
        for v in values(fields[3], 1, 12, &months) {
            reference.months[v as usize] = true;
        }
        let days = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];
        for v in values(fields[4], 0, 7, &days) {
            reference.weekdays[(v % 7) as usize] = true;
        }
        reference
    }

    fn day_matches(&self, day: Date) -> bool {
        let by_day = self.days[day.day() as usize];
        let by_weekday = self.weekdays[day.weekday().to_sunday_zero_offset() as usize];
        let by_days = if self.days_unrestricted || self.weekdays_unrestricted {
            by_day && by_weekday
        } else {
            by_day || by_weekday
        };
        self.months[day.month() as usize] && by_days
    }
}

fn values(field: &str, lo: u32, hi: u32, names: &[&str]) -> Vec<u32> {
    let number = |text: &str| -> u32 {
        let lower = text.to_ascii_lowercase();
        match names.iter().position(|name| *name == lower) {
            Some(index) => lo + index as u32,
            None => text.parse().expect("reference: a number"),
        }
    };
    let mut out = Vec::new();
    for element in field.split(',') {
        let (span, step) = match element.split_once('/') {
            Some((span, step)) => (span, step.parse::<u32>().expect("reference: a step")),
            None => (element, 1),
        };
        let (first, last) = if span == "*" {
            (lo, hi)
        } else if let Some((a, b)) = span.split_once('-') {
            (number(a), number(b))
        } else {
            (number(span), number(span))
        };
        let mut v = first;
        while v <= last {
            out.push(v);
            v += step;
        }
    }
    out
}

fn reference_elapses(cron: &str, after: Timestamp, until: Timestamp, cap: usize) -> Vec<Timestamp> {
    let reference = Reference::new(cron);
    let mut out = Vec::new();
    let mut day = TimeZone::UTC.to_datetime(after).date();
    let last = TimeZone::UTC.to_datetime(until).date();
    while day <= last {
        if reference.day_matches(day) {
            for hour in 0..24_i8 {
                for minute in 0..60_i8 {
                    if !(reference.hours[hour as usize] && reference.minutes[minute as usize]) {
                        continue;
                    }
                    let instant = TimeZone::UTC
                        .to_timestamp(day.at(hour, minute, 0, 0))
                        .unwrap();
                    if instant > after && instant <= until {
                        out.push(instant);
                        if out.len() == cap {
                            return out;
                        }
                    }
                }
            }
        }
        day = day.tomorrow().unwrap();
    }
    out
}

fn converted_elapses(
    events: &[CalendarEvent],
    after: Timestamp,
    until: Timestamp,
) -> Vec<Timestamp> {
    let mut out: Vec<Timestamp> = events
        .iter()
        .flat_map(|event| {
            event
                .upcoming(after, &TimeZone::UTC)
                .unwrap()
                .take_while(move |t| *t <= until)
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

struct Comparison {
    cases: usize,
    elapses: usize,
    failures: Vec<String>,
}

impl Comparison {
    fn new() -> Self {
        Self {
            cases: 0,
            elapses: 0,
            failures: Vec::new(),
        }
    }

    fn check(&mut self, cron: &str, after: Timestamp, until: Timestamp, cap: usize) {
        let events = CalendarEvent::from_cron(cron)
            .unwrap_or_else(|error| panic!("{cron:?} does not convert: {error}"));
        let expected = reference_elapses(cron, after, until, cap);
        let until = if expected.len() == cap {
            *expected.last().unwrap()
        } else {
            until
        };
        let got = converted_elapses(&events, after, until);
        self.cases += 1;
        self.elapses += expected.len();
        if got != expected {
            let first = got
                .iter()
                .zip(&expected)
                .position(|(g, e)| g != e)
                .unwrap_or(got.len().min(expected.len()));
            let shown: Vec<String> = events.iter().map(ToString::to_string).collect();
            self.failures.push(format!(
                "{cron:?} as {shown:?} after {after}: {} elapses, cron {}; first difference at {first}: {:?} against cron {:?}",
                got.len(),
                expected.len(),
                got.get(first),
                expected.get(first)
            ));
        }
    }

    fn finish(self, what: &str) {
        println!(
            "{what}: {} cases, {} elapses compared",
            self.cases, self.elapses
        );
        assert!(
            self.failures.is_empty(),
            "{what}: {} of {} cases differ from cron:\n{}",
            self.failures.len(),
            self.cases,
            self.failures
                .iter()
                .take(10)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

fn instant(civil: &str) -> Timestamp {
    format!("{civil}Z").parse().unwrap()
}

#[test]
fn fixture_rows_elapse_when_cron_runs() {
    let after = instant("2023-12-31T23:59:59");
    let until = instant("2032-01-01T00:00:00");
    let mut comparison = Comparison::new();
    for (_, cron, row) in fixture_rows() {
        if let Row::Converts(_) = row {
            comparison.check(cron, after, until, 20_000);
        }
    }
    comparison.finish("fixture rows over 2024 to 2031");
}

fn random_field(rng: &mut Rng, lo: u32, hi: u32, names: &[&str]) -> String {
    let spell = |rng: &mut Rng, v: u32| -> String {
        let index = (v - lo) as usize;
        if !names.is_empty() && index < names.len() && rng.below(3) == 0 {
            let name = names[index];
            match rng.below(3) {
                0 => name.to_owned(),
                1 => name.to_ascii_uppercase(),
                _ => {
                    let mut chars = name.chars();
                    let first = chars.next().unwrap().to_ascii_uppercase();
                    std::iter::once(first).chain(chars).collect()
                }
            }
        } else {
            v.to_string()
        }
    };
    match rng.below(10) {
        0 | 1 => "*".to_owned(),
        2 => format!("*/{}", rng.range(1, hi)),
        _ => {
            let count = rng.range(1, 3);
            (0..count)
                .map(|_| match rng.below(4) {
                    0 | 1 => {
                        let v = rng.range(lo, hi);
                        spell(rng, v)
                    }
                    2 => {
                        let a = rng.range(lo, hi);
                        let b = rng.range(a, hi);
                        format!("{}-{}", spell(rng, a), spell(rng, b))
                    }
                    _ => {
                        let a = rng.range(lo, hi);
                        let b = rng.range(a, hi);
                        let step = rng.range(1, hi);
                        format!("{}-{}/{step}", spell(rng, a), spell(rng, b))
                    }
                })
                .collect::<Vec<_>>()
                .join(",")
        }
    }
}

const MONTHS: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];
const WEEKDAYS: [&str; 7] = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];

fn random_cron(rng: &mut Rng) -> String {
    format!(
        "{} {} {} {} {}",
        random_field(rng, 0, 59, &[]),
        random_field(rng, 0, 23, &[]),
        random_field(rng, 1, 31, &[]),
        random_field(rng, 1, 12, &MONTHS),
        random_field(rng, 0, 7, &WEEKDAYS),
    )
}

fn random_after(rng: &mut Rng) -> Timestamp {
    let start = instant("2024-01-01T00:00:00").as_second();
    let minutes = rng.below(6 * 366 * 24 * 60);
    Timestamp::from_second(start + i64::from(minutes) * 60 - i64::from(rng.below(2))).unwrap()
}

#[test]
fn random_expressions_elapse_when_cron_runs() {
    let mut rng = Rng(0x0c0f_fee5);
    let mut comparison = Comparison::new();
    for _ in 0..1_500 {
        let cron = random_cron(&mut rng);
        let after = random_after(&mut rng);
        let until = after + (4 * 366 * 24).hours();
        comparison.check(&cron, after, until, 1_500);
    }
    comparison.finish("random expressions over 4 years each");
}

#[test]
#[ignore = "a long run: cargo test --release --all-features --test cron -- --ignored"]
fn many_random_expressions_elapse_when_cron_runs() {
    let mut rng = Rng(0x5eed_0001);
    let mut comparison = Comparison::new();
    for _ in 0..50_000 {
        let cron = random_cron(&mut rng);
        let after = random_after(&mut rng);
        let until = after + (8 * 366 * 24).hours();
        comparison.check(&cron, after, until, 5_000);
    }
    comparison.finish("random expressions over 8 years each");
}

#[test]
#[ignore = "prints cases for oracle/run.sh and scripts/verify compare"]
fn print_random_conversions_for_oracle() {
    let mut rng = Rng(0x0dd5_eed5);
    for _ in 0..300 {
        let cron = random_cron(&mut rng);
        let after = random_after(&mut rng).round(jiff::Unit::Second).unwrap();
        let base = TimeZone::UTC
            .to_datetime(after)
            .to_string()
            .replacen('T', " ", 1);
        for event in CalendarEvent::from_cron(&cron).unwrap() {
            println!("UTC\t{base} UTC\t{event}");
        }
    }
}

fn hostile_inputs() -> Vec<String> {
    let alphabet: Vec<char> = "0123456789*,/- \t@LW#?janfebmonsunJANMON\u{0}\n\u{1b}é０－"
        .chars()
        .collect();
    let mut rng = Rng(0xbad_c0de);
    let mut inputs: Vec<String> = Vec::new();
    for _ in 0..20_000 {
        let len = rng.below(30);
        inputs.push((0..len).map(|_| *rng.pick(&alphabet)).collect());
    }
    for _ in 0..20_000 {
        let mut chars: Vec<char> = random_cron(&mut rng).chars().collect();
        for _ in 0..rng.range(1, 3) {
            let at = rng.below(chars.len() as u32 + 1) as usize;
            let c = *rng.pick(&alphabet);
            match rng.below(3) {
                0 => chars.insert(at, c),
                1 if at < chars.len() => {
                    chars.remove(at);
                }
                _ if at < chars.len() => chars[at] = c,
                _ => chars.push(c),
            }
        }
        inputs.push(chars.into_iter().collect());
    }
    for (_, cron, _) in fixture_rows() {
        inputs.push(cron.to_owned());
    }
    inputs
}

#[test]
fn hostile_input_never_panics_and_errors_do_not_echo_it() {
    const MARKER: &str = "Xq9INJECT";
    let mut inputs = hostile_inputs();
    inputs.extend(
        [
            "Xq9INJECT * * * *",
            "* * * * Xq9INJECT",
            "@Xq9INJECT",
            "0 0 * * * Xq9INJECT",
            "0 0 1 Xq9INJECT\nFAKE LOG LINE *",
        ]
        .map(str::to_owned),
    );
    let mut failures = Vec::new();
    let mut accepted = 0;
    for input in &inputs {
        let Ok(result) = catch_unwind(|| CalendarEvent::from_cron(input)) else {
            failures.push(format!("{input:?}: panicked"));
            continue;
        };
        match result {
            Ok(events) => {
                accepted += 1;
                if !(1..=2).contains(&events.len()) {
                    failures.push(format!("{input:?}: {} events", events.len()));
                }
                for event in events {
                    let text = event.to_string();
                    if text.parse::<CalendarEvent>().as_ref() != Ok(&event) {
                        failures.push(format!("{input:?}: {text:?} does not parse back"));
                    }
                }
            }
            Err(error) => {
                let message = error.to_string();
                if error.offset() > input.len() || !input.is_char_boundary(error.offset()) {
                    failures.push(format!("{input:?}: offset {}", error.offset()));
                }
                if message.len() > MAX_MESSAGE_LEN
                    || !message.bytes().all(|b| (0x20..=0x7e).contains(&b))
                {
                    failures.push(format!("{input:?}: message {message:?}"));
                }
                if input.contains(MARKER) && message.contains(MARKER) {
                    failures.push(format!("{input:?}: message repeats the input"));
                }
            }
        }
    }
    assert!(
        accepted > 1_000,
        "not exercised: {accepted} accepted inputs"
    );
    assert!(
        failures.is_empty(),
        "{} of {} inputs fail:\n{}",
        failures.len(),
        inputs.len(),
        failures
            .iter()
            .take(10)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn input_length_and_item_limits() {
    let padded = format!("0 0 * * *{}", " ".repeat(1024 - 9));
    assert!(CalendarEvent::from_cron(&padded).is_ok());
    let error = CalendarEvent::from_cron(&format!("{padded} ")).unwrap_err();
    assert_eq!(
        error.to_string(),
        "cron expression: longer than 1024 bytes at byte 1024"
    );
    let multibyte = format!("{}é", "0".repeat(1023));
    assert_eq!(
        CalendarEvent::from_cron(&multibyte).unwrap_err().offset(),
        1023
    );

    let list = |n: usize| vec!["5"; n].join(",");
    assert!(CalendarEvent::from_cron(&format!("{} * * * *", list(241))).is_ok());
    let error = CalendarEvent::from_cron(&format!("{} * * * *", list(242))).unwrap_err();
    assert_eq!(
        error.to_string(),
        "cron expression: too many minute items at byte 482"
    );

    for len in [1_000, 10_000, 100_000, 1_000_000] {
        for unit in [",", "1,", "*/", " ", "-", "@", "mon-"] {
            let input = unit.repeat(len / unit.len());
            let start = Instant::now();
            let result = CalendarEvent::from_cron(&input);
            assert!(
                start.elapsed() < TIME_LIMIT,
                "{unit:?} x {len} took too long"
            );
            assert!(result.is_err() || input.trim().is_empty());
        }
    }
}
