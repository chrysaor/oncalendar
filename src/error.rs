use core::fmt;

/// Why an expression was rejected, and where.
///
/// The message is a fixed phrase plus a byte offset. It never contains any part of the input, so
/// it is safe to write to a log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    offset: usize,
}

impl ParseError {
    pub(crate) fn new(offset: usize) -> Self {
        Self { offset }
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
        write!(
            f,
            "invalid calendar expression at byte {}: parsing is not implemented yet",
            self.offset
        )
    }
}

impl std::error::Error for ParseError {}
