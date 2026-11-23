use core::iter::FusedIterator;

use jiff::Timestamp;
use jiff::civil::{Date, DateTime};
use jiff::tz::{AmbiguousOffset, TimeZone, TimeZoneDatabase};

use crate::model::{
    CalendarEvent, Component, FieldSpec, Hour, MICROS, Minute, Month, WeekdaySet, Year,
};
use crate::zone::{UnknownTimeZone, resolve};

/// Search steps allowed in one `next` call. The proven bound is about 5,000.
const FUEL: u32 = 20_000;

impl CalendarEvent {
    /// The elapses strictly after `after`, in increasing order. Available with the `jiff`
    /// feature.
    ///
    /// `local` is the time zone for an expression that names none. An expression that names a
    /// time zone ignores `local`. `UTC` needs no database, and any other name is looked up in
    /// jiff's global time zone database, [`jiff::tz::db`]. Results have microsecond precision
    /// and lie in the years 1970 to 2199 on the wall clock of the effective time zone, so the
    /// UTC instant can be a few hours outside those years. A time that does not exist on the wall clock (a daylight saving
    /// gap) is skipped. A time that occurs twice (a fold) elapses once, at the earliest instant
    /// after `after`.
    ///
    /// The iterator can be very long (`*:*:0/0.000001` elapses every microsecond), so bound it
    /// with [`Iterator::take`] before collecting.
    ///
    /// # Errors
    ///
    /// [`UnknownTimeZone`] when the expression names a time zone that the time zone database
    /// does not have.
    ///
    /// # Examples
    ///
    /// ```
    /// use jiff::Timestamp;
    /// use jiff::tz::TimeZone;
    /// use oncalendar::CalendarEvent;
    ///
    /// let event: CalendarEvent = "*-*-* 12:00".parse()?;
    /// let after: Timestamp = "2026-01-01T12:00:00Z".parse()?;
    /// let mut elapses = event.upcoming(after, &TimeZone::UTC)?;
    /// assert_eq!(elapses.next(), Some("2026-01-02T12:00:00Z".parse()?));
    ///
    /// let unknown: CalendarEvent = "daily Nowhere/Bogus".parse()?;
    /// assert!(unknown.upcoming(after, &TimeZone::UTC).is_err());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn upcoming(
        &self,
        after: Timestamp,
        local: &TimeZone,
    ) -> Result<Upcoming<'_>, UnknownTimeZone> {
        self.upcoming_with(after, local, jiff::tz::db())
    }

    /// Like [`upcoming`](CalendarEvent::upcoming), with a time zone named in the expression
    /// looked up in `database`. Available with the `jiff` feature.
    ///
    /// Use it where the host has no time zone files, such as a minimal container, or to get the
    /// same answers on every host, with [`TimeZoneDatabase::bundled`] (jiff's
    /// `tzdb-bundle-always` feature). The name must match a name in `database` exactly, letter
    /// case included.
    ///
    /// # Errors
    ///
    /// [`UnknownTimeZone`] when the expression names a time zone that `database` does not have.
    ///
    /// # Examples
    ///
    /// ```
    /// use jiff::Timestamp;
    /// use jiff::tz::{TimeZone, TimeZoneDatabase};
    /// use oncalendar::CalendarEvent;
    ///
    /// let database = TimeZoneDatabase::bundled();
    /// let after: Timestamp = "2026-07-01T00:00:00Z".parse()?;
    ///
    /// let event: CalendarEvent = "*-*-* 09:00 Europe/Berlin".parse()?;
    /// let next = event.upcoming_with(after, &TimeZone::UTC, &database)?.next();
    /// assert_eq!(next, Some("2026-07-01T07:00:00Z".parse()?));
    ///
    /// let lower_case: CalendarEvent = "*-*-* 09:00 europe/berlin".parse()?;
    /// assert!(lower_case.upcoming_with(after, &TimeZone::UTC, &database).is_err());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn upcoming_with(
        &self,
        after: Timestamp,
        local: &TimeZone,
        database: &TimeZoneDatabase,
    ) -> Result<Upcoming<'_>, UnknownTimeZone> {
        let zone = resolve(&self.zone, local, database)?;
        let after = after
            .as_nanosecond()
            .checked_div_euclid(1_000)
            .and_then(|micros| i64::try_from(micros).ok());
        Ok(Upcoming {
            event: self,
            zone,
            after,
        })
    }
}

