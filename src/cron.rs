use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

use crate::error::ErrorKind;
use crate::model::{
    CalendarEvent, Component, DateSpec, Day, DaySpec, FieldSpec, Hour, Item, Items, MAX_ITEMS,
    Minute, Month, Second, TimeSpec, WeekdaySet, Year, Zone,
};

/// The longest cron expression [`CalendarEvent::from_cron`] reads, in bytes.
const MAX_LEN: usize = 1024;

const MONTH_NAMES: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];

const WEEKDAY_NAMES: [&str; 7] = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];

const SPECIAL_STRINGS: [(&str, &str); 7] = [
    ("@yearly", "0 0 1 1 *"),
    ("@annually", "0 0 1 1 *"),
    ("@monthly", "0 0 1 * *"),
    ("@weekly", "0 0 * * 0"),
    ("@daily", "0 0 * * *"),
    ("@midnight", "0 0 * * *"),
    ("@hourly", "0 * * * *"),
];

/// One of the five fields of a cron expression, in input order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CronField {
    Minute,
    Hour,
    DayOfMonth,
    Month,
    DayOfWeek,
}

impl CronField {
    fn name(self) -> &'static str {
        match self {
            CronField::Minute => "minute",
            CronField::Hour => "hour",
            CronField::DayOfMonth => "day of month",
            CronField::Month => "month",
            CronField::DayOfWeek => "day of week",
        }
    }

    /// Smallest value, largest value, and the names that spell values from the smallest one.
    fn spec(self) -> (u32, u32, &'static [&'static str]) {
        match self {
            CronField::Minute => (0, 59, &[]),
            CronField::Hour => (0, 23, &[]),
            CronField::DayOfMonth => (1, 31, &[]),
            CronField::Month => (1, 12, &MONTH_NAMES),
            CronField::DayOfWeek => (0, 7, &WEEKDAY_NAMES),
        }
    }
}

/// What went wrong. Each kind has a fixed message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CronErrorKind {
    Empty,
    TooLong,
    FieldCount,
    SpecialString,
    Extension,
    Syntax,
    Number,
    Name,
    StepWithoutRange,
    OutOfRange(CronField),
    Range(CronField),
    Step(CronField),
    TooManyItems(CronField),
}

/// Why a cron expression was rejected, and where.
///
/// The message is a fixed phrase plus a byte offset. It never contains any part of the input, so
/// it is safe to write to a log.
///
/// ```
/// use oncalendar::CalendarEvent;
///
/// let error = CalendarEvent::from_cron("0 0 L * *").unwrap_err();
/// assert_eq!(
///     error.to_string(),
///     "cron expression: unsupported extension (L, W, # or ?) at byte 4",
/// );
/// assert_eq!(error.offset(), 4);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CronError {
    kind: CronErrorKind,
    offset: usize,
}

impl CronError {
    fn new(kind: CronErrorKind, offset: usize) -> Self {
        Self { kind, offset }
    }

    /// The byte offset in the input where the problem starts.
    ///
    /// It is never greater than the input length and always lies on a character boundary.
    pub fn offset(&self) -> usize {
        self.offset
    }
}

impl fmt::Display for CronError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("cron expression: ")?;
        match self.kind {
            CronErrorKind::Empty => f.write_str("empty expression")?,
            CronErrorKind::TooLong => write!(f, "longer than {MAX_LEN} bytes")?,
            CronErrorKind::FieldCount => {
                f.write_str("expected 5 fields: minute, hour, day of month, month, day of week")?;
            }
            CronErrorKind::SpecialString => f.write_str("unknown or unsupported special string")?,
            CronErrorKind::Extension => f.write_str("unsupported extension (L, W, # or ?)")?,
            CronErrorKind::Syntax => f.write_str("unexpected character")?,
            CronErrorKind::Number => f.write_str("invalid number")?,
            CronErrorKind::Name => f.write_str("unknown month or weekday name")?,
            CronErrorKind::StepWithoutRange => {
                f.write_str("a step needs * or a range before it")?
            }
            CronErrorKind::OutOfRange(field) => write!(f, "{} out of range", field.name())?,
            CronErrorKind::Range(field) => write!(f, "invalid {} range", field.name())?,
            CronErrorKind::Step(field) => write!(f, "invalid {} step", field.name())?,
            CronErrorKind::TooManyItems(field) => write!(f, "too many {} items", field.name())?,
        }
        write!(f, " at byte {}", self.offset)
    }
}

