#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    reason = "benchmark code: a panic is a failed run"
)]

use std::hint::black_box;
use std::time::{Duration, Instant};

use jiff::Timestamp;
use jiff::tz::{TimeZone, TimeZoneDatabase};
use oncalendar::CalendarEvent;

const CASES: &str = include_str!("../tests/fixtures/cases.tsv");
const SAMPLES: usize = 15;
const SAMPLE_TIME: Duration = Duration::from_millis(4);

struct Bench {
    timed: bool,
    filter: Option<String>,
}

impl Bench {
    fn from_args() -> Self {
        let args: Vec<String> = std::env::args().skip(1).collect();
        Self {
            timed: args.iter().any(|arg| arg == "--bench"),
            filter: args.into_iter().find(|arg| !arg.starts_with('-')),
        }
    }

    fn run<T>(&self, name: &str, items: u64, check: &str, mut work: impl FnMut() -> T) {
        if self.filter.as_deref().is_some_and(|f| !name.contains(f)) {
            return;
        }
        if !self.timed {
            black_box(work());
            println!("{name:<34} {check}");
            return;
        }
        let mut iters: u64 = 1;
        loop {
            if time(iters, &mut work) >= SAMPLE_TIME || iters >= 1 << 30 {
                break;
            }
            iters *= 2;
        }
        let mut per_call: Vec<f64> = (0..SAMPLES)
            .map(|_| time(iters, &mut work).as_nanos() as f64 / iters as f64)
            .collect();
        per_call.sort_by(f64::total_cmp);
        let median = per_call[SAMPLES / 2];
        let low = per_call[0];
        let high = per_call[SAMPLES - 1];
        let per_item = median / items as f64;
        println!(
            "{name:<34} {:>12} {:>12} {:>8} {:>6}  {check}",
            nanos(median),
            nanos(per_item),
            format!("{:+.0}%", (high - low) / median * 100.0),
            iters,
        );
    }
}

fn time<T>(iters: u64, work: &mut impl FnMut() -> T) -> Duration {
    let start = Instant::now();
    for _ in 0..iters {
        black_box(work());
    }
    start.elapsed()
}

fn nanos(value: f64) -> String {
    if value >= 10_000.0 {
        format!("{:.1} µs", value / 1_000.0)
    } else {
        format!("{value:.0} ns")
    }
}

fn instant(text: &str) -> Timestamp {
    text.parse().expect("a valid instant")
}

fn fixture_expressions() -> Vec<&'static str> {
    CASES
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.split('\t').nth(2).expect("three columns"))
        .collect()
}

fn joined(values: impl Iterator<Item = u32>) -> String {
    values.map(|v| v.to_string()).collect::<Vec<_>>().join(",")
}

fn parse_case(bench: &Bench, name: &str, text: &str, accepted: bool) {
    let check = match text.parse::<CalendarEvent>() {
        Ok(event) if accepted => format!("ok, {} input bytes -> {}", text.len(), event),
        Err(error) if !accepted => format!("rejected at byte {} of {}", error.offset(), text.len()),
        _ => panic!("{name}: unexpected parse outcome"),
    };
    let check = shorten(&check);
    bench.run(name, 1, &check, || black_box(text).parse::<CalendarEvent>());
}

fn shorten(text: &str) -> String {
    let short: String = text.chars().take(60).collect();
    if short.len() < text.len() {
        format!("{short}...")
    } else {
        short
    }
}

struct Search<'a> {
    database: &'a TimeZoneDatabase,
    after: Timestamp,
    local: TimeZone,
}

impl Search<'_> {
    fn case(&self, bench: &Bench, name: &str, expression: &str, take: usize) {
        let event: CalendarEvent = expression.parse().expect("a valid expression");
        let elapses: Vec<Timestamp> = event
            .upcoming_with(self.after, &self.local, self.database)
            .expect("a known zone")
            .take(take)
            .collect();
        assert_eq!(elapses.len(), take, "{name}: too few elapses");
        assert!(
            elapses.windows(2).all(|pair| pair[0] < pair[1]),
            "{name}: not increasing"
        );
        let first = elapses.first().expect("one elapse");
        let last = elapses.last().expect("one elapse");
        assert!(*first > self.after, "{name}: elapse not after the base");
        let check = if take == 1 {
            format!("{first}")
        } else {
            format!("{first} .. {last}")
        };
        bench.run(name, take as u64, &check, || {
            event
                .upcoming_with(black_box(self.after), &self.local, self.database)
                .expect("a known zone")
                .take(take)
                .last()
        });
    }
}

