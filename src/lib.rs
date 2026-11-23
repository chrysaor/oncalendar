//! Parse systemd calendar event expressions (`OnCalendar=`) and compute when they elapse next.
//!
//! ```text
//! let event: CalendarEvent = "Mon..Fri *-*-* 09:00".parse()?;
//! event.to_string();                       // "Mon..Fri *-*-* 09:00:00"
//! event.upcoming(now, &local)?.next();     // the next elapse after `now`, or `None`
//! ```
//!
//! - [`CalendarEvent`] is a parsed expression. Parse it with [`str::parse`]. Its [`Display`]
//!   output is the normalized form, the same text `systemd-analyze calendar` prints.
//! - [`ParseError`] reports why an expression was rejected and the byte offset where the
//!   problem starts. Its message never repeats the input.
//! - With the `jiff` feature (on by default), `CalendarEvent::upcoming` lists the elapses
//!   after a given instant. The crate has no ambient "local time": the caller passes the
//!   time zone that stands for "local time". `CalendarEvent::upcoming_with` does the same with
//!   a time zone database the caller chooses, such as jiff's bundled one.
//!
//! Without the `jiff` feature the crate parses and normalizes only and has no dependencies.
//! Parsing never touches the file system. A time zone name in an expression is checked for its
//! form when parsing and looked up in the time zone database when `CalendarEvent::upcoming` or
//! `CalendarEvent::upcoming_with` runs.
//!
//! [`Display`]: core::fmt::Display

mod error;
mod format;
mod model;
#[cfg(feature = "jiff")]
mod next;
mod parse;
#[cfg(feature = "jiff")]
mod zone;

pub use crate::error::ParseError;
pub use crate::model::CalendarEvent;
#[cfg(feature = "jiff")]
pub use crate::next::Upcoming;
#[cfg(feature = "jiff")]
pub use crate::zone::UnknownTimeZone;