impl core::error::Error for CronError {}

/// `start`, `start + step`, ... up to `stop`, all inside the field. `step` is at least 1.
#[derive(Clone, Copy, Debug)]
struct Progression {
    start: u32,
    stop: u32,
    step: u32,
}

/// One parsed field. `Star` is `*` or `*/step`, which cron treats as unrestricted.
#[derive(Clone, Debug)]
enum Pattern {
    Star { step: u32 },
    List(Vec<Progression>),
}

impl Pattern {
    fn progressions(&self, field: CronField) -> Vec<Progression> {
        let (min, max, _) = field.spec();
        match self {
            Pattern::Star { step } => vec![Progression {
                start: min,
                stop: max,
                step: *step,
            }],
            Pattern::List(items) => items.clone(),
        }
    }
}

impl CalendarEvent {
    /// Converts a standard five-field cron schedule into the calendar events that elapse at the
    /// same minutes.
    ///
    /// The input is the time part of a crontab line: minute, hour, day of month, month, and day
    /// of week, separated by spaces or tabs, or one of `@yearly`, `@annually`, `@monthly`,
    /// `@weekly`, `@daily`, `@midnight`, and `@hourly`. Fields take `*`, values, ranges `a-b`,
    /// lists, and steps after `*` or a range (`*/15`, `10-50/15`). Months and weekdays also
    /// take three-letter English names in any case, and 0 and 7 are both Sunday.
    ///
    /// The result holds one event, or two when both the day of month and the day of week are
    /// restricted. cron then runs on a day that matches either field, and a calendar event
    /// matches only days that match both, so the result has one event for each field. Write
    /// one `OnCalendar=` line per event. The events name no time zone, so they elapse in the
    /// local time of the timer, by systemd's daylight saving time rules.
    ///
    /// ```
    /// use oncalendar::CalendarEvent;
    ///
    /// let events = CalendarEvent::from_cron("*/15 9-17 * * mon-fri")?;
    /// let lines: Vec<String> = events.iter().map(ToString::to_string).collect();
    /// assert_eq!(lines, ["Mon..Fri *-*-* 09..17:00/15:00"]);
    ///
    /// let events = CalendarEvent::from_cron("0 9 1 * 1")?;
    /// let lines: Vec<String> = events.iter().map(ToString::to_string).collect();
    /// assert_eq!(lines, ["*-*-01 09:00:00", "Mon *-*-* 09:00:00"]);
    /// # Ok::<(), oncalendar::CronError>(())
    /// ```
    ///
    /// # Errors
    ///
    /// [`CronError`] when the input is not a five-field cron schedule. Seconds and year
    /// fields, `L`, `W`, `#`, `?`, `@reboot`, and a step after a single value such as `5/15`
    /// are rejected, and so is an input longer than 1024 bytes.
    pub fn from_cron(input: &str) -> Result<Vec<CalendarEvent>, CronError> {
        if input.len() > MAX_LEN {
            let at = (0..=MAX_LEN)
                .rev()
                .find(|at| input.is_char_boundary(*at))
                .unwrap_or(0);
            return Err(CronError::new(CronErrorKind::TooLong, at));
        }
        let words: Vec<(usize, &str)> = words(input).collect();
        match words.as_slice() {
            [] => Err(CronError::new(CronErrorKind::Empty, 0)),
            [(at, word), rest @ ..] if word.starts_with('@') => {
                let (_, expansion) = SPECIAL_STRINGS
                    .iter()
                    .find(|(name, _)| word.eq_ignore_ascii_case(name))
                    .ok_or(CronError::new(CronErrorKind::SpecialString, *at))?;
                match rest.first() {
                    Some((extra_at, _)) => Err(CronError::new(CronErrorKind::Syntax, *extra_at)),
                    None => CalendarEvent::from_cron(expansion),
                }
            }
            [minute, hour, day, month, weekday] => {
                let field = |(at, word): &(usize, &str), field| {
                    pattern(word, *at, field).map(|pattern| (pattern, *at))
                };
                events([
                    field(minute, CronField::Minute)?,
                    field(hour, CronField::Hour)?,
                    field(day, CronField::DayOfMonth)?,
                    field(month, CronField::Month)?,
                    field(weekday, CronField::DayOfWeek)?,
                ])
            }
            words => {
                let at = words.get(5).map_or(input.len(), |(at, _)| *at);
                Err(CronError::new(CronErrorKind::FieldCount, at))
            }
        }
    }
}

