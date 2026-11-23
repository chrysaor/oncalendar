use alloc::vec::Vec;
use core::str::FromStr;

use crate::error::{ErrorKind, ParseError};
use crate::model::{
    CalendarEvent, Component, DateSpec, Day, DayFromEnd, DaySpec, FieldSpec, Hour, Item, Items,
    MAX_ITEMS, MICROS, Minute, Month, Second, TimeSpec, WeekdaySet, Year, Zone,
};

/// Longer than any IANA time zone name.
const MAX_ZONE_LEN: usize = 64;

/// The largest number systemd accepts, 2^31 - 1. A seconds number counts in microseconds.
const MAX_NUMBER: u32 = 2_147_483_647;

const WEEKDAYS: [(&str, &str); 7] = [
    ("mon", "monday"),
    ("tue", "tuesday"),
    ("wed", "wednesday"),
    ("thu", "thursday"),
    ("fri", "friday"),
    ("sat", "saturday"),
    ("sun", "sunday"),
];

const SHORTHANDS: [(&str, &str); 9] = [
    ("minutely", "*-*-* *:*:00"),
    ("hourly", "*-*-* *:00:00"),
    ("daily", "*-*-* 00:00:00"),
    ("weekly", "Mon *-*-* 00:00:00"),
    ("monthly", "*-*-01 00:00:00"),
    ("yearly", "*-01-01 00:00:00"),
    ("annually", "*-01-01 00:00:00"),
    ("quarterly", "*-01,04,07,10-01 00:00:00"),
    ("semiannually", "*-01,07-01 00:00:00"),
];

impl FromStr for CalendarEvent {
    type Err = ParseError;

    /// Parses an expression in the grammar of `systemd.time(7)`, "Calendar Events".
    ///
    /// Leading whitespace is an error, and so is trailing whitespace after a time or a
    /// shorthand. Each numeric field takes at most 241 list items. A time zone name is checked
    /// for its form only.
    ///
    /// # Errors
    ///
    /// [`ParseError`] when the input is not a calendar event expression, such as an empty
    /// string, a value out of range, or a misplaced space.
    fn from_str(input: &str) -> Result<Self, ParseError> {
        if input.is_empty() {
            return Err(ParseError::new(ErrorKind::Empty, 0));
        }
        let error = match expression(input) {
            Ok(event) => return Ok(event),
            Err(error) => error,
        };
        let Some((head, zone)) = split_zone(input) else {
            return Err(error);
        };
        let mut event = expression(head).map_err(|_| error)?;
        event.zone = zone;
        Ok(event)
    }
}

/// Splits `head zone` at the last space when the last word has the form of a zone name.
fn split_zone(input: &str) -> Option<(&str, Zone)> {
    let (head, name) = input.rsplit_once(' ')?;
    if head.is_empty() || !is_zone_name(name) {
        return None;
    }
    let zone = if name.eq_ignore_ascii_case("UTC") {
        Zone::Utc
    } else {
        Zone::Named(name.into())
    };
    Some((head, zone))
}

/// ASCII letters, digits, `_`, `+`, `-` and `/`, at most 64 bytes, no empty path component.
fn is_zone_name(name: &str) -> bool {
    (1..=MAX_ZONE_LEN).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'+' | b'-' | b'/'))
        && name.split('/').all(|component| !component.is_empty())
}

/// An expression without a time zone. Its zone is [`Zone::Local`].
fn expression(text: &str) -> Result<CalendarEvent, ParseError> {
    if text.starts_with(' ') {
        return Err(ParseError::new(ErrorKind::Whitespace, 0));
    }
    let trimmed = text.trim_end_matches(' ');
    let trailing_space = ParseError::new(ErrorKind::Whitespace, trimmed.len());
    if let Some((_, expansion)) = SHORTHANDS
        .iter()
        .find(|(name, _)| trimmed.eq_ignore_ascii_case(name))
    {
        if trimmed.len() != text.len() {
            return Err(trailing_space);
        }
        return expression(expansion);
    }

    let mut tokens = tokens(text);
    let mut token = tokens.next();
    let mut weekdays = WeekdaySet::ALL;
    if let Some((at, word)) =
        token.filter(|(_, word)| word.starts_with(|c: char| c.is_ascii_alphabetic()))
    {
        weekdays = weekday_list(word, at)?;
        token = tokens.next();
    }
    let mut date = DateSpec {
        year: Component::Any,
        month: Component::Any,
        day: DaySpec::FromStart(Component::Any),
    };
    if let Some((at, word)) = token.filter(|(_, word)| !word.contains(':')) {
        date = date_spec(word, at)?;
        token = tokens.next();
    }
    let mut time = TimeSpec {
        hour: zero(0)?,
        minute: zero(0)?,
        second: zero(0)?,
    };
    let has_time = token.is_some();
    if let Some((at, word)) = token {
        time = time_spec(word, at)?;
        token = tokens.next();
    }
    if let Some((at, _)) = token {
        return Err(ParseError::new(ErrorKind::Syntax, at));
    }
    if has_time && trimmed.len() != text.len() {
        return Err(trailing_space);
    }
    Ok(CalendarEvent {
        weekdays,
        date,
        time,
        zone: Zone::Local,
    })
}

