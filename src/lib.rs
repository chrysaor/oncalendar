//! Parse systemd calendar event expressions (`OnCalendar=`) and compute when they elapse next.
//!
//! ```
//! use oncalendar::CalendarEvent;
//!
//! let event: CalendarEvent = "Mon..Fri 9:00".parse()?;
//! assert_eq!(event.to_string(), "Mon..Fri *-*-* 09:00:00");
//! # Ok::<(), oncalendar::ParseError>(())
//! ```
//!
//! With the `jiff` feature, `event.upcoming(after, &local)?.next()` is the first elapse after
//! `after`, or `None` when there is none.
//!
//! - [`CalendarEvent`] is a parsed expression. Parse it with [`str::parse`]. Its [`Display`]
//!   output is the normalized form, the same text `systemd-analyze calendar` prints.
//! - [`ParseError`] reports why an expression was rejected and the byte offset where the
//!   problem starts. Its message never repeats the input.
//! - With the `jiff` feature (on by default), `CalendarEvent::upcoming` lists the elapses
//!   after a given instant. The crate has no ambient "local time": the caller passes the
//!   time zone that stands for "local time". `CalendarEvent::upcoming_with` does the same with
//!   a time zone database the caller chooses, such as jiff's bundled one.
//! - With the `cron` feature (off by default), `CalendarEvent::from_cron` converts a
//!   five-field cron schedule into the calendar events that elapse at the same minutes. It adds
//!   no dependency and works without `std`.
//!
//! Without the `jiff` feature the crate parses and normalizes only and has no dependencies.
//! The crate is `no_std` and needs `alloc`. The `jiff` feature alone also builds for targets
//! without `std`. The `system-tzdb` feature (on by default) reads the system time zone
//! database and needs `std`. Without it, `CalendarEvent::upcoming` knows no zone name other
//! than `UTC`, and `CalendarEvent::upcoming_with` takes the database from the caller.
//! Parsing never touches the file system. A time zone name in an expression is checked for its
//! form when parsing and looked up in the time zone database when `CalendarEvent::upcoming` or
//! `CalendarEvent::upcoming_with` runs.
//!
//! [`Display`]: core::fmt::Display

#![no_std]

extern crate alloc;

#[cfg(feature = "cron")]
mod cron;
mod error;
mod format;
mod model;
#[cfg(feature = "jiff")]
mod next;
mod parse;
#[cfg(feature = "jiff")]
mod zone;

#[cfg(feature = "cron")]
pub use crate::cron::CronError;
pub use crate::error::ParseError;
pub use crate::model::CalendarEvent;
#[cfg(feature = "jiff")]
pub use crate::next::Upcoming;
#[cfg(feature = "jiff")]
pub use crate::zone::UnknownTimeZone;

#[cfg(all(doctest, feature = "jiff", feature = "cron"))]
#[doc = include_str!("../README.md")]
struct ReadmeExamples;
