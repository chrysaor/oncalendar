use core::fmt::{self, Write as _};

use crate::error::Field;
use crate::model::{
    CalendarEvent, Component, DaySpec, FieldSpec, Item, Items, MICROS, Value, WeekdaySet, Zone,
};

const WEEKDAY_NAMES: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

impl fmt::Display for CalendarEvent {
    /// Writes the normalized form, which parses back to an equal value.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.weekdays != WeekdaySet::ALL {
            write_weekdays(f, self.weekdays)?;
            f.write_char(' ')?;
        }
        write_component(f, &self.date.year)?;
        f.write_char('-')?;
        write_component(f, &self.date.month)?;
        match &self.date.day {
            DaySpec::FromStart(day) => {
                f.write_char('-')?;
                write_component(f, day)?;
            }
            DaySpec::FromEnd(items) => {
                f.write_char('~')?;
                write_items(f, items)?;
            }
        }
        f.write_char(' ')?;
        write_component(f, &self.time.hour)?;
        f.write_char(':')?;
        write_component(f, &self.time.minute)?;
        f.write_char(':')?;
        write_component(f, &self.time.second)?;
        match &self.zone {
            Zone::Local => Ok(()),
            Zone::Utc => f.write_str(" UTC"),
            Zone::Named(name) => write!(f, " {name}"),
        }
    }
}

/// Runs of three or more days print as `A..B`, other days as a comma list.
fn write_weekdays(f: &mut fmt::Formatter<'_>, days: WeekdaySet) -> fmt::Result {
    let mut first = true;
    let mut day = 0_u32;
    while day < 7 {
        if !days.contains(day) {
            day = day.saturating_add(1);
            continue;
        }
        let mut end = day;
        while end < 6 && days.contains(end.saturating_add(1)) {
            end = end.saturating_add(1);
        }
        if !first {
            f.write_char(',')?;
        }
        first = false;
        if end.saturating_sub(day) >= 2 {
            write!(f, "{}..{}", weekday_name(day), weekday_name(end))?;
            day = end.saturating_add(1);
        } else {
            f.write_str(weekday_name(day))?;
            day = day.saturating_add(1);
        }
    }
    Ok(())
}

fn weekday_name(day: u32) -> &'static str {
    usize::try_from(day)
        .ok()
        .and_then(|i| WEEKDAY_NAMES.get(i))
        .copied()
        .unwrap_or("")
}

fn write_value<F: FieldSpec>(f: &mut fmt::Formatter<'_>, value: u32) -> fmt::Result {
    match F::FIELD {
        Field::Year => write!(f, "{value:04}"),
        Field::Second => write_seconds(f, value, true),
        _ => write!(f, "{value:02}"),
    }
}

fn write_step<F: FieldSpec>(f: &mut fmt::Formatter<'_>, step: u32) -> fmt::Result {
    match F::FIELD {
        Field::Second => write_seconds(f, step, false),
        _ => write!(f, "{step}"),
    }
}

/// Whole seconds, then `.` and six digits when the microsecond part is not zero.
fn write_seconds(f: &mut fmt::Formatter<'_>, micros: u32, pad: bool) -> fmt::Result {
    let whole = micros.checked_div(MICROS).unwrap_or(0);
    let fraction = micros.checked_rem(MICROS).unwrap_or(0);
    if pad {
        write!(f, "{whole:02}")?;
    } else {
        write!(f, "{whole}")?;
    }
    if fraction != 0 {
        write!(f, ".{fraction:06}")?;
    }
    Ok(())
}

fn write_component<F: FieldSpec>(
    f: &mut fmt::Formatter<'_>,
    component: &Component<F>,
) -> fmt::Result {
    match component {
        Component::Any => f.write_char('*'),
        Component::List(items) => write_items(f, items),
    }
}

fn write_items<F: FieldSpec>(f: &mut fmt::Formatter<'_>, items: &Items<F>) -> fmt::Result {
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            f.write_char(',')?;
        }
        write_item(f, item)?;
    }
    Ok(())
}

fn write_item<F: FieldSpec>(f: &mut fmt::Formatter<'_>, item: &Item<F>) -> fmt::Result {
    let value = |f: &mut fmt::Formatter<'_>, v: Value<F>| write_value::<F>(f, v.get());
    match *item {
        Item::Value(v) => value(f, v),
        Item::Every { start, step } => {
            value(f, start)?;
            f.write_char('/')?;
            write_step::<F>(f, step.get())
        }
        Item::Range { start, stop } => {
            value(f, start)?;
            f.write_str("..")?;
            value(f, stop)
        }
        Item::Stepped { start, stop, step } => {
            value(f, start)?;
            f.write_str("..")?;
            value(f, stop)?;
            f.write_char('/')?;
            write_step::<F>(f, step.get())
        }
    }
}
