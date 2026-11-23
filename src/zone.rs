use core::fmt;

use jiff::tz::{TimeZone, TimeZoneDatabase};

use crate::model::Zone;

/// The time zone named in the expression is not in the time zone database.
///
/// Parsing accepts any well-formed name, so this is reported by
/// [`CalendarEvent::upcoming`](crate::CalendarEvent::upcoming) and
/// [`CalendarEvent::upcoming_with`](crate::CalendarEvent::upcoming_with). Local time zone
/// abbreviations such as `KST` are not supported and are reported the same way. Available with
/// the `jiff` feature.
///
/// ```
/// use jiff::Timestamp;
/// use jiff::tz::TimeZone;
/// use oncalendar::CalendarEvent;
///
/// let event: CalendarEvent = "daily Nowhere/Bogus".parse()?;
/// let error = event.upcoming(Timestamp::UNIX_EPOCH, &TimeZone::UTC).unwrap_err();
/// assert!(error.to_string().contains("not in the time zone database"));
/// # Ok::<(), oncalendar::ParseError>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownTimeZone {
    _private: (),
}

impl fmt::Display for UnknownTimeZone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(
            "the time zone in the calendar expression is not in the time zone database \
             (local abbreviations such as KST are not supported)",
        )
    }
}

impl core::error::Error for UnknownTimeZone {}

/// The zone the search runs in. A named zone must match a name in `database` exactly, case
/// included.
pub(crate) fn resolve(
    zone: &Zone,
    local: &TimeZone,
    database: &TimeZoneDatabase,
) -> Result<TimeZone, UnknownTimeZone> {
    let unknown = UnknownTimeZone { _private: () };
    match zone {
        Zone::Local => Ok(local.clone()),
        Zone::Utc => Ok(TimeZone::UTC),
        Zone::Named(name) => {
            let found = database.get(name).map_err(|_| unknown.clone())?;
            if found.iana_name() == Some(&**name) {
                Ok(found)
            } else {
                Err(unknown)
            }
        }
    }
}
