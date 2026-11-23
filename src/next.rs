use core::iter::FusedIterator;

use jiff::Timestamp;
use jiff::civil::{Date, DateTime};
use jiff::tz::{AmbiguousOffset, TimeZone};

use crate::model::{CalendarEvent, FieldSpec, MICROS, WeekdaySet, Year};
use crate::zone::{UnknownTimeZone, resolve};

/// Search steps allowed in one `next` call. The proven bound is about 5,000.
const FUEL: u32 = 20_000;

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
        let zone = resolve(&self.zone, local)?;
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

/// The elapses of a [`CalendarEvent`] after an instant. Made by [`CalendarEvent::upcoming`].
///
/// Once it returns `None` it keeps returning `None`.
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
    loop {
        let candidate = first_match_at_or_after(event, cursor, fuel)?;
        let datetime = candidate.to_datetime()?;
        let later_than_after = |offset: jiff::tz::Offset| {
            offset
                .to_timestamp(datetime)
                .ok()
                .filter(|t| t.as_microsecond() > after)
        };
        let skip_one = || {
            let mut next = candidate;
            next.micros = next.micros.checked_add(1)?;
            Some(next)
        };
        cursor = match zone.to_ambiguous_timestamp(datetime).offset() {
            AmbiguousOffset::Unambiguous { offset } => match later_than_after(offset) {
                Some(t) => return Some(t),
                None => skip_one()?,
            },
            AmbiguousOffset::Fold { before, after } => {
                match later_than_after(before).or_else(|| later_than_after(after)) {
                    Some(t) => return Some(t),
                    None => skip_one()?,
                }
            }
            AmbiguousOffset::Gap { before, .. } => match gap_end(zone, datetime, before) {
                Some(end) => end,
                None => skip_one()?,
            },
        };
    }
}

/// The wall-clock time at the end of the gap that holds `datetime`.
fn gap_end(zone: &TimeZone, datetime: DateTime, before: jiff::tz::Offset) -> Option<Civil> {
    let inside = before.to_timestamp(datetime).ok()?;
    let probe = Timestamp::from_microsecond(inside.as_microsecond().checked_add(1)?).ok()?;
    let transition = zone.preceding(probe).next()?.timestamp();
    Civil::from_datetime(zone.to_datetime(transition))
}

/// The first wall-clock time at or after `cursor` that matches every field of `event`.
fn first_match_at_or_after(event: &CalendarEvent, cursor: Civil, fuel: &mut u32) -> Option<Civil> {
    let date = &event.date;
    let time = &event.time;
    let mut c = cursor;
    loop {
        *fuel = fuel.checked_sub(1)?;
        let year = date.year.next_at_or_after(c.year)?;
        if year != c.year {
            c = Civil::start_of_month(year, 1);
        }
        let Some(month) = date.month.next_at_or_after(c.month) else {
            c = Civil::start_of_month(year.checked_add(1)?, 1);
            continue;
        };
        if month != c.month {
            c = Civil::start_of_month(year, month);
        }
        let len = days_in_month(year, month)?;
        let mask = date.day.mask(len) & weekday_mask(event.weekdays, year, month, len)?;
        let Some(day) = first_day_at_or_after(mask, c.day) else {
            c = Civil::start_of_month(year, month.checked_add(1)?);
            continue;
        };
        if day != c.day {
            c = Civil {
                day,
                ..Civil::start_of_month(year, month)
            };
        }
        let Some(hour) = time.hour.next_at_or_after(c.hour) else {
            c = Civil {
                day: day.checked_add(1)?,
                ..Civil::start_of_month(year, month)
            };
            continue;
        };
        if hour != c.hour {
            c.hour = hour;
            c.minute = 0;
            c.micros = 0;
        }
        let Some(minute) = time.minute.next_at_or_after(c.minute) else {
            c.hour = hour.checked_add(1)?;
            c.minute = 0;
            c.micros = 0;
            continue;
        };
        if minute != c.minute {
            c.minute = minute;
            c.micros = 0;
        }
        let Some(micros) = time.second.next_at_or_after(c.micros) else {
            c.minute = minute.checked_add(1)?;
            c.micros = 0;
            continue;
        };
        c.micros = micros;
        return Some(c);
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

fn days_in_month(year: u32, month: u32) -> Option<u32> {
    let first = Date::new(i16::try_from(year).ok()?, i8::try_from(month).ok()?, 1).ok()?;
    u32::try_from(first.days_in_month()).ok()
}

/// Bit `d - 1` is set when day `d` of the month falls on a day in `weekdays`.
fn weekday_mask(weekdays: WeekdaySet, year: u32, month: u32, len: u32) -> Option<u32> {
    if weekdays == WeekdaySet::ALL {
        return Some(u32::MAX);
    }
    let first = Date::new(i16::try_from(year).ok()?, i8::try_from(month).ok()?, 1).ok()?;
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
    use super::*;

    const EXPECTED: &str = include_str!("../tests/fixtures/expected.tsv");
    const BASES: [i64; 3] = [
        1_767_225_600_000_000,
        6_942_585_600_000_000,
        1_793_512_799_999_999,
    ];

    #[test]
    fn the_fixture_corpus_never_runs_out_of_fuel() {
        let mut least_left = FUEL;
        let mut searches = 0_u32;
        for line in EXPECTED.lines().filter(|line| !line.starts_with('#')) {
            let mut fields = line.split('\t');
            let (Some(local), Some(_), Some(expression)) =
                (fields.next(), fields.next(), fields.next())
            else {
                continue;
            };
            let (Ok(event), Ok(local)) =
                (expression.parse::<CalendarEvent>(), TimeZone::get(local))
            else {
                continue;
            };
            let Ok(zone) = resolve(&event.zone, &local) else {
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