/// The words of `text` with their byte offsets. Runs of spaces and tabs separate words.
fn words(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.split([' ', '\t'])
        .scan(0_usize, |next, piece| {
            let at = *next;
            *next = next.saturating_add(piece.len()).saturating_add(1);
            Some((at, piece))
        })
        .filter(|(_, word)| !word.is_empty())
}

fn pattern(word: &str, at: usize, field: CronField) -> Result<Pattern, CronError> {
    if word == "*" {
        return Ok(Pattern::Star { step: 1 });
    }
    if let Some(step) = word.strip_prefix("*/") {
        let step = parse_step(step, field).map_err(|kind| CronError::new(kind, at))?;
        return Ok(Pattern::Star { step });
    }
    let mut items = Vec::new();
    let mut offset = 0_usize;
    for piece in word.split(',') {
        let item_at = at.saturating_add(offset);
        offset = offset.saturating_add(piece.len()).saturating_add(1);
        if items.len() >= MAX_ITEMS {
            return Err(CronError::new(CronErrorKind::TooManyItems(field), item_at));
        }
        items.push(progression(piece, field).map_err(|kind| CronError::new(kind, item_at))?);
    }
    Ok(Pattern::List(items))
}

/// `v`, `a-b`, or `a-b/step`.
fn progression(text: &str, field: CronField) -> Result<Progression, CronErrorKind> {
    if is_extension(text) {
        return Err(CronErrorKind::Extension);
    }
    let (values, step) = match text.split_once('/') {
        Some((values, step)) => (values, Some(parse_step(step, field)?)),
        None => (text, None),
    };
    match (values.split_once('-'), step) {
        (Some((start, stop)), step) => {
            let start = value(start, field)?;
            let stop = value(stop, field)?;
            if stop < start {
                return Err(CronErrorKind::Range(field));
            }
            Ok(Progression {
                start,
                stop,
                step: step.unwrap_or(1),
            })
        }
        (None, Some(_)) => Err(CronErrorKind::StepWithoutRange),
        (None, None) => {
            let v = value(values, field)?;
            Ok(Progression {
                start: v,
                stop: v,
                step: 1,
            })
        }
    }
}

/// `L`, `W`, `#` and `?` from other cron dialects: `L`, `5L`, `15W`, `LW`, `L-3`, `5#3`, `?`.
fn is_extension(text: &str) -> bool {
    let rest = text.trim_start_matches(|c: char| c.is_ascii_digit());
    text.contains(['#', '?'])
        || ["L", "W", "LW"]
            .iter()
            .any(|suffix| rest.eq_ignore_ascii_case(suffix))
        || text.starts_with("L-")
        || text.starts_with("l-")
}

/// A number or a name, inside the field.
fn value(text: &str, field: CronField) -> Result<u32, CronErrorKind> {
    let (min, max, names) = field.spec();
    if text.starts_with(|c: char| c.is_ascii_alphabetic()) {
        let index = names
            .iter()
            .position(|name| text.eq_ignore_ascii_case(name))
            .ok_or(CronErrorKind::Name)?;
        return u32::try_from(index)
            .ok()
            .and_then(|index| index.checked_add(min))
            .ok_or(CronErrorKind::Name);
    }
    let v = number(text)?;
    if (min..=max).contains(&v) {
        Ok(v)
    } else {
        Err(CronErrorKind::OutOfRange(field))
    }
}

