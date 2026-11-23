#![no_main]

use libfuzzer_sys::fuzz_target;
use oncalendar::CalendarEvent;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = core::str::from_utf8(data) else {
        return;
    };
    let Ok(event) = text.parse::<CalendarEvent>() else {
        return;
    };
    let normalized = event.to_string();
    let reparsed = normalized
        .parse::<CalendarEvent>()
        .unwrap_or_else(|error| panic!("normalized form {normalized:?} does not parse: {error}"));
    assert_eq!(
        reparsed, event,
        "normalized form {normalized:?} parses to another event"
    );
    assert_eq!(
        reparsed.to_string(),
        normalized,
        "normalizing twice changes {normalized:?}"
    );
});
