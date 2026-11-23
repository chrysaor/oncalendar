#![no_main]

use libfuzzer_sys::fuzz_target;
use oncalendar::CalendarEvent;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = core::str::from_utf8(data) else {
        return;
    };
    let events = match CalendarEvent::from_cron(text) {
        Ok(events) => events,
        Err(error) => {
            assert!(
                error.offset() <= text.len() && text.is_char_boundary(error.offset()),
                "offset {} is not a character boundary of the input",
                error.offset()
            );
            return;
        }
    };
    assert!(
        (1..=2).contains(&events.len()),
        "{} events from one cron expression",
        events.len()
    );
    for event in events {
        let normalized = event.to_string();
        let reparsed = normalized
            .parse::<CalendarEvent>()
            .unwrap_or_else(|error| panic!("converted form {normalized:?} does not parse: {error}"));
        assert_eq!(
            reparsed, event,
            "converted form {normalized:?} parses to another event"
        );
    }
});