/// The elapses of a [`CalendarEvent`] after an instant. Made by [`CalendarEvent::upcoming`]
/// and [`CalendarEvent::upcoming_with`].
///
/// Once it returns `None` it keeps returning `None`. Available with the `jiff` feature.
///
/// ```
/// use jiff::Timestamp;
/// use jiff::tz::TimeZone;
/// use oncalendar::CalendarEvent;
///
/// let event: CalendarEvent = "2026-03-15 08:00".parse()?;
/// let after: Timestamp = "2026-01-01T00:00:00Z".parse()?;
/// let mut elapses = event.upcoming(after, &TimeZone::UTC)?;
/// assert_eq!(elapses.next(), Some("2026-03-15T08:00:00Z".parse()?));
/// assert_eq!(elapses.next(), None);
/// assert_eq!(elapses.next(), None);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Debug)]
pub struct Upcoming<'a> {
    event: &'a CalendarEvent,
    zone: TimeZone,
    after: Option<i64>,
}

impl Iterator for Upcoming<'_> {
    type Item = Timestamp;

    fn next(&mut self) -> Option<Timestamp> {
        let mut fuel = FUEL;
        let found = next_elapse(self.event, &self.zone, self.after?, &mut fuel);
        self.after = found.map(Timestamp::as_microsecond);
        found
    }
}

impl FusedIterator for Upcoming<'_> {}

/// A wall-clock time in the search. Fields may run one past their range, which carries.
#[derive(Clone, Copy, Debug)]
struct Civil {
    year: u32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    micros: u32,
}

impl Civil {
    fn start_of_month(year: u32, month: u32) -> Self {
        Self {
            year,
            month,
            day: 1,
            hour: 0,
            minute: 0,
            micros: 0,
        }
    }

    /// The wall-clock time of `datetime`, or the start of 1970 when it is earlier.
    fn from_datetime(datetime: DateTime) -> Option<Self> {
        let Ok(year) = u32::try_from(datetime.year()) else {
            return Some(Self::start_of_month(Year::MIN, 1));
        };
        if year < Year::MIN {
            return Some(Self::start_of_month(Year::MIN, 1));
        }
        let nanos = u32::try_from(datetime.subsec_nanosecond()).ok()?;
        let micros = u32::try_from(datetime.second())
            .ok()?
            .checked_mul(MICROS)?
            .checked_add(nanos.checked_div(1_000)?)?;
        Some(Self {
            year,
            month: u32::try_from(datetime.month()).ok()?,
            day: u32::try_from(datetime.day()).ok()?,
            hour: u32::try_from(datetime.hour()).ok()?,
            minute: u32::try_from(datetime.minute()).ok()?,
            micros,
        })
    }

    fn to_datetime(self) -> Option<DateTime> {
        let second = self.micros.checked_div(MICROS)?;
        let nanos = self.micros.checked_rem(MICROS)?.checked_mul(1_000)?;
        DateTime::new(
            i16::try_from(self.year).ok()?,
            i8::try_from(self.month).ok()?,
            i8::try_from(self.day).ok()?,
            i8::try_from(self.hour).ok()?,
            i8::try_from(self.minute).ok()?,
            i8::try_from(second).ok()?,
            i32::try_from(nanos).ok()?,
        )
        .ok()
    }
}

/// The first elapse strictly after `after`, in microseconds since the epoch.
///
/// Each search step burns one unit of `fuel`. With no fuel left the answer is `None`.
fn next_elapse(
    event: &CalendarEvent,
    zone: &TimeZone,
    after: i64,
    fuel: &mut u32,
) -> Option<Timestamp> {
    let start = Timestamp::from_microsecond(after.checked_add(1)?).ok()?;
    let mut cursor = Civil::from_datetime(zone.to_datetime(start))?;
    let mut floor = Floor::None;
    loop {
        let (candidate, kept) = first_match_at_or_after(event, cursor, floor, fuel)?;
        let datetime = candidate.to_datetime()?;
        let (first, second) = match zone.to_ambiguous_timestamp(datetime).offset() {
            AmbiguousOffset::Unambiguous { offset } => (offset, offset),
            AmbiguousOffset::Fold { before, after } => (before, after),
            AmbiguousOffset::Gap { before, .. } => {
                cursor = gap_end(zone, datetime, before)?;
                floor = kept;
                continue;
            }
        };
        let later_than_after = |offset: jiff::tz::Offset| {
            offset
                .to_timestamp(datetime)
                .ok()
                .filter(|t| t.as_microsecond() > after)
        };
        let found = later_than_after(first).or_else(|| later_than_after(second))?;
        if matches!(kept, Floor::Second(_))
            && kept != floor
            && fell_back_between(zone, after, found)
        {
            cursor = Civil {
                micros: 0,
                ..candidate
            };
            floor = Floor::None;
            continue;
        }
        return Some(found);
    }
}