/// The component that holds only the field's value 0.
fn zero<F: FieldSpec>(at: usize) -> Result<Component<F>, ParseError> {
    Component::value(0).map_err(|kind| ParseError::new(kind, at))
}

/// The words of `text` with their byte offsets. Runs of spaces separate words.
fn tokens(text: &str) -> impl Iterator<Item = (usize, &str)> {
    pieces(text, ' ').filter(|(_, word)| !word.is_empty())
}

/// The pieces of `text` between `separator`s, with their byte offsets.
fn pieces(text: &str, separator: char) -> impl Iterator<Item = (usize, &str)> {
    text.split(separator).scan(0_usize, |next, piece| {
        let at = *next;
        *next = next.saturating_add(piece.len()).saturating_add(1);
        Some((at, piece))
    })
}

/// `Mon`, `Monday`, `Mon..Fri`, `Mon-Fri`, comma separated, with one optional trailing comma.
fn weekday_list(word: &str, at: usize) -> Result<WeekdaySet, ParseError> {
    let error = |at| ParseError::new(ErrorKind::Weekday, at);
    let body = word.strip_suffix(',').unwrap_or(word);
    let mut bits = 0_u8;
    for (offset, item) in pieces(body, ',') {
        let item_at = at.saturating_add(offset);
        let (first, last) = match item.split_once("..").or_else(|| item.split_once('-')) {
            Some((first, last)) => (first, Some(last)),
            None => (item, None),
        };
        let first = weekday(first).ok_or_else(|| error(item_at))?;
        let last = match last {
            Some(last) => weekday(last).ok_or_else(|| error(item_at))?,
            None => first,
        };
        if last < first {
            return Err(error(item_at));
        }
        for day in first..=last {
            bits |= 1_u8.checked_shl(day).unwrap_or(0);
        }
    }
    WeekdaySet::from_bits(bits).ok_or_else(|| error(at))
}

fn weekday(name: &str) -> Option<u32> {
    let index = WEEKDAYS.iter().position(|(short, long)| {
        name.eq_ignore_ascii_case(short) || name.eq_ignore_ascii_case(long)
    })?;
    u32::try_from(index).ok()
}

/// `[year-]month-day` or `[year-]month~day`. Short dates fill from the right.
fn date_spec(word: &str, at: usize) -> Result<DateSpec, ParseError> {
    let (head, day, from_end) = match word.split_once('~') {
        Some((head, day)) => (head, day, true),
        None => match word.rsplit_once('-') {
            Some((head, day)) => (head, day, false),
            None => return Err(ParseError::new(ErrorKind::Syntax, at)),
        },
    };
    let day_at = at.saturating_add(head.len()).saturating_add(1);
    let (year, month, month_at) = match head.split_once('-') {
        Some((year, month)) => (
            component::<Year>(year, at)?,
            month,
            at.saturating_add(year.len()).saturating_add(1),
        ),
        None => (Component::Any, head, at),
    };
    let month = component::<Month>(month, month_at)?;
    let day = if !from_end {
        DaySpec::FromStart(component::<Day>(day, day_at)?)
    } else if day == "*" {
        DaySpec::FromStart(Component::Any)
    } else {
        DaySpec::from_end(items::<DayFromEnd>(day, day_at)?)
            .map_err(|kind| ParseError::new(kind, day_at))?
    };
    Ok(DateSpec { year, month, day })
}

/// `hour:minute[:second]`. A missing second is 0.
fn time_spec(word: &str, at: usize) -> Result<TimeSpec, ParseError> {
    let mut parts = pieces(word, ':').map(|(offset, part)| (at.saturating_add(offset), part));
    let (hour_at, hour) = parts.next().unwrap_or((at, ""));
    let Some((minute_at, minute)) = parts.next() else {
        return Err(ParseError::new(ErrorKind::Syntax, at));
    };
    let second = match parts.next() {
        Some((second_at, second)) => component::<Second>(second, second_at)?,
        None => zero(at)?,
    };
    if let Some((extra_at, _)) = parts.next() {
        return Err(ParseError::new(
            ErrorKind::Syntax,
            extra_at.saturating_sub(1),
        ));
    }
    Ok(TimeSpec {
        hour: component::<Hour>(hour, hour_at)?,
        minute: component::<Minute>(minute, minute_at)?,
        second,
    })
}

