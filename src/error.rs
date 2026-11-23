use core::fmt;

/// The part of an expression an error refers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Field {
    Year,
    Month,
    Day,
    Hour,
    Minute,
    Second,
}

impl Field {
    fn name(self) -> &'static str {
        match self {
            Field::Year => "year",
            Field::Month => "month",
            Field::Day => "day",
            Field::Hour => "hour",
            Field::Minute => "minute",
            Field::Second => "second",
        }
    }
}

/// What went wrong. Each kind has a fixed message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ErrorKind {
    Empty,
    Whitespace,
    Weekday,
    Syntax,
    Number,
    OutOfRange(Field),
    Repetition(Field),
    Range(Field),
    TooManyItems(Field),
    DayList,
}

/// Why an expression was rejected, and where.
///
/// The message is a fixed phrase plus a byte offset. It never contains any part of the input, so
/// it is safe to write to a log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    kind: ErrorKind,
    offset: usize,
}

impl ParseError {
    pub(crate) fn new(kind: ErrorKind, offset: usize) -> Self {
        Self { kind, offset }
    }

    /// The byte offset in the input where the problem starts.
    ///
    /// It is never greater than the input length and always lies on a character boundary.
    pub fn offset(&self) -> usize {
        self.offset
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("calendar expression: ")?;
        match self.kind {
            ErrorKind::Empty => f.write_str("empty expression")?,
            ErrorKind::Whitespace => f.write_str("misplaced whitespace")?,
            ErrorKind::Weekday => f.write_str("invalid weekday")?,
            ErrorKind::Syntax => f.write_str("unexpected character")?,
            ErrorKind::Number => f.write_str("invalid number")?,
            ErrorKind::OutOfRange(field) => write!(f, "{} out of range", field.name())?,
            ErrorKind::Repetition(field) => write!(f, "invalid {} repetition", field.name())?,
            ErrorKind::Range(field) => write!(f, "invalid {} range", field.name())?,
            ErrorKind::TooManyItems(field) => write!(f, "too many {} items", field.name())?,
            ErrorKind::DayList => f.write_str("unsupported list of days from the month's end")?,
        }
        write!(f, " at byte {}", self.offset)
    }
}

impl core::error::Error for ParseError {}
