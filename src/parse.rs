use core::str::FromStr;

use crate::error::ParseError;
use crate::model::CalendarEvent;

impl FromStr for CalendarEvent {
    type Err = ParseError;

    /// Parses an expression in the grammar of `systemd.time(7)`, "Calendar Events".
    ///
    /// Leading and trailing whitespace is an error. Each numeric field takes at most 241 list
    /// items. A time zone name is checked for its form only.
    fn from_str(input: &str) -> Result<Self, ParseError> {
        let _ = input;
        Err(ParseError::new(0))
    }
}
