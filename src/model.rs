use core::convert::Infallible;

/// A parsed calendar event expression, always held in normalized form.
///
/// Two expressions that normalize to the same text are equal. Create one with [`str::parse`],
/// print the normalized form with [`Display`](core::fmt::Display).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CalendarEvent {
    /// The model does not exist yet, so no value of this type can be built.
    pub(crate) unbuilt: Infallible,
}
