use jiff::Timestamp;
use jiff::tz::{TimeZone, TimeZoneDatabase};
use oncalendar::CalendarEvent;

pub const MAX_EXPRESSION_BYTES: usize = 1 << 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Status {
    Ok = 0,
    Invalid = 1,
    Usage = 2,
    UnknownZone = 3,
    NoElapse = 4,
    Io = 5,
}

impl Status {
    pub fn and(self, other: Status) -> Status {
        match (self, other) {
            (Status::Ok, next) => next,
            (worst, Status::Ok) => worst,
            (a, b) => a.min(b),
        }
    }
}

pub struct Settings {
    pub count: usize,
    pub after: Timestamp,
    pub local: TimeZone,
    pub database: TimeZoneDatabase,
}

pub enum Outcome {
    Invalid {
        message: String,
        offset: Option<usize>,
    },
    UnknownZone {
        normalized: String,
        message: String,
        offset: usize,
    },
    Checked {
        normalized: String,
        elapses: Vec<Timestamp>,
        exhausted: bool,
    },
}

pub struct Report {
    pub line: Option<usize>,
    pub expression: String,
    pub outcome: Outcome,
}

impl Report {
    pub fn status(&self) -> Status {
        match &self.outcome {
            Outcome::Invalid { .. } => Status::Invalid,
            Outcome::UnknownZone { .. } => Status::UnknownZone,
            Outcome::Checked {
                elapses, exhausted, ..
            } if elapses.is_empty() && *exhausted => Status::NoElapse,
            Outcome::Checked { .. } => Status::Ok,
        }
    }
}

pub fn from_bytes(line: Option<usize>, mut bytes: Vec<u8>, settings: &Settings) -> Report {
    if bytes.len() > MAX_EXPRESSION_BYTES {
        bytes.truncate(MAX_EXPRESSION_BYTES);
        return Report {
            line,
            expression: String::from_utf8_lossy(&bytes).into_owned(),
            outcome: Outcome::Invalid {
                message: format!("calendar expression: longer than {MAX_EXPRESSION_BYTES} bytes"),
                offset: None,
            },
        };
    }
    match String::from_utf8(bytes) {
        Ok(expression) => check(line, expression, settings),
        Err(error) => Report {
            line,
            expression: String::from_utf8_lossy(error.as_bytes()).into_owned(),
            outcome: Outcome::Invalid {
                message: format!(
                    "calendar expression: not valid UTF-8 at byte {}",
                    error.utf8_error().valid_up_to()
                ),
                offset: None,
            },
        },
    }
}

fn check(line: Option<usize>, expression: String, settings: &Settings) -> Report {
    let outcome = outcome(&expression, settings);
    Report {
        line,
        expression,
        outcome,
    }
}

fn outcome(expression: &str, settings: &Settings) -> Outcome {
    let event = match expression.parse::<CalendarEvent>() {
        Ok(event) => event,
        Err(error) => {
            return Outcome::Invalid {
                message: error.to_string(),
                offset: Some(error.offset()),
            };
        }
    };
    let normalized = event.to_string();
    match event.upcoming_with(settings.after, &settings.local, &settings.database) {
        Err(error) => Outcome::UnknownZone {
            normalized,
            message: error.to_string(),
            offset: expression
                .rfind(' ')
                .map_or(0, |space| space.saturating_add(1)),
        },
        Ok(upcoming) => {
            let elapses: Vec<Timestamp> = upcoming.take(settings.count).collect();
            let exhausted = elapses.len() < settings.count;
            Outcome::Checked {
                normalized,
                elapses,
                exhausted,
            }
        }
    }
}
