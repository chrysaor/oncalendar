use core::iter::FusedIterator;

use jiff::Timestamp;
use jiff::tz::TimeZone;

use crate::model::CalendarEvent;
use crate::zone::UnknownTimeZone;

impl CalendarEvent {
    /// The elapses strictly after `after`, in increasing order.
    ///
    /// `local` is the time zone for an expression that names none. An expression that names a
    /// time zone ignores it. Results have microsecond precision and lie in the years 1970 to
    /// 2199. A time that does not exist on the wall clock (a daylight saving gap) is skipped.
    /// A time that occurs twice (a fold) elapses once, at the earliest instant after `after`.
    ///
    /// The iterator can be very long (`*:*:0/0.000001` elapses every microsecond), so bound it
    /// with [`Iterator::take`] before collecting.
    ///
    /// # Errors
    ///
    /// [`UnknownTimeZone`] when the expression names a time zone that the time zone database
    /// does not have.
    pub fn upcoming<'a>(
        &'a self,
        after: Timestamp,
        local: &'a TimeZone,
    ) -> Result<Upcoming<'a>, UnknownTimeZone> {
        let _ = (after, local);
        match self.unbuilt {}
    }
}

/// The elapses of a [`CalendarEvent`] after an instant. Made by [`CalendarEvent::upcoming`].
///
/// Once it returns `None` it keeps returning `None`.
#[derive(Clone, Debug)]
pub struct Upcoming<'a> {
    event: &'a CalendarEvent,
}

impl Iterator for Upcoming<'_> {
    type Item = Timestamp;

    fn next(&mut self) -> Option<Timestamp> {
        match self.event.unbuilt {}
    }
}

impl FusedIterator for Upcoming<'_> {}
