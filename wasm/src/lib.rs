use core::fmt::Write as _;
use core::sync::atomic::{AtomicU8, Ordering};

use jiff::Timestamp;
use jiff::civil::DateTime;
use jiff::tz::{TimeZone, TimeZoneDatabase};
use oncalendar::CalendarEvent;

const INPUT_CAPACITY: usize = 64 * 1024;
const OUTPUT_CAPACITY: usize = 128 * 1024;
const MAX_ELAPSES: usize = 100;
const LOCAL_FORMAT: &str = "%a %Y-%m-%d %H:%M:%S%.f %Z";

static INPUT: [AtomicU8; INPUT_CAPACITY] = [const { AtomicU8::new(0) }; INPUT_CAPACITY];
static OUTPUT: [AtomicU8; OUTPUT_CAPACITY] = [const { AtomicU8::new(0) }; OUTPUT_CAPACITY];

#[unsafe(no_mangle)]
pub extern "C" fn input_buffer() -> *const AtomicU8 {
    INPUT.as_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn input_capacity() -> usize {
    INPUT_CAPACITY
}

#[unsafe(no_mangle)]
pub extern "C" fn output_buffer() -> *const AtomicU8 {
    OUTPUT.as_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn evaluate(input_len: usize, count: usize) -> usize {
    let response = match read_input(input_len) {
        Some(request) => respond(&request, count.min(MAX_ELAPSES)),
        None => bad_request("the request is not UTF-8 or does not fit the input buffer"),
    };
    write_output(&response)
}

#[unsafe(no_mangle)]
pub extern "C" fn zones() -> usize {
    let database = TimeZoneDatabase::bundled();
    let mut names: Vec<String> = database
        .available()
        .map(|name| name.as_str().to_owned())
        .collect();
    names.sort_unstable();
    write_output(&names.join("\n"))
}

fn read_input(len: usize) -> Option<String> {
    let bytes = INPUT
        .get(..len)?
        .iter()
        .map(|byte| byte.load(Ordering::Relaxed))
        .collect();
    String::from_utf8(bytes).ok()
}

fn write_output(response: &str) -> usize {
    let fallback = bad_request("the response does not fit the output buffer");
    let bytes = if response.len() <= OUTPUT_CAPACITY {
        response.as_bytes()
    } else {
        fallback.as_bytes()
    };
    for (cell, byte) in OUTPUT.iter().zip(bytes) {
        cell.store(*byte, Ordering::Relaxed);
    }
    bytes.len()
}

fn bad_request(message: &str) -> String {
    format!("bad-request\t{message}")
}

fn respond(request: &str, count: usize) -> String {
    let mut fields = request.splitn(3, '\t');
    let (Some(local), Some(base), Some(expression)) = (fields.next(), fields.next(), fields.next())
    else {
        return bad_request("the request needs a local time zone, a base time, and an expression");
    };
    let event = match expression.parse::<CalendarEvent>() {
        Ok(event) => event,
        Err(error) => return format!("invalid\t{}\t{error}", error.offset()),
    };
    let database = TimeZoneDatabase::bundled();
    let Ok(local) = database.get(local) else {
        return bad_request("the local time zone is not in the bundled database");
    };
    let Some(after) = parse_base(&database, base) else {
        return bad_request("the base time is not a civil time followed by a time zone name");
    };
    let upcoming = match event.upcoming_with(after, &local, &database) {
        Ok(upcoming) => upcoming,
        Err(error) => return format!("unknown-zone\t{event}\t{error}"),
    };
    let mut response = format!("ok\t{event}");
    for elapse in upcoming.take(count) {
        let local_time = elapse
            .to_zoned_datetime(local.clone())
            .strftime(LOCAL_FORMAT);
        let _ = write!(response, "\n{elapse}\t{local_time}");
    }
    response
}

fn parse_base(database: &TimeZoneDatabase, base: &str) -> Option<Timestamp> {
    let (civil, zone) = base.rsplit_once(' ')?;
    let civil: DateTime = civil.parse().ok()?;
    let zone: TimeZone = database.get(zone).ok()?;
    Some(civil.to_zoned_datetime(zone).ok()?.timestamp())
}