/// Whether the wall clock is set back at an instant later than `after` and no later than `found`.
fn fell_back_between(zone: &TimeZone, after: i64, found: Timestamp) -> bool {
    let Some(probe) = found
        .as_microsecond()
        .checked_add(1)
        .and_then(|t| Timestamp::from_microsecond(t).ok())
    else {
        return false;
    };
    let Some(transition) = zone.preceding(probe).next() else {
        return false;
    };
    let at = transition.timestamp();
    let Ok(just_before) = at.checked_sub(jiff::SignedDuration::from_micros(1)) else {
        return false;
    };
    at.as_microsecond() > after && transition.offset() < zone.to_offset(just_before)
}

/// The wall-clock time at the end of the gap that holds `datetime`.
fn gap_end(zone: &TimeZone, datetime: DateTime, before: jiff::tz::Offset) -> Option<Civil> {
    let inside = before.to_timestamp(datetime).ok()?;
    let probe = Timestamp::from_microsecond(inside.as_microsecond().checked_add(1)?).ok()?;
    let transition = zone.preceding(probe).next()?.timestamp();
    Civil::from_datetime(zone.to_datetime(transition))
}

/// A lower bound on one time field, left by a wrap in [`first_match_at_or_after`].
///
/// It holds until a larger field changes in the search, and it also holds across a restart at
/// the end of a daylight saving gap (observed).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Floor {
    None,
    Hour(u32),
    Minute(u32),
    Second(u32),
}

impl Floor {
    /// Moves `c` up to the floor when it is below it.
    fn raise(self, mut c: Civil) -> Civil {
        match self {
            Floor::Hour(hour) if c.hour < hour => {
                c.hour = hour;
                c.minute = 0;
                c.micros = 0;
            }
            Floor::Minute(minute) if c.minute < minute => {
                c.minute = minute;
                c.micros = 0;
            }
            Floor::Second(micros) if c.micros < micros => c.micros = micros,
            _ => {}
        }
        c
    }
}

/// The first wall-clock time at or after `cursor`, raised to `floor`, that matches every field
/// of `event`, and the floor in effect when it was found.
///
/// A field with no value left carries into the next larger field and starts that period at its
/// minimum. One exception follows systemd (observed): when the hour, minute, or second field
/// runs out at a value inside its range and the carry also runs past the end of the next larger
/// field, the new period starts at [`Component::wrapped_floor`] instead.
fn first_match_at_or_after(
    event: &CalendarEvent,
    cursor: Civil,
    mut floor: Floor,
    fuel: &mut u32,
) -> Option<(Civil, Floor)> {
    let date = &event.date;
    let time = &event.time;
    let mut c = floor.raise(cursor);
    loop {
        *fuel = fuel.checked_sub(1)?;
        let year = date.year.next_at_or_after(c.year)?;
        if year != c.year {
            c = Civil::start_of_month(year, 1);
            floor = Floor::None;
        }
        let Some(month) = date.month.next_at_or_after(c.month) else {
            c = Civil::start_of_month(year.checked_add(1)?, 1);
            floor = Floor::None;
            continue;
        };
        if month != c.month {
            c = Civil::start_of_month(year, month);
            floor = Floor::None;
        }
        let first = Date::new(i16::try_from(year).ok()?, i8::try_from(month).ok()?, 1).ok()?;
        let len = u32::try_from(first.days_in_month()).ok()?;
        let mask = date.day.mask(len) & weekday_mask(event.weekdays, first, len)?;
        let Some(day) = first_day_at_or_after(mask, c.day) else {
            c = next_month(year, month)?;
            floor = Floor::None;
            continue;
        };
        if day != c.day {
            c = Civil {
                day,
                ..Civil::start_of_month(year, month)
            };
            floor = Floor::None;
        }
        let next_day = || {
            if day < len {
                Some(Civil {
                    day: day.checked_add(1)?,
                    ..Civil::start_of_month(year, month)
                })
            } else {
                next_month(year, month)
            }
        };
        let Some(hour) = time.hour.next_at_or_after(c.hour) else {
            floor = wrapped_floor(&time.hour, c.hour, day == len).map_or(Floor::None, Floor::Hour);
            c = floor.raise(next_day()?);
            continue;
        };
        if hour != c.hour {
            c.hour = hour;
            c.minute = 0;
            c.micros = 0;
            if matches!(floor, Floor::Minute(_) | Floor::Second(_)) {
                floor = Floor::None;
            }
        }
        let Some(minute) = time.minute.next_at_or_after(c.minute) else {
            if hour < Hour::MAX {
                c.hour = hour.checked_add(1)?;
                c.minute = 0;
                c.micros = 0;
                floor = Floor::None;
            } else {
                floor =
                    wrapped_floor(&time.minute, c.minute, true).map_or(Floor::None, Floor::Minute);
                c = floor.raise(next_day()?);
            }
            continue;
        };
        if minute != c.minute {
            c.minute = minute;
            c.micros = 0;
            if matches!(floor, Floor::Second(_)) {
                floor = Floor::None;
            }
        }
        let Some(micros) = time.second.next_at_or_after(c.micros) else {
            if minute < Minute::MAX {
                c.minute = minute.checked_add(1)?;
                c.micros = 0;
                floor = Floor::None;
            } else {
                floor =
                    wrapped_floor(&time.second, c.micros, true).map_or(Floor::None, Floor::Second);
                if hour < Hour::MAX {
                    c.hour = hour.checked_add(1)?;
                    c.minute = 0;
                    c.micros = 0;
                } else {
                    c = next_day()?;
                }
                c = floor.raise(c);
            }
            continue;
        };
        c.micros = micros;
        return Some((c, floor));
    }
}

