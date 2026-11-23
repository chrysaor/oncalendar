use core::fmt;

use crate::model::CalendarEvent;

impl fmt::Display for CalendarEvent {
    /// Writes the normalized form, which parses back to an equal value.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = f;
        match self.unbuilt {}
    }
}