/// A step from 1 to the largest value of the field.
fn parse_step(text: &str, field: CronField) -> Result<u32, CronErrorKind> {
    let (_, max, _) = field.spec();
    let step = number(text)?;
    if (1..=max).contains(&step) {
        Ok(step)
    } else {
        Err(CronErrorKind::Step(field))
    }
}

/// Decimal digits only. Every value above `u32::MAX` is out of every field.
fn number(text: &str) -> Result<u32, CronErrorKind> {
    if text.is_empty() {
        return Err(CronErrorKind::Number);
    }
    if !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(CronErrorKind::Syntax);
    }
    Ok(text.bytes().fold(0_u32, |v, b| {
        v.saturating_mul(10)
            .saturating_add(u32::from(b.saturating_sub(b'0')))
    }))
}

/// One event, or two when both day fields are restricted.
fn events(
    [minute, hour, day, month, (weekday, _)]: [(Pattern, usize); 5],
) -> Result<Vec<CalendarEvent>, CronError> {
    let time = TimeSpec {
        hour: component::<Hour>(&hour, CronField::Hour, true)?,
        minute: component::<Minute>(&minute, CronField::Minute, true)?,
        second: Component::<Second>::value(0)
            .map_err(|_| CronError::new(CronErrorKind::OutOfRange(CronField::Minute), 0))?,
    };
    let month = component::<Month>(&month, CronField::Month, false)?;
    let days = component::<Day>(&day, CronField::DayOfMonth, false)?;
    let weekdays = weekday_set(&weekday);
    let both_restricted = matches!(day.0, Pattern::List(_)) && matches!(weekday, Pattern::List(_));
    let event = |weekdays, day| CalendarEvent {
        weekdays,
        date: DateSpec {
            year: Component::<Year>::Any,
            month: month.clone(),
            day: DaySpec::FromStart(day),
        },
        time: time.clone(),
        zone: Zone::Local,
    };
    Ok(if both_restricted {
        vec![
            event(WeekdaySet::ALL, days),
            event(weekdays, Component::Any),
        ]
    } else {
        vec![event(weekdays, days)]
    })
}

/// The component of one field. `floor` is set for the hour and the minute, where a `v/r` item
/// would restart after the end of the field at a value other than `v` unless `r` divides the
/// rest of the field.
fn component<F: FieldSpec>(
    (pattern, at): &(Pattern, usize),
    field: CronField,
    floor: bool,
) -> Result<Component<F>, CronError> {
    if matches!(pattern, Pattern::Star { step: 1 }) {
        return Ok(Component::Any);
    }
    pattern
        .progressions(field)
        .into_iter()
        .map(|p| item::<F>(p, floor))
        .collect::<Result<Vec<_>, _>>()
        .and_then(Items::new)
        .map(Component::from_items)
        .map_err(|_: ErrorKind| CronError::new(CronErrorKind::OutOfRange(field), *at))
}

fn item<F: FieldSpec>(p: Progression, floor: bool) -> Result<Item<F>, ErrorKind> {
    if p.step == 1 || p.start == p.stop {
        return Item::range(p.start, p.stop, None);
    }
    let restarts_at_start = !floor
        || F::MAX
            .checked_add(1)
            .and_then(|length| length.checked_sub(p.start))
            .and_then(|rest| rest.checked_rem(p.step))
            == Some(0);
    if p.stop == F::MAX && restarts_at_start {
        if let Ok(every) = Item::every(p.start, p.step) {
            return Ok(every);
        }
    }
    Item::range(p.start, p.stop, Some(p.step))
}

/// The weekdays of the day of week field. cron counts from Sunday, 0 and 7 both Sunday.
fn weekday_set(pattern: &Pattern) -> WeekdaySet {
    let mut bits = 0_u8;
    for p in pattern.progressions(CronField::DayOfWeek) {
        let mut day = Some(p.start);
        while let Some(d) = day.filter(|d| *d <= p.stop) {
            let monday_zero = d.checked_add(6).and_then(|d| d.checked_rem(7)).unwrap_or(0);
            bits |= 1_u8.checked_shl(monday_zero).unwrap_or(0);
            day = d.checked_add(p.step);
        }
    }
    WeekdaySet::from_bits(bits).unwrap_or(WeekdaySet::ALL)
}
