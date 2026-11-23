#![allow(
    missing_docs,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "test code: a panic is a test failure"
)]

use std::panic::catch_unwind;
use std::time::{Duration, Instant};

use oncalendar::{CalendarEvent, ParseError};

const TIME_LIMIT: Duration = Duration::from_secs(5);
const MAX_ITEMS: usize = 241;
const MAX_ITEM_LEN: usize = 31;
const MAX_NORMALIZED_LEN: usize = 6 * MAX_ITEMS * MAX_ITEM_LEN + 1024;
const MAX_MESSAGE_LEN: usize = 200;

const EXPECTED: &str = include_str!("fixtures/expected.tsv");

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }

    fn range(&mut self, lo: u32, hi: u32) -> u32 {
        lo + (self.next_u64() % u64::from(hi - lo + 1)) as u32
    }

    #[cfg_attr(
        not(feature = "jiff"),
        allow(dead_code, reason = "used by the computation tests")
    )]
    fn percent(&mut self, p: usize) -> bool {
        self.below(100) < p
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

enum Parsed {
    Accepted(CalendarEvent),
    Rejected(ParseError),
    Panicked,
}

fn parse(input: &str) -> Parsed {
    match catch_unwind(|| input.parse::<CalendarEvent>()) {
        Ok(Ok(event)) => Parsed::Accepted(event),
        Ok(Err(error)) => Parsed::Rejected(error),
        Err(_) => Parsed::Panicked,
    }
}

fn accepted_corpus() -> Vec<&'static str> {
    fixture_expressions("ok")
}

fn rejected_corpus() -> Vec<&'static str> {
    fixture_expressions("error")
}

fn fixture_expressions(status: &str) -> Vec<&'static str> {
    EXPECTED
        .lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            match fields[..] {
                [_, _, expr, s, _, _] if s == status => Some(expr),
                _ => None,
            }
        })
        .collect()
}

const GRAMMAR: &[char] = &[
    '0', '1', '2', '3', '5', '9', '*', ',', '.', '/', '~', ':', '-', ' ', 'M', 'o', 'n', 'T', 'u',
    'e', 'S', 'a', 't', 'U', 'C', 'A', 's', 'i', '_', '+',
];

const HOSTILE: &[char] = &[
    '\0',
    '\u{7}',
    '\u{1b}',
    '\t',
    '\n',
    '\r',
    '\u{a0}',
    '\u{3000}',
    '\u{200b}',
    '\u{feff}',
    '\u{301}',
    '\u{202e}',
    'é',
    '한',
    '😀',
    'Ｍ',
    '０',
    '：',
    '－',
    '／',
    '\u{10ffff}',
];

fn fuzz_inputs() -> Vec<String> {
    let mut rng = Rng::new(0x0ca1_e0da);
    let mut inputs = Vec::new();
    let mut seeds: Vec<&str> = accepted_corpus();
    seeds.extend(rejected_corpus());
    seeds.retain(|s| s.len() < 200);

    for _ in 0..5_000 {
        let len = rng.below(40);
        let text: String = (0..len)
            .map(|_| match rng.below(3) {
                0 => *rng.pick(HOSTILE),
                1 => *rng.pick(GRAMMAR),
                _ => char::from_u32(rng.range(0, 0x2_FFFF)).unwrap_or('?'),
            })
            .collect();
        inputs.push(text);
    }
    for seed in seeds.iter().take(40) {
        for (at, _) in seed.char_indices().chain([(seed.len(), ' ')]) {
            let mut text = seed.to_string();
            text.insert(at, *rng.pick(HOSTILE));
            inputs.push(text);
        }
    }
    for _ in 0..20_000 {
        let mut chars: Vec<char> = rng.pick(&seeds).chars().collect();
        for _ in 0..rng.range(1, 3) {
            let at = rng.below(chars.len() + 1);
            match rng.below(3) {
                0 => chars.insert(at, *rng.pick(GRAMMAR)),
                1 if at < chars.len() => {
                    chars.remove(at);
                }
                _ if at < chars.len() => chars[at] = *rng.pick(GRAMMAR),
                _ => chars.push(*rng.pick(GRAMMAR)),
            }
        }
        inputs.push(chars.into_iter().collect());
    }
    for _ in 0..20_000 {
        let len = rng.range(1, 30) as usize;
        inputs.push((0..len).map(|_| *rng.pick(GRAMMAR)).collect());
    }
    inputs
}

