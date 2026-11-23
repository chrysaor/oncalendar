#![no_main]

use jiff::Timestamp;
use jiff::tz::{Offset, TimeZone, TimeZoneDatabase};
use libfuzzer_sys::fuzz_target;
use oncalendar::CalendarEvent;

const NAMED_ZONES: [&str; 8] = [
    "UTC",
    "America/New_York",
    "Australia/Lord_Howe",
    "Pacific/Chatham",
    "Pacific/Apia",
    "Europe/Dublin",
    "Africa/Casablanca",
    "America/Havana",
];

const BASE_2026: i128 = 1_767_225_600;

fn base_time(seconds: i64, nanoseconds: u32) -> Timestamp {
    let min = i128::from(Timestamp::MIN.as_second());
    let span = i128::from(Timestamp::MAX.as_second()) - min;
    let offset = (i128::from(seconds) + BASE_2026 - min).rem_euclid(span);
    let second = i64::try_from(min + offset).unwrap_or(0);
    let nanosecond = i32::try_from(nanoseconds % 1_000_000_000).unwrap_or(0);
    Timestamp::new(second, nanosecond).unwrap_or(Timestamp::UNIX_EPOCH)
}

fn local_zone(database: &TimeZoneDatabase, choice: u8) -> TimeZone {
    let named = usize::from(choice) % (NAMED_ZONES.len() + 2);
    match NAMED_ZONES.get(named) {
        Some(name) => database.get(name).unwrap_or(TimeZone::UTC),
        None if named == NAMED_ZONES.len() => TimeZone::fixed(Offset::MAX),
        None => TimeZone::fixed(Offset::MIN),
    }
}

fuzz_target!(|input: (i64, u32, u8, u8, &str)| {
    let (seconds, nanoseconds, zone, count, expression) = input;
    let Ok(event) = expression.parse::<CalendarEvent>() else {
        return;
    };
    let database = TimeZoneDatabase::bundled();
    let local = local_zone(&database, zone);
    let after = base_time(seconds, nanoseconds);
    let Ok(upcoming) = event.upcoming_with(after, &local, &database) else {
        return;
    };
    let mut previous = after;
    for elapse in upcoming.take(usize::from(count % 16) + 1) {
        assert!(
            elapse > previous,
            "{event} from {after} in {local:?}: {elapse} does not follow {previous}"
        );
        previous = elapse;
    }
});
