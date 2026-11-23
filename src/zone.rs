use core::fmt;

/// The time zone named in the expression is not in the time zone database.
///
/// Parsing accepts any well-formed name, so this is reported by
/// [`CalendarEvent::upcoming`](crate::CalendarEvent::upcoming).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownTimeZone {
    _private: (),
}

impl fmt::Display for UnknownTimeZone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the time zone in the calendar expression is not in the time zone database")
    }
}

impl std::error::Error for UnknownTimeZone {}