/// How the numbers of one field are spelled.
trait Grammar: FieldSpec {
    /// A value or a repetition, in the field's unit.
    fn number(text: &str) -> Result<u32, ErrorKind> {
        integer(text, Self::FIELD)
    }

    /// Turns a spelled value into the stored value. Repetitions are not adjusted.
    fn adjust(value: u32) -> u32 {
        value
    }
}

impl Grammar for Year {
    /// Two-digit years by value: 0 to 69 are 20xx, 70 to 99 are 19xx.
    fn adjust(value: u32) -> u32 {
        match value {
            0..70 => value.saturating_add(2000),
            70..100 => value.saturating_add(1900),
            _ => value,
        }
    }
}

impl Grammar for Month {}
impl Grammar for Day {}
impl Grammar for DayFromEnd {}
impl Grammar for Hour {}
impl Grammar for Minute {}

impl Grammar for Second {
    /// Whole seconds with an optional fraction, rounded to microseconds.
    fn number(text: &str) -> Result<u32, ErrorKind> {
        let out_of_range = ErrorKind::OutOfRange(Self::FIELD);
        let (whole, fraction) = match text.split_once('.') {
            Some((whole, fraction)) => (whole, Some(fraction)),
            None => (text, None),
        };
        let whole = integer(whole, Self::FIELD)?;
        let micros = match fraction {
            Some(fraction) => round_fraction(fraction)?,
            None => 0,
        };
        whole
            .checked_mul(MICROS)
            .and_then(|w| w.checked_add(micros))
            .filter(|micros| *micros <= MAX_NUMBER)
            .ok_or(out_of_range)
    }
}

/// Six digits of microseconds, rounded half up by the seventh digit. Up to 1,000,000.
fn round_fraction(fraction: &str) -> Result<u32, ErrorKind> {
    if fraction.is_empty() || !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ErrorKind::Number);
    }
    let mut digits = fraction.bytes().map(|b| u32::from(b.saturating_sub(b'0')));
    let mut micros = 0_u32;
    for _ in 0..6 {
        let digit = digits.next().unwrap_or(0);
        micros = micros.saturating_mul(10).saturating_add(digit);
    }
    if digits.next().is_some_and(|digit| digit >= 5) {
        micros = micros.saturating_add(1);
    }
    Ok(micros)
}

/// Decimal digits only. A value above [`MAX_NUMBER`] is out of range.
fn integer(text: &str, field: crate::error::Field) -> Result<u32, ErrorKind> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ErrorKind::Number);
    }
    text.bytes().try_fold(0_u32, |value, b| {
        value
            .checked_mul(10)
            .and_then(|v| v.checked_add(u32::from(b.saturating_sub(b'0'))))
            .filter(|v| *v <= MAX_NUMBER)
            .ok_or(ErrorKind::OutOfRange(field))
    })
}

/// `*` alone, or a list.
fn component<F: Grammar>(text: &str, at: usize) -> Result<Component<F>, ParseError> {
    if text == "*" {
        return Ok(Component::Any);
    }
    items::<F>(text, at).map(Component::from_items)
}

/// A comma list of `v`, `v/r`, `a..b` and `a..b/r`. Stops at item 242.
fn items<F: Grammar>(text: &str, at: usize) -> Result<Items<F>, ParseError> {
    let mut list = Vec::new();
    for (offset, piece) in pieces(text, ',') {
        let item_at = at.saturating_add(offset);
        if list.len() >= MAX_ITEMS {
            return Err(ParseError::new(ErrorKind::TooManyItems(F::FIELD), item_at));
        }
        list.push(item::<F>(piece).map_err(|kind| ParseError::new(kind, item_at))?);
    }
    Items::new(list).map_err(|kind| ParseError::new(kind, at))
}

fn item<F: Grammar>(text: &str) -> Result<Item<F>, ErrorKind> {
    let (values, step) = match text.split_once('/') {
        Some((values, step)) => (values, Some(F::number(step)?)),
        None => (text, None),
    };
    let value = |text: &str| F::number(text).map(F::adjust);
    match (values.split_once(".."), step) {
        (Some((start, stop)), step) => Item::range(value(start)?, value(stop)?, step),
        (None, Some(step)) => Item::every(value(values)?, step),
        (None, None) => Item::value(value(values)?),
    }
}