fn main() {
    let bench = Bench::from_args();
    if bench.timed {
        println!(
            "{:<34} {:>12} {:>12} {:>8} {:>6}  result",
            "case", "per call", "per item", "spread", "iters"
        );
    }

    let corpus = fixture_expressions();
    let longest = corpus
        .iter()
        .copied()
        .max_by_key(|text| text.len())
        .expect("fixtures");
    let long_list = format!(
        "Mon..Fri *-*-{} {}:{}:00",
        joined((1..=31).step_by(2)),
        joined((0..24).step_by(2)),
        joined((0..60).step_by(5)),
    );
    let over_cap = format!("*-*-* *:{}", joined((0..242).map(|v| v % 60)));

    parse_case(&bench, "parse simple: daily", "daily", true);
    parse_case(
        &bench,
        "parse common: Mon..Fri 09:00",
        "Mon..Fri *-*-* 09:00:00",
        true,
    );
    parse_case(
        &bench,
        "parse common: zone",
        "*-*-* 02:30 Europe/Berlin",
        true,
    );
    parse_case(&bench, "parse long list", &long_list, true);
    parse_case(&bench, "parse longest fixture", longest, true);
    parse_case(&bench, "reject early: month 13", "2026-13-01", false);
    parse_case(&bench, "reject late: 242-item list", &over_cap, false);

    let total: usize = corpus.iter().map(|text| text.len()).sum();
    let accepted = corpus
        .iter()
        .filter(|text| text.parse::<CalendarEvent>().is_ok())
        .count();
    bench.run(
        "parse fixture corpus",
        corpus.len() as u64,
        &format!("{accepted} of {} accepted, {total} bytes", corpus.len()),
        || {
            corpus
                .iter()
                .filter(|text| black_box(**text).parse::<CalendarEvent>().is_ok())
                .count()
        },
    );

    for (name, text) in [
        ("format simple: daily", "daily"),
        ("format long list", long_list.as_str()),
        ("format longest fixture", longest),
    ] {
        let event: CalendarEvent = text.parse().expect("a valid expression");
        let printed = event.to_string();
        bench.run(name, 1, &format!("{} bytes", printed.len()), || {
            black_box(&event).to_string()
        });
    }

    let database = TimeZoneDatabase::bundled();
    let zone = |name: &str| database.get(name).expect("a bundled zone");
    let base = instant("2026-01-01T00:00:00Z");
    let utc = Search {
        database: &database,
        after: base,
        local: TimeZone::UTC,
    };
    let berlin = Search {
        database: &database,
        after: base,
        local: zone("Europe/Berlin"),
    };
    let before_fold = Search {
        database: &database,
        after: instant("2026-11-01T05:59:59.999999Z"),
        local: zone("America/New_York"),
    };
    let before_gap = Search {
        database: &database,
        after: instant("2026-03-08T06:59:00Z"),
        local: zone("America/New_York"),
    };

    utc.case(&bench, "next daily", "daily", 1);
    utc.case(&bench, "next daily x100", "daily", 100);
    berlin.case(&bench, "next Mon..Fri 09:00 Berlin", "Mon..Fri 09:00", 1);
    berlin.case(
        &bench,
        "next Mon..Fri 09:00 Berlin x100",
        "Mon..Fri 09:00",
        100,
    );
    utc.case(&bench, "next Mon *-02-29", "Mon *-02-29", 1);
    utc.case(&bench, "next Mon *-02-29 x5", "Mon *-02-29", 5);
    utc.case(&bench, "next *:*:0/5 x1000", "*:*:0/5", 1000);
    utc.case(&bench, "next *:*:0/0.000001 x1000", "*:*:0/0.000001", 1000);
    utc.case(
        &bench,
        "next named zone in expression",
        "*-*-* 02:30 America/New_York",
        1,
    );
    before_fold.case(&bench, "next *:0/20 at NY fold", "*:0/20", 1);
    before_fold.case(&bench, "next *:0/20 at NY fold x100", "*:0/20", 100);
    before_gap.case(&bench, "next *-03-08 02:30 at NY gap", "*-03-08 02:30", 1);
}