fn assert_all<'a, I>(property: &str, inputs: I, mut check: impl FnMut(&str) -> Option<String>)
where
    I: IntoIterator<Item = &'a str>,
{
    let mut failures = Vec::new();
    let mut total = 0;
    for input in inputs {
        total += 1;
        if let Some(problem) = check(input) {
            failures.push(format!("{}: {problem}", shorten(input)));
        }
    }
    assert!(
        failures.is_empty(),
        "{property}: {} of {total} inputs fail, for example:\n{}",
        failures.len(),
        failures
            .iter()
            .take(10)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

fn shorten(text: &str) -> String {
    const KEEP: usize = 60;
    if text.chars().count() <= KEEP {
        return format!("{text:?}");
    }
    let head: String = text.chars().take(KEEP).collect();
    format!("{head:?}... ({} bytes)", text.len())
}

fn accepted_events(property: &str) -> Vec<(String, CalendarEvent)> {
    let mut candidates: Vec<String> = accepted_corpus().into_iter().map(str::to_owned).collect();
    candidates.extend(fuzz_inputs());
    candidates.extend(
        item_limit_cases()
            .into_iter()
            .filter(|c| c.accept)
            .map(|c| c.input),
    );
    candidates.push(format!(
        "Mon{}*-*-*{}10:00",
        " ".repeat(100_000),
        " ".repeat(100_000)
    ));
    let accepted: Vec<(String, CalendarEvent)> = candidates
        .into_iter()
        .filter_map(|input| match parse(&input) {
            Parsed::Accepted(event) => Some((input, event)),
            _ => None,
        })
        .collect();
    assert!(
        !accepted.is_empty(),
        "{property}: no input was accepted, so the property was not exercised"
    );
    accepted
}

#[test]
fn parse_never_panics() {
    let inputs = fuzz_inputs();
    assert_all(
        "parse panicked",
        inputs.iter().map(String::as_str),
        |input| matches!(parse(input), Parsed::Panicked).then(|| "panicked".to_owned()),
    );
}

#[test]
fn error_offset_is_a_char_boundary_within_the_input() {
    let inputs = fuzz_inputs();
    assert_all(
        "error offset",
        inputs.iter().map(String::as_str),
        |input| match parse(input) {
            Parsed::Rejected(error) => {
                let offset = error.offset();
                (offset > input.len() || !input.is_char_boundary(offset))
                    .then(|| format!("offset {offset} in {} bytes", input.len()))
            }
            _ => None,
        },
    );
}

#[test]
fn error_message_is_short_ascii_and_does_not_echo_input() {
    const MARKER: &str = "Xq9INJECT";
    let mut inputs = fuzz_inputs();
    inputs.extend(
        [
            "Xq9INJECT",
            "daily Xq9INJECT",
            "Mon Xq9INJECT\nFAKE LOG LINE",
            "daily \u{1b}[31mXq9INJECT",
            "*-*-* 10:00 Evil/Xq9INJECT\r\n",
            "*-*-Xq9INJECT",
            "Xq9INJECT,Mon *-*-*",
        ]
        .map(str::to_owned),
    );
    assert_all(
        "error message",
        inputs.iter().map(String::as_str),
        |input| {
            let Parsed::Rejected(error) = parse(input) else {
                return None;
            };
            let message = error.to_string();
            if message.len() > MAX_MESSAGE_LEN {
                return Some(format!("message is {} bytes", message.len()));
            }
            if !message.bytes().all(|b| (0x20..=0x7e).contains(&b)) {
                return Some(format!("message is not printable ASCII: {message:?}"));
            }
            (input.contains(MARKER) && message.contains(MARKER))
                .then(|| format!("message repeats the input: {message:?}"))
        },
    );
}

#[test]
fn numeric_overflow_does_not_panic() {
    let inputs = [
        "4294967296".to_owned(),
        "18446744073709551616-01-01".to_owned(),
        "99999999999999999999-01-01".to_owned(),
        "*-*-* 99999999999999999999:00".to_owned(),
        "*:00/4294967296".to_owned(),
        "*:00/18446744073709551616".to_owned(),
        format!("1{}", "0".repeat(100)),
        format!("*-*-* {}:00", "9".repeat(100)),
        format!("*-*-* {}1:00", "0".repeat(1000)),
        format!("*-*-* 00:00:0.{}", "9".repeat(1000)),
        format!("*-*-* 00:00:0.{}1", "0".repeat(999)),
        format!("*:*:0/0.{}1", "0".repeat(1000)),
        format!("*:0/{}", "9".repeat(100)),
        format!("*-*~{}", "9".repeat(50)),
        format!("2026/{}-01-01", "9".repeat(30)),
        format!("*:*:{}.5", "9".repeat(30)),
        format!(
            "*-*-* {}:{}:{}",
            "0".repeat(500),
            "0".repeat(500),
            "0".repeat(500)
        ),
    ];
    assert_all(
        "numeric input",
        inputs.iter().map(String::as_str),
        |input| matches!(parse(input), Parsed::Panicked).then(|| "panicked".to_owned()),
    );
}

#[test]
fn parse_time_is_linear_in_input_length() {
    type Family = (&'static str, fn(usize) -> String);
    let families: [Family; 14] = [
        ("commas", |n| ",".repeat(n)),
        ("dots", |n| ".".repeat(n)),
        ("slashes", |n| "/".repeat(n)),
        ("tildes", |n| "~".repeat(n)),
        ("dashes", |n| "-".repeat(n)),
        ("digits", |n| "9".repeat(n)),
        ("spaces", |n| " ".repeat(n)),
        ("inner spaces", |n| format!("Mon{}10:00", " ".repeat(n))),
        ("weekday list", |n| format!("Mon{}", ",Tue".repeat(n / 4))),
        ("long zone", |n| format!("daily {}", "A".repeat(n))),
        ("zone components", |n| {
            format!("daily {}A", "a/".repeat(n / 2))
        }),
        ("zero list", |n| format!("*:{}0", "0,".repeat(n / 2))),
        ("range list", |n| {
            format!("*:{}0..59", "0..59,".repeat(n / 6))
        }),
        ("colons", |n| format!("*-*-*{}", ":*".repeat(n / 2))),
    ];
    let mut failures = Vec::new();
    for (name, make) in families {
        for len in [1_000, 10_000, 100_000, 1_000_000] {
            let input = make(len);
            let start = Instant::now();
            let outcome = parse(&input);
            if let Parsed::Accepted(event) = &outcome {
                let _ = event.to_string();
            }
            let elapsed = start.elapsed();
            if matches!(outcome, Parsed::Panicked) {
                failures.push(format!("{name} x{len}: panicked"));
            } else if elapsed > TIME_LIMIT {
                failures.push(format!("{name} x{len}: {elapsed:?}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "slow or panicking parses:\n{}",
        failures.join("\n")
    );
}

struct LimitCase {
    name: String,
    input: String,
    accept: bool,
}

fn item_limit_cases() -> Vec<LimitCase> {
    type Item = fn(usize) -> String;
    let fields: [(&str, &str, &str, Item, &str, &str); 7] = [
        (
            "year",
            "",
            "-01-01",
            |i| (1970 + i % 230).to_string(),
            "2026..2199",
            "2027",
        ),
        (
            "month",
            "*-",
            "-01",
            |i| (i % 12 + 1).to_string(),
            "1..12",
            "5",
        ),
        (
            "day",
            "*-*-",
            "",
            |i| (i % 31 + 1).to_string(),
            "1..31",
            "5",
        ),
        (
            "day from end",
            "*-*~",
            "",
            |i| (i % 28 + 1).to_string(),
            "1..28",
            "5",
        ),
        ("hour", "", ":00", |i| (i % 24).to_string(), "0..23", "5"),
        ("minute", "*:", "", |i| (i % 60).to_string(), "0..59", "5"),
        ("second", "*:*:", "", |i| (i % 60).to_string(), "0..59", "5"),
    ];
    let mut cases = Vec::new();
    for (field, prefix, suffix, value, range, repeat) in fields {
        for shape in ["values", "ranges", "repeats"] {
            if field == "day from end" && shape == "values" {
                continue;
            }
            for count in [MAX_ITEMS, MAX_ITEMS + 1] {
                let list: Vec<String> = (0..count)
                    .map(|i| match shape {
                        "values" => value(i),
                        "ranges" => range.to_owned(),
                        _ => repeat.to_owned(),
                    })
                    .collect();
                cases.push(LimitCase {
                    name: format!("{field}, {count} {shape}"),
                    input: format!("{prefix}{}{suffix}", list.join(",")),
                    accept: count <= MAX_ITEMS,
                });
            }
        }
    }
    for count in [257, 10_000] {
        cases.push(LimitCase {
            name: format!("{count} weekdays"),
            input: vec!["Mon"; count].join(","),
            accept: true,
        });
    }
    cases.push(LimitCase {
        name: "every field at the limit".to_owned(),
        input: every_field_at_the_limit(),
        accept: true,
    });
    cases
}

fn every_field_at_the_limit() -> String {
    let list = |f: &dyn Fn(usize) -> String| (0..MAX_ITEMS).map(f).collect::<Vec<_>>().join(",");
    format!(
        "Mon,Wed,Fri {}-{}-{} {}:{}:{} America/New_York",
        list(&|i| (2026 + i % 174).to_string()),
        list(&|i| (i % 12 + 1).to_string()),
        list(&|i| (i % 31 + 1).to_string()),
        list(&|i| (i % 24).to_string()),
        list(&|i| (i % 60).to_string()),
        list(&|i| format!("{}.{:06}", i % 60, (i * 7919) % 1_000_000)),
    )
}

#[test]
fn list_item_limit() {
    let mut failures = Vec::new();
    for case in item_limit_cases() {
        match (parse(&case.input), case.accept) {
            (Parsed::Accepted(_), true) | (Parsed::Rejected(_), false) => {}
            (Parsed::Panicked, _) => failures.push(format!("{}: panicked", case.name)),
            (_, true) => failures.push(format!("{}: rejected, should be accepted", case.name)),
            (_, false) => failures.push(format!("{}: accepted, should be rejected", case.name)),
        }
    }
    assert!(failures.is_empty(), "list limit:\n{}", failures.join("\n"));
}

#[test]
fn normalized_length_is_bounded() {
    let accepted = accepted_events("normalized length");
    assert_all(
        "normalized length",
        accepted.iter().map(|(s, _)| s.as_str()),
        |input| {
            let Parsed::Accepted(event) = parse(input) else {
                return Some("no longer accepted".to_owned());
            };
            let len = event.to_string().len();
            (len > MAX_NORMALIZED_LEN).then(|| format!("normalized form is {len} bytes"))
        },
    );
}

#[test]
fn normalization_is_idempotent() {
    let accepted = accepted_events("idempotence");
    let mut failures = Vec::new();
    for (input, event) in &accepted {
        let normalized = event.to_string();
        match parse(&normalized) {
            Parsed::Accepted(again) if again == *event && again.to_string() == normalized => {}
            Parsed::Accepted(again) => failures.push(format!(
                "{} -> {} -> {}",
                shorten(input),
                shorten(&normalized),
                shorten(&again.to_string())
            )),
            _ => failures.push(format!(
                "{} -> {} is rejected",
                shorten(input),
                shorten(&normalized)
            )),
        }
    }
    assert!(
        failures.is_empty(),
        "idempotence: {} of {} fail:\n{}",
        failures.len(),
        accepted.len(),
        failures
            .iter()
            .take(10)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn normalized_form_uses_the_grammar_alphabet() {
    let accepted = accepted_events("output alphabet");
    let allowed = |c: char| c.is_ascii_alphanumeric() || " *,./~:-_+".contains(c);
    let mut failures = Vec::new();
    for (input, event) in &accepted {
        let normalized = event.to_string();
        if !normalized.chars().all(allowed) {
            failures.push(format!("{} -> {normalized:?}", shorten(input)));
        }
    }
    assert!(
        failures.is_empty(),
        "output alphabet:\n{}",
        failures.join("\n")
    );
}

#[test]
fn a_tab_is_not_a_separator() {
    let with_spaces = ["Mon 10:00", "Mon *-*-* 10:00", "daily UTC", "10:00 UTC"];
    let mut failures = Vec::new();
    for spaced in with_spaces {
        if spaced.parse::<CalendarEvent>().is_err() {
            failures.push(format!("{spaced:?} is rejected"));
        }
        let tabbed = spaced.replace(' ', "\t");
        if tabbed.parse::<CalendarEvent>().is_ok() {
            failures.push(format!("{tabbed:?} is accepted"));
        }
    }
    for edge in ["\tdaily", "daily\t", "Mon,\tTue 10:00"] {
        if edge.parse::<CalendarEvent>().is_ok() {
            failures.push(format!("{edge:?} is accepted"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn path_like_time_zone_names_are_parse_errors() {
    let long = "A".repeat(10_000);
    let hostile = [
        "../../etc/passwd",
        "/etc/passwd",
        "Asia/../../etc/passwd",
        "Asia//Seoul",
        "./UTC",
        "Asia/Seoul/",
        ":/etc/localtime",
        "Asia\\Seoul",
        "..\\..\\etc\\passwd",
        "Asia/Seoul\0",
        "Asia/\0Seoul",
        long.as_str(),
        "Asia/Ṣeoul",
        "UТC",
        "Asia/Seoul\u{200b}",
        "Asia/.hidden",
    ];
    let mut failures = Vec::new();
    for control in [
        "daily UTC",
        "daily Asia/Seoul",
        "*-*-* 00:00:00 America/New_York",
    ] {
        if !matches!(parse(control), Parsed::Accepted(_)) {
            failures.push(format!("control {control:?} is not accepted"));
        }
    }
    for name in hostile {
        for input in [format!("daily {name}"), format!("*-*-* 00:00:00 {name}")] {
            if !matches!(parse(&input), Parsed::Rejected(_)) {
                failures.push(format!("{} is not a parse error", shorten(&input)));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "time zone names:\n{}",
        failures.join("\n")
    );
}

#[test]
fn utc_is_case_insensitive() {
    let mut failures = Vec::new();
    for input in ["daily UTC", "daily utc", "daily Utc", "Mon uTc"] {
        match parse(input) {
            Parsed::Accepted(event) if event.to_string().ends_with(" UTC") => {}
            Parsed::Accepted(event) => failures.push(format!("{input:?} -> {event}")),
            _ => failures.push(format!("{input:?} is not accepted")),
        }
    }
    assert!(
        failures.is_empty(),
        "UTC spelling:\n{}",
        failures.join("\n")
    );
}

#[cfg(feature = "jiff")]
mod computation {
    use super::*;

    use std::panic::AssertUnwindSafe;

    use jiff::civil::{Date, DateTime};
    use jiff::tz::{AmbiguousOffset, Offset, TimeZone, TimeZoneDatabase};
    use jiff::{Timestamp, ToSpan};

    fn tz(name: &str) -> TimeZone {
        TimeZoneDatabase::bundled().get(name).unwrap()
    }

    fn at(rfc3339: &str) -> Timestamp {
        rfc3339.parse().unwrap()
    }

    fn micro(us: i64) -> jiff::SignedDuration {
        jiff::SignedDuration::from_micros(us)
    }

    fn parsed_corpus(property: &str) -> Vec<(&'static str, CalendarEvent)> {
        let mut inputs = accepted_corpus();
        let limit = Box::leak(every_field_at_the_limit().into_boxed_str());
        inputs.push(limit);
        let parsed: Vec<_> = inputs
            .into_iter()
            .filter_map(|s| s.parse::<CalendarEvent>().ok().map(|e| (s, e)))
            .collect();
        assert!(
            !parsed.is_empty(),
            "{property}: no expression was accepted, so the property was not exercised"
        );
        parsed
    }

    fn zones_for_edges() -> Vec<(String, TimeZone)> {
        let mut zones: Vec<(String, TimeZone)> = [
            "UTC",
            "America/New_York",
            "Australia/Lord_Howe",
            "Pacific/Apia",
            "Europe/Dublin",
            "Africa/Casablanca",
        ]
        .iter()
        .map(|n| ((*n).to_owned(), tz(n)))
        .collect();
        zones.push(("fixed max".to_owned(), TimeZone::fixed(Offset::MAX)));
        zones.push(("fixed min".to_owned(), TimeZone::fixed(Offset::MIN)));
        zones
    }

    #[test]
    fn other_time_zone_names_match_exactly() {
        let seoul = tz("Asia/Seoul");
        let base = at("2026-01-01T00:00:00Z");
        let mut failures = Vec::new();
        for control in [
            "daily Asia/Seoul",
            "daily America/New_York",
            "daily Etc/GMT+5",
        ] {
            let works = control
                .parse::<CalendarEvent>()
                .is_ok_and(|event| event.upcoming(base, &seoul).is_ok());
            if !works {
                failures.push(format!("control {control:?} does not work"));
            }
        }
        for input in [
            "daily asia/seoul",
            "daily ASIA/SEOUL",
            "daily america/new_york",
            "daily Nowhere/Bogus",
            "daily Z",
            "daily +09:00",
            "daily KST",
        ] {
            let accepted = input
                .parse::<CalendarEvent>()
                .is_ok_and(|event| event.upcoming(base, &seoul).is_ok());
            if accepted {
                failures.push(format!("{input:?} is accepted"));
            }
        }
        if "daily KST".parse::<CalendarEvent>().is_err() {
            failures.push("\"daily KST\" is a parse error, should parse".to_owned());
        }
        assert!(
            failures.is_empty(),
            "time zone names:\n{}",
            failures.join("\n")
        );
    }

    #[test]
    fn computation_never_panics() {
        let corpus = parsed_corpus("computation panics");
        let bases = [
            Timestamp::MIN,
            Timestamp::MAX,
            Timestamp::UNIX_EPOCH,
            Timestamp::UNIX_EPOCH - micro(1),
            Timestamp::UNIX_EPOCH + micro(1),
            at("2199-12-31T23:59:59.999999Z"),
            at("2199-12-31T23:59:59.999999Z") - micro(1),
            at("2200-01-01T00:00:00Z"),
            at("2200-01-01T00:00:00Z") + micro(1),
            at("2026-01-01T00:00:00Z"),
        ];
        let zones = zones_for_edges();
        let mut elapses = 0_usize;
        let mut failures = Vec::new();
        for (expr, event) in &corpus {
            for base in bases {
                for (zone_name, zone) in &zones {
                    let run = catch_unwind(AssertUnwindSafe(|| {
                        event
                            .upcoming(base, zone)
                            .map_or(0, |upcoming| upcoming.take(3).count())
                    }));
                    match run {
                        Ok(count) => elapses += count,
                        Err(_) => {
                            failures.push(format!("{} after {base} in {zone_name}", shorten(expr)));
                        }
                    }
                }
            }
        }
        assert!(
            elapses > 0,
            "not exercised: upcoming never produced an elapse"
        );
        assert!(
            failures.is_empty(),
            "upcoming panicked {} times, for example:\n{}",
            failures.len(),
            failures
                .iter()
                .take(10)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    #[test]
    fn elapses_increase_strictly_after_the_base() {
        let corpus = parsed_corpus("increasing elapses");
        let bases = [
            "2026-01-01T00:00:00Z",
            "2026-03-08T06:30:00Z",
            "2026-11-01T05:15:00Z",
            "2026-11-01T06:15:00Z",
            "2026-04-04T15:10:00Z",
            "2199-12-31T23:00:00Z",
        ];
        let zones = [
            "UTC",
            "America/New_York",
            "Australia/Lord_Howe",
            "Europe/Dublin",
        ];
        let mut checked = 0_usize;
        let mut failures = Vec::new();
        for (expr, event) in &corpus {
            for base in bases.map(at) {
                for name in zones {
                    let zone = tz(name);
                    let Ok(upcoming) = event.upcoming(base, &zone) else {
                        continue;
                    };
                    let mut previous = base;
                    for t in upcoming.take(50) {
                        checked += 1;
                        if t <= previous || t.subsec_nanosecond() % 1_000 != 0 {
                            failures.push(format!(
                                "{} after {base} in {name}: {t} follows {previous}",
                                shorten(expr)
                            ));
                            break;
                        }
                        previous = t;
                    }
                }
            }
        }
        assert!(
            checked > 0,
            "not exercised: upcoming never produced an elapse"
        );
        assert!(
            failures.is_empty(),
            "elapse order:\n{}",
            failures.join("\n")
        );
    }

    #[test]
    fn elapses_match_the_expression() {
        let mut rng = Rng::new(15);
        let mut checked = 0;
        let mut failures = Vec::new();
        for _ in 0..300 {
            let spec = Spec::generate(&mut rng);
            let local_name: &&str = rng.pick(LOCAL_ZONES);
            let local = tz(local_name);
            let base = random_base(&mut rng);
            let expr = spec.render();
            let Ok(event) = expr.parse::<CalendarEvent>() else {
                failures.push(format!("{expr:?} is valid but rejected"));
                continue;
            };
            let zone = spec.effective_zone(&local);
            let Ok(upcoming) = event.upcoming(base, &local) else {
                failures.push(format!("{expr:?}: upcoming failed"));
                continue;
            };
            for t in upcoming.take(30) {
                checked += 1;
                let civil = zone.to_datetime(t);
                if !spec.matches(civil) {
                    failures.push(format!(
                        "{expr:?} after {base}: {t} is {civil} on the wall clock"
                    ));
                    break;
                }
            }
        }
        assert!(
            failures.is_empty(),
            "elapses that do not match: {}\n{}",
            failures.len(),
            failures
                .iter()
                .take(10)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        );
        assert!(
            checked > 0,
            "no elapse was produced, so the property was not exercised"
        );
    }

    #[test]
    fn elapses_equal_the_reference() {
        let mut rng = Rng::new(16);
        let mut compared = 0;
        let mut failures = Vec::new();
        let mut cases: Vec<(Spec, &str, Timestamp)> = Vec::new();
        for _ in 0..200 {
            cases.push((
                Spec::generate(&mut rng),
                *rng.pick(LOCAL_ZONES),
                random_base(&mut rng),
            ));
        }
        for (local, base) in dst_bases() {
            for _ in 0..6 {
                cases.push((Spec::generate_dense(&mut rng), local, base));
            }
        }
        for (spec, local_name, base) in cases {
            let local = tz(local_name);
            let expr = spec.render();
            let Ok(event) = expr.parse::<CalendarEvent>() else {
                failures.push(format!("{expr:?} is valid but rejected"));
                continue;
            };
            let zone = spec.effective_zone(&local);
            let last_day = zone.to_datetime(base).date() + 400.days();
            let expected = reference_upcoming(&spec, base, &zone, last_day, 20);
            let Ok(upcoming) = event.upcoming(base, &local) else {
                failures.push(format!("{expr:?}: upcoming failed"));
                continue;
            };
            let got: Vec<Timestamp> = upcoming.take(expected.len() + 1).collect();
            compared += 1;
            let prefix_ok = got.len() >= expected.len() && got[..expected.len()] == expected[..];
            let tail_ok = expected.len() == 20
                || got
                    .get(expected.len())
                    .is_none_or(|t| zone.to_datetime(*t).date() > last_day);
            if !(prefix_ok && tail_ok) {
                failures.push(format!(
                    "{expr:?} local {local_name} after {base}:\n      reference {}\n      crate     {}",
                    list(&expected),
                    list(&got)
                ));
            }
        }
        assert!(
            failures.is_empty(),
            "{} of {} cases differ from the reference:\n{}",
            failures.len(),
            compared.max(failures.len()),
            failures
                .iter()
                .take(8)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    #[test]
    fn slow_inputs_finish_in_time() {
        let limit_expr = every_field_at_the_limit();
        let fold = "2026-11-01T06:00:00Z";
        let cases: [(&str, &str, &str, usize, Option<usize>); 8] = [
            ("*-02-30", "UTC", "2026-01-01T00:00:00Z", 1, Some(0)),
            ("*-2,4,6,9,11-31", "UTC", "2026-01-01T00:00:00Z", 1, Some(0)),
            (
                "2027-03-14 02:*:0/0.000001 America/New_York",
                "UTC",
                "2026-01-01T00:00:00Z",
                1,
                Some(0),
            ),
            (
                "Sun *-03-08..14 02:*:0/0.000001 America/New_York",
                "UTC",
                "2026-01-01T00:00:00Z",
                1,
                Some(0),
            ),
            (
                "Sun *-03-08..14 02:30 America/New_York",
                "UTC",
                "2026-01-01T00:00:00Z",
                1,
                Some(0),
            ),
            (
                "*:*:0/0.000001",
                "America/New_York",
                fold,
                1_000,
                Some(1_000),
            ),
            ("*:*:*", "America/New_York", fold, 10_000, Some(10_000)),
            (
                limit_expr.as_str(),
                "UTC",
                "2026-01-01T00:00:00Z",
                1_000,
                None,
            ),
        ];
        let mut failures = Vec::new();
        for (expr, local, base, take, count) in cases {
            let Ok(event) = expr.parse::<CalendarEvent>() else {
                failures.push(format!("{} is rejected", shorten(expr)));
                continue;
            };
            let local = tz(local);
            let start = Instant::now();
            let Ok(upcoming) = event.upcoming(at(base), &local) else {
                failures.push(format!("{}: upcoming failed", shorten(expr)));
                continue;
            };
            let got = upcoming.take(take).count();
            let elapsed = start.elapsed();
            if elapsed > TIME_LIMIT {
                failures.push(format!("{}: {elapsed:?}", shorten(expr)));
            }
            if count.is_some_and(|c| c != got) {
                failures.push(format!(
                    "{}: {got} elapses, expected {count:?}",
                    shorten(expr)
                ));
            }
        }
        assert!(failures.is_empty(), "slow inputs:\n{}", failures.join("\n"));
    }

    #[test]
    fn daylight_saving_policy() {
        let exprs = [
            "*:30", "*:*:0/30", "01:*:00", "*:0/20", "*:15", "*:45", "02:30", "01:30",
        ];
        let mut checked = 0;
        let mut failures = Vec::new();
        for (local_name, base) in dst_bases() {
            let zone = tz(local_name);
            for expr in exprs {
                let Ok(event) = expr.parse::<CalendarEvent>() else {
                    failures.push(format!("{expr:?} is rejected"));
                    continue;
                };
                let Ok(upcoming) = event.upcoming(base, &zone) else {
                    failures.push(format!("{expr:?}: upcoming failed"));
                    continue;
                };
                let mut seen: Vec<DateTime> = Vec::new();
                for t in upcoming.take(30) {
                    checked += 1;
                    let civil = zone.to_datetime(t);
                    let ambiguous = zone.to_ambiguous_timestamp(civil);
                    let problem = match ambiguous.offset() {
                        AmbiguousOffset::Gap { .. } => Some("in a gap"),
                        AmbiguousOffset::Fold { .. } => {
                            let earlier = ambiguous.earlier().unwrap();
                            (t != earlier && earlier > base).then_some("later instant of a fold")
                        }
                        AmbiguousOffset::Unambiguous { .. } => None,
                    };
                    let problem =
                        problem.or(seen.contains(&civil).then_some("repeated wall-clock time"));
                    let fixed = match expr {
                        "02:30" => Some((2, 30)),
                        "01:30" => Some((1, 30)),
                        _ => None,
                    };
                    let problem = problem.or(fixed
                        .is_some_and(|wall| wall != (civil.hour(), civil.minute()))
                        .then_some("shifted away from the wall-clock time it names"));
                    if let Some(problem) = problem {
                        failures.push(format!(
                            "{expr:?} in {local_name} after {base}: {t} ({civil}) {problem}"
                        ));
                        break;
                    }
                    seen.push(civil);
                }
            }
        }
        assert!(
            failures.is_empty(),
            "daylight saving:\n{}",
            failures.join("\n")
        );
        assert!(
            checked > 0,
            "no elapse was produced, so the property was not exercised"
        );
    }

    #[test]
    fn dense_iterator_steps_by_one_microsecond() {
        let event: CalendarEvent = "*:*:0/0.000001".parse().expect("systemd accepts this");
        let base = at("2026-01-01T00:00:00Z");
        let start = Instant::now();
        let got: Vec<Timestamp> = event
            .upcoming(base, &TimeZone::UTC)
            .unwrap()
            .take(10_000)
            .collect();
        assert!(start.elapsed() <= TIME_LIMIT, "took {:?}", start.elapsed());
        assert_eq!(got.len(), 10_000);
        let mut previous = base;
        for t in got {
            assert_eq!(
                t.duration_since(previous),
                micro(1),
                "{t} follows {previous}"
            );
            previous = t;
        }
    }

    #[test]
    fn exhausted_iterator_stays_exhausted() {
        let cases: [(&str, &str, &[&str]); 5] = [
            ("*-02-30", "2026-01-01T00:00:00Z", &[]),
            ("2003-03-05", "2026-01-01T00:00:00Z", &[]),
            ("Mon 2026-01-01", "2025-01-01T00:00:00Z", &[]),
            (
                "2199-12-31 23:59:58,59",
                "2199-12-31T23:00:00Z",
                &["2199-12-31T23:59:58Z", "2199-12-31T23:59:59Z"],
            ),
            (
                "*-*-* *:*:*",
                "2199-12-31T23:59:58Z",
                &["2199-12-31T23:59:59Z"],
            ),
        ];
        for (expr, base, expected) in cases {
            let event: CalendarEvent = expr.parse().unwrap_or_else(|e| panic!("{expr:?}: {e}"));
            let utc = TimeZone::UTC;
            let mut upcoming = event.upcoming(at(base), &utc).unwrap();
            for want in expected {
                assert_eq!(upcoming.next(), Some(at(want)), "{expr:?} after {base}");
            }
            for _ in 0..3 {
                assert_eq!(
                    upcoming.next(),
                    None,
                    "{expr:?} after {base} did not stay exhausted"
                );
            }
        }
    }

    #[test]
    #[ignore = "prints cases for oracle/run.sh"]
    fn print_generated_cases_for_oracle() {
        let mut rng = Rng::new(16);
        let mut cases: Vec<(Spec, &str, Timestamp)> = Vec::new();
        for _ in 0..200 {
            cases.push((
                Spec::generate(&mut rng),
                *rng.pick(LOCAL_ZONES),
                random_base(&mut rng),
            ));
        }
        for (local, base) in dst_bases() {
            for _ in 0..6 {
                cases.push((Spec::generate_dense(&mut rng), local, base));
            }
        }
        for (spec, local_name, base) in cases {
            let zone = spec.effective_zone(&tz(local_name));
            let last_day = zone.to_datetime(base).date() + 400.days();
            let expected = reference_upcoming(&spec, base, &zone, last_day, 5);
            let base_civil = TimeZone::UTC
                .to_datetime(base)
                .to_string()
                .replacen('T', " ", 1);
            let elapses: Vec<String> = expected.iter().map(|t| t.to_string()).collect();
            let elapses = if elapses.is_empty() {
                "none-in-window".to_owned()
            } else {
                elapses.join(",")
            };
            println!(
                "{local_name}\t{base_civil} UTC\t{}\t{elapses}",
                spec.render()
            );
        }
    }

    fn list(ts: &[Timestamp]) -> String {
        if ts.is_empty() {
            return "(none)".to_owned();
        }
        ts.iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",")
    }

    const LOCAL_ZONES: &[&str] = &[
        "UTC",
        "America/New_York",
        "Australia/Lord_Howe",
        "Europe/London",
        "Asia/Kolkata",
        "Pacific/Chatham",
    ];

    fn dst_bases() -> Vec<(&'static str, Timestamp)> {
        let transitions = [
            ("America/New_York", "2026-03-08T07:00:00Z"),
            ("America/New_York", "2026-11-01T06:00:00Z"),
            ("Australia/Lord_Howe", "2026-04-04T15:00:00Z"),
            ("Australia/Lord_Howe", "2026-10-03T15:30:00Z"),
            ("Europe/London", "2026-03-29T01:00:00Z"),
            ("Europe/London", "2026-10-25T01:00:00Z"),
        ];
        let offsets_min = [-90, -45, -15, 0, 15, 45, 75];
        let mut bases = Vec::new();
        for (zone, at_transition) in transitions {
            for minutes in offsets_min {
                bases.push((
                    zone,
                    at(at_transition) + jiff::SignedDuration::from_mins(minutes),
                ));
            }
            bases.push((zone, at(at_transition) - micro(1)));
        }
        bases
    }

    fn random_base(rng: &mut Rng) -> Timestamp {
        let start = at("2026-01-01T00:00:00Z").as_second();
        let end = at("2028-12-31T00:00:00Z").as_second();
        let second = start + (rng.next_u64() % (end - start) as u64) as i64;
        let micros = if rng.percent(50) {
            0
        } else {
            rng.below(1_000_000) as i64
        };
        Timestamp::from_microsecond(second * 1_000_000 + micros).unwrap()
    }

    #[derive(Clone, Copy, Debug)]
    enum Item {
        Value(u32),
        Range(u32, u32),
        Every(u32, u32),
        Stepped(u32, u32, u32),
    }

    impl Item {
        fn matches(self, v: u32) -> bool {
            match self {
                Item::Value(a) => v == a,
                Item::Range(a, b) => (a..=b).contains(&v),
                Item::Every(a, r) => v >= a && (v - a) % r == 0,
                Item::Stepped(a, b, r) => (a..=b).contains(&v) && (v - a) % r == 0,
            }
        }

        fn render(self) -> String {
            match self {
                Item::Value(a) => a.to_string(),
                Item::Range(a, b) => format!("{a}..{b}"),
                Item::Every(a, r) => format!("{a}/{r}"),
                Item::Stepped(a, b, r) => format!("{a}..{b}/{r}"),
            }
        }
    }

    #[derive(Clone, Debug)]
    struct Field(Option<Vec<Item>>);

    impl Field {
        fn matches(&self, v: u32) -> bool {
            self.0
                .as_ref()
                .is_none_or(|items| items.iter().any(|i| i.matches(v)))
        }

        fn render(&self) -> String {
            match &self.0 {
                None => "*".to_owned(),
                Some(items) => items
                    .iter()
                    .map(|i| i.render())
                    .collect::<Vec<_>>()
                    .join(","),
            }
        }

        fn first_only(self) -> Field {
            Field(self.0.map(|items| items.into_iter().take(1).collect()))
        }

        fn generate(rng: &mut Rng, any: usize, lo: u32, hi: u32, max: u32, simple: bool) -> Field {
            if rng.percent(any) {
                return Field(None);
            }
            let count = rng.range(1, 3);
            let items = (0..count)
                .map(|_| {
                    let a = rng.range(lo, hi - 1);
                    let b = rng.range(a + 1, hi);
                    match rng.below(if simple { 2 } else { 4 }) {
                        0 => Item::Value(rng.range(lo, hi)),
                        1 => Item::Range(a, b),
                        2 => Item::Every(a, rng.range(1, (max - a).min(hi - lo).max(1))),
                        _ => Item::Stepped(a, b, rng.range(1, b - a)),
                    }
                })
                .collect();
            Field(Some(items))
        }
    }

    #[derive(Clone, Debug)]
    struct Spec {
        weekdays: Option<Vec<u8>>,
        year: Field,
        month: Field,
        day: Field,
        from_end: bool,
        hour: Field,
        minute: Field,
        second: Field,
        zone: Option<&'static str>,
    }

    const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

    impl Spec {
        fn generate(rng: &mut Rng) -> Spec {
            let from_end = rng.percent(15);
            Spec {
                weekdays: rng
                    .percent(30)
                    .then(|| (0..rng.range(1, 4)).map(|_| rng.below(7) as u8).collect()),
                year: Field::generate(rng, 80, 2026, 2029, 2199, false),
                month: Field::generate(rng, 50, 1, 12, 12, false),
                day: if from_end {
                    Field::generate(rng, 0, 1, 28, 28, true).first_only()
                } else {
                    Field::generate(rng, 50, 1, 31, 31, false)
                },
                from_end,
                hour: Field::generate(rng, 40, 0, 23, 23, false),
                minute: Field::generate(rng, 30, 0, 59, 59, false),
                second: Field::generate(rng, 30, 0, 59, 59, false),
                zone: *rng.pick(&[
                    None,
                    None,
                    None,
                    None,
                    Some("UTC"),
                    Some("America/New_York"),
                ]),
            }
        }

        fn generate_dense(rng: &mut Rng) -> Spec {
            Spec {
                weekdays: None,
                year: Field(None),
                month: Field(None),
                day: Field(None),
                from_end: false,
                hour: Field(None),
                minute: Field::generate(rng, 30, 0, 59, 59, false),
                second: Field::generate(rng, 20, 0, 59, 59, false),
                zone: None,
            }
        }

        fn render(&self) -> String {
            let mut out = String::new();
            if let Some(days) = &self.weekdays {
                out += &days
                    .iter()
                    .map(|&d| WEEKDAYS[usize::from(d)])
                    .collect::<Vec<_>>()
                    .join(",");
                out.push(' ');
            }
            let separator = if self.from_end { "~" } else { "-" };
            out += &format!(
                "{}-{}{separator}{} {}:{}:{}",
                self.year.render(),
                self.month.render(),
                self.day.render(),
                self.hour.render(),
                self.minute.render(),
                self.second.render()
            );
            if let Some(zone) = self.zone {
                out.push(' ');
                out += zone;
            }
            out
        }

        fn effective_zone(&self, local: &TimeZone) -> TimeZone {
            self.zone.map_or_else(|| local.clone(), tz)
        }

        fn date_matches(&self, date: Date) -> bool {
            let day = date.day() as u32;
            let day_value = if self.from_end {
                date.days_in_month() as u32 - day + 1
            } else {
                day
            };
            self.weekdays
                .as_ref()
                .is_none_or(|days| days.contains(&(date.weekday().to_monday_zero_offset() as u8)))
                && self.year.matches(date.year() as u32)
                && self.month.matches(date.month() as u32)
                && self.day.matches(day_value)
        }

        fn matches(&self, civil: DateTime) -> bool {
            civil.subsec_nanosecond() == 0
                && self.date_matches(civil.date())
                && self.hour.matches(civil.hour() as u32)
                && self.minute.matches(civil.minute() as u32)
                && self.second.matches(civil.second() as u32)
        }
    }

    fn reference_next(
        spec: &Spec,
        after: Timestamp,
        zone: &TimeZone,
        last_day: Date,
    ) -> Option<Timestamp> {
        let start = zone.to_datetime(after + micro(1));
        let start_hms = (start.hour(), start.minute(), start.second());
        let start_is_whole = start.subsec_nanosecond() == 0;
        let mut date = start.date();
        while date <= last_day {
            if spec.date_matches(date) {
                let first_day = date == start.date();
                for h in 0..24i8 {
                    if !spec.hour.matches(h as u32) || (first_day && h < start_hms.0) {
                        continue;
                    }
                    for m in 0..60i8 {
                        if !spec.minute.matches(m as u32)
                            || (first_day && (h, m) < (start_hms.0, start_hms.1))
                        {
                            continue;
                        }
                        for s in 0..60i8 {
                            if !spec.second.matches(s as u32)
                                || (first_day && (h, m, s) < start_hms)
                                || (first_day && (h, m, s) == start_hms && !start_is_whole)
                            {
                                continue;
                            }
                            let civil = date.at(h, m, s, 0);
                            let ambiguous = zone.to_ambiguous_timestamp(civil);
                            match ambiguous.offset() {
                                AmbiguousOffset::Gap { .. } => {}
                                AmbiguousOffset::Unambiguous { .. } => {
                                    let t = ambiguous.compatible().unwrap();
                                    if t > after {
                                        return Some(t);
                                    }
                                }
                                AmbiguousOffset::Fold { .. } => {
                                    let earlier = ambiguous.earlier().unwrap();
                                    let later = ambiguous.later().unwrap();
                                    if let Some(t) =
                                        [earlier, later].into_iter().find(|t| *t > after)
                                    {
                                        return Some(t);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            date = date.tomorrow().ok()?;
        }
        None
    }

    fn reference_upcoming(
        spec: &Spec,
        mut after: Timestamp,
        zone: &TimeZone,
        last_day: Date,
        count: usize,
    ) -> Vec<Timestamp> {
        let mut out = Vec::new();
        while out.len() < count {
            let Some(t) = reference_next(spec, after, zone, last_day) else {
                break;
            };
            out.push(t);
            after = t;
        }
        out
    }
}