/// The floor a time field leaves after it ran out at `x`. `wraps` says whether the carry also
/// runs past the end of the next larger field. Only then, and only when `x` lies inside the
/// field, is there a floor: [`Component::wrapped_floor`].
fn wrapped_floor<F: FieldSpec>(component: &Component<F>, x: u32, wraps: bool) -> Option<u32> {
    if wraps && x <= F::MAX {
        component.wrapped_floor(x)
    } else {
        None
    }
}

/// The start of the first day of the month after `month`.
fn next_month(year: u32, month: u32) -> Option<Civil> {
    if month < Month::MAX {
        Some(Civil::start_of_month(year, month.checked_add(1)?))
    } else {
        Some(Civil::start_of_month(year.checked_add(1)?, Month::MIN))
    }
}

/// The first set bit at or after day `from`, where bit `d - 1` stands for day `d`.
fn first_day_at_or_after(mask: u32, from: u32) -> Option<u32> {
    let rest = mask.checked_shr(from.checked_sub(1)?)?;
    if rest == 0 {
        return None;
    }
    from.checked_add(rest.trailing_zeros())
}

/// Bit `d - 1` is set when day `d` of the month that starts on `first` falls on a day in
/// `weekdays`.
fn weekday_mask(weekdays: WeekdaySet, first: Date, len: u32) -> Option<u32> {
    if weekdays == WeekdaySet::ALL {
        return Some(u32::MAX);
    }
    let mut weekday = u32::try_from(first.weekday().to_monday_zero_offset()).ok()?;
    let mut mask = 0_u32;
    for day in 0..len {
        if weekdays.contains(weekday) {
            mask |= 1_u32.checked_shl(day).unwrap_or(0);
        }
        weekday = weekday.checked_add(1)?.checked_rem(7)?;
    }
    Some(mask)
}

#[cfg(test)]
mod tests {
    extern crate std;

    use std::println;

    use super::*;

    const EXPECTED: &str = include_str!("../tests/fixtures/expected.tsv");
    const BASES: [i64; 3] = [
        1_767_225_600_000_000,
        6_942_585_600_000_000,
        1_793_512_799_999_999,
    ];

    #[test]
    fn the_fixture_corpus_never_runs_out_of_fuel() {
        let database = TimeZoneDatabase::bundled();
        let mut least_left = FUEL;
        let mut searches = 0_u32;
        for line in EXPECTED.lines().filter(|line| !line.starts_with('#')) {
            let mut fields = line.split('\t');
            let (Some(local), Some(_), Some(expression)) =
                (fields.next(), fields.next(), fields.next())
            else {
                continue;
            };
            let (Ok(event), Ok(local)) = (expression.parse::<CalendarEvent>(), database.get(local))
            else {
                continue;
            };
            let Ok(zone) = resolve(&event.zone, &local, &database) else {
                continue;
            };
            for base in BASES {
                let mut after = Some(base);
                for _ in 0..5 {
                    let Some(previous) = after else {
                        break;
                    };
                    let mut fuel = FUEL;
                    after = next_elapse(&event, &zone, previous, &mut fuel)
                        .map(Timestamp::as_microsecond);
                    least_left = least_left.min(fuel);
                    searches = searches.saturating_add(1);
                }
            }
        }
        println!(
            "{searches} searches, at most {} steps",
            FUEL.saturating_sub(least_left)
        );
        assert!(
            searches > 0,
            "not exercised: no fixture expression was searched"
        );
        assert!(least_left > 0, "a search ran out of fuel");
    }
}
