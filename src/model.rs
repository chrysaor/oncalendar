use core::fmt::Debug;
use core::hash::Hash;
use core::marker::PhantomData;
use core::num::{NonZeroU8, NonZeroU32};

use crate::error::{ErrorKind, Field};

/// The most items one numeric field may hold, counted before duplicates are removed.
pub(crate) const MAX_ITEMS: usize = 241;

/// Range, unit and error label of one field. Implemented by zero-sized marker types.
pub(crate) trait FieldSpec: Copy + Ord + Hash + Debug {
    const MIN: u32;
    const MAX: u32;
    const FIELD: Field;
    /// Distance between neighboring values. A range without a repetition steps by this.
    const UNIT: u32 = 1;
    /// `v/r` counts down toward `MIN` instead of up toward `MAX`.
    const DESCENDING: bool = false;
    /// A range without a repetition must reach at least two values.
    const PLAIN_RANGE_NEEDS_TWO: bool = false;
    /// The item `MIN/UNIT` turns the whole component into `*`.
    const EVERY_UNIT_IS_ANY: bool = false;
}

/// Years 1970 to 2199.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Year;
/// Months 1 to 12.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Month;
/// Days of the month 1 to 31, counted from the start.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Day;
/// Days 1 to 28 counted from the end of the month. 1 is the last day.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct DayFromEnd;
/// Hours 0 to 23.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Hour;
/// Minutes 0 to 59.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Minute;
/// Seconds in whole microseconds, 0 to 59.999999 s.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Second;

/// Microseconds in one second.
pub(crate) const MICROS: u32 = 1_000_000;

impl FieldSpec for Year {
    const MIN: u32 = 1970;
    const MAX: u32 = 2199;
    const FIELD: Field = Field::Year;
}

impl FieldSpec for Month {
    const MIN: u32 = 1;
    const MAX: u32 = 12;
    const FIELD: Field = Field::Month;
}

impl FieldSpec for Day {
    const MIN: u32 = 1;
    const MAX: u32 = 31;
    const FIELD: Field = Field::Day;
}

impl FieldSpec for DayFromEnd {
    const MIN: u32 = 1;
    const MAX: u32 = 28;
    const FIELD: Field = Field::Day;
    const DESCENDING: bool = true;
}

impl FieldSpec for Hour {
    const MIN: u32 = 0;
    const MAX: u32 = 23;
    const FIELD: Field = Field::Hour;
}

impl FieldSpec for Minute {
    const MIN: u32 = 0;
    const MAX: u32 = 59;
    const FIELD: Field = Field::Minute;
}

impl FieldSpec for Second {
    const MIN: u32 = 0;
    const MAX: u32 = 59_999_999;
    const FIELD: Field = Field::Second;
    const UNIT: u32 = MICROS;
    const PLAIN_RANGE_NEEDS_TWO: bool = true;
    const EVERY_UNIT_IS_ANY: bool = true;
}

/// A value inside the range of field `F`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Value<F: FieldSpec>(u32, PhantomData<F>);

impl<F: FieldSpec> Value<F> {
    pub(crate) fn new(value: u32) -> Result<Self, ErrorKind> {
        if (F::MIN..=F::MAX).contains(&value) {
            Ok(Self(value, PhantomData))
        } else {
            Err(ErrorKind::OutOfRange(F::FIELD))
        }
    }

    pub(crate) fn get(self) -> u32 {
        self.0
    }
}

/// A repetition of field `F`, never zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Step<F: FieldSpec>(NonZeroU32, PhantomData<F>);

impl<F: FieldSpec> Step<F> {
    fn new(step: u32) -> Result<Self, ErrorKind> {
        NonZeroU32::new(step)
            .map(|step| Self(step, PhantomData))
            .ok_or(ErrorKind::Repetition(F::FIELD))
    }

    pub(crate) fn get(self) -> u32 {
        self.0.get()
    }
}

/// One list item. Built only by the constructors, which keep it normalized.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Item<F: FieldSpec> {
    /// `v`
    Value(Value<F>),
    /// `v/r`. Reaches at least two values.
    Every { start: Value<F>, step: Step<F> },
    /// `a..b` with `a < b`, stepping by `F::UNIT`.
    Range { start: Value<F>, stop: Value<F> },
    /// `a..b/r` with `a < b`, `b - a` a multiple of `r`, and `r` not `F::UNIT`.
    Stepped {
        start: Value<F>,
        stop: Value<F>,
        step: Step<F>,
    },
}

impl<F: FieldSpec> Item<F> {
    pub(crate) fn value(value: u32) -> Result<Self, ErrorKind> {
        Value::new(value).map(Item::Value)
    }

    /// `start/step`. It must reach a second value inside the field.
    pub(crate) fn every(start: u32, step: u32) -> Result<Self, ErrorKind> {
        let first = Value::new(start)?;
        let repetition = Step::new(step)?;
        let second = if F::DESCENDING {
            start.checked_sub(step).filter(|v| *v >= F::MIN)
        } else {
            start.checked_add(step).filter(|v| *v <= F::MAX)
        };
        match second {
            Some(_) => Ok(Item::Every {
                start: first,
                step: repetition,
            }),
            None => Err(ErrorKind::Repetition(F::FIELD)),
        }
    }

    /// `start..stop`, or `start..stop/step`. The end shrinks to the last value reached, and only
    /// the shrunk end must lie inside the field.
    pub(crate) fn range(start: u32, stop: u32, step: Option<u32>) -> Result<Self, ErrorKind> {
        let first = Value::new(start)?;
        let distance = stop.checked_sub(start).ok_or(ErrorKind::Range(F::FIELD))?;
        if step.is_none() && F::PLAIN_RANGE_NEEDS_TWO && distance < F::UNIT {
            return Err(ErrorKind::Range(F::FIELD));
        }
        let repetition = Step::new(step.unwrap_or(F::UNIT))?;
        let reach = distance
            .checked_div(repetition.get())
            .and_then(|n| n.checked_mul(repetition.get()))
            .and_then(|span| start.checked_add(span))
            .ok_or(ErrorKind::Range(F::FIELD))?;
        let last = Value::new(reach)?;
        Ok(if reach == start {
            Item::Value(first)
        } else if repetition.get() == F::UNIT {
            Item::Range {
                start: first,
                stop: last,
            }
        } else {
            Item::Stepped {
                start: first,
                stop: last,
                step: repetition,
            }
        })
    }

    /// Sort key: start, then end with a missing end first, then step with a missing step first.
    fn key(&self) -> (u32, Option<u32>, Option<u32>) {
        match *self {
            Item::Value(v) => (v.get(), None, None),
            Item::Every { start, step } => (start.get(), None, Some(step.get())),
            Item::Range { start, stop } => (start.get(), Some(stop.get()), None),
            Item::Stepped { start, stop, step } => {
                (start.get(), Some(stop.get()), Some(step.get()))
            }
        }
    }

    /// First value, last value, and step, as an arithmetic progression in the field's direction.
    #[cfg(feature = "jiff")]
    fn progression(&self) -> (u32, u32, u32) {
        match *self {
            Item::Value(v) => (v.get(), v.get(), F::UNIT),
            Item::Every { start, step } => {
                let end = if F::DESCENDING { F::MIN } else { F::MAX };
                (start.get(), end, step.get())
            }
            Item::Range { start, stop } => (start.get(), stop.get(), F::UNIT),
            Item::Stepped { start, stop, step } => (start.get(), stop.get(), step.get()),
        }
    }

    /// Calls `f` with each value of the item. Only for fields with at most 31 values.
    #[cfg(feature = "jiff")]
    fn for_each_value(&self, mut f: impl FnMut(u32)) {
        let (start, end, step) = self.progression();
        let mut value = Some(start);
        while let Some(v) = value {
            f(v);
            value = if F::DESCENDING && matches!(self, Item::Every { .. }) {
                v.checked_sub(step).filter(|next| *next >= end)
            } else {
                v.checked_add(step).filter(|next| *next <= end)
            };
        }
    }
}

/// The first value of `start, start + step, ...` up to `stop` that is at least `x`.
#[cfg(feature = "jiff")]
fn first_stepped(start: u32, stop: u32, step: u32, x: u32) -> Option<u32> {
    if x <= start {
        return (start <= stop).then_some(start);
    }
    let distance = x.checked_sub(start)?;
    let steps = distance
        .checked_add(step.checked_sub(1)?)?
        .checked_div(step)?;
    let value = start.checked_add(steps.checked_mul(step)?)?;
    (value <= stop).then_some(value)
}

impl Item<DayFromEnd> {
    /// The largest day from the end the item reaches. `v/r` counts down, so that is `v`.
    fn largest_from_end(&self) -> u32 {
        match *self {
            Item::Value(start) | Item::Every { start, .. } => start.get(),
            Item::Range { stop, .. } | Item::Stepped { stop, .. } => stop.get(),
        }
    }
}

/// Not empty, sorted, no exact duplicates, at most `MAX_ITEMS` items.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Items<F: FieldSpec>(Vec<Item<F>>);

impl<F: FieldSpec> Items<F> {
    pub(crate) fn new(mut items: Vec<Item<F>>) -> Result<Self, ErrorKind> {
        if items.len() > MAX_ITEMS {
            return Err(ErrorKind::TooManyItems(F::FIELD));
        }
        if items.is_empty() {
            return Err(ErrorKind::Syntax);
        }
        items.sort_by_key(Item::key);
        items.dedup();
        Ok(Self(items))
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &Item<F>> {
        self.0.iter()
    }
}

/// `*`, or a list of items.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Component<F: FieldSpec> {
    Any,
    List(Items<F>),
}

impl<F: FieldSpec> Component<F> {
    pub(crate) fn from_items(items: Items<F>) -> Self {
        let every_unit = items.iter().any(|item| {
            matches!(item, Item::Every { start, step }
                if start.get() == F::MIN && step.get() == F::UNIT)
        });
        if F::EVERY_UNIT_IS_ANY && every_unit {
            Component::Any
        } else {
            Component::List(items)
        }
    }

    pub(crate) fn value(value: u32) -> Result<Self, ErrorKind> {
        Ok(Component::List(Items::new(vec![Item::value(value)?])?))
    }

    /// The first value at or after `x`, or `None` when the field has no more values.
    #[cfg(feature = "jiff")]
    pub(crate) fn next_at_or_after(&self, x: u32) -> Option<u32> {
        match self {
            Component::Any => first_stepped(F::MIN, F::MAX, F::UNIT, x),
            Component::List(items) => items
                .iter()
                .filter_map(|item| {
                    let (start, stop, step) = item.progression();
                    first_stepped(start, stop, step, x)
                })
                .min(),
        }
    }

    /// For a component with no value at or after `x`: the first value at or after `x` that a
    /// `v/r` item would reach if it ran on past the end of the field, minus the length of the
    /// field. `None` when the component has no `v/r` item. Only for ascending fields that
    /// start at 0.
    #[cfg(feature = "jiff")]
    pub(crate) fn wrapped_floor(&self, x: u32) -> Option<u32> {
        let Component::List(items) = self else {
            return None;
        };
        let length = F::MAX.checked_add(1)?.checked_sub(F::MIN)?;
        items
            .iter()
            .filter_map(|item| match *item {
                Item::Every { start, step } => first_stepped(start.get(), u32::MAX, step.get(), x),
                _ => None,
            })
            .min()?
            .checked_sub(length)
    }
}

/// `-` counts days from the start of the month, `~` from the end.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum DaySpec {
    FromStart(Component<Day>),
    /// No `Any` here. `*-*~*` is `FromStart(Component::Any)`.
    FromEnd(Items<DayFromEnd>),
}

/// How much lower the largest value of each further item from the end must be.
const FROM_END_ITEM_COST: u32 = 3;

impl DaySpec {
    /// Days from the end. Item `i` of the sorted list may reach at most `28 - 3 * i`.
    pub(crate) fn from_end(items: Items<DayFromEnd>) -> Result<Self, ErrorKind> {
        for (index, item) in items.iter().enumerate() {
            let limit = u32::try_from(index)
                .ok()
                .and_then(|i| i.checked_mul(FROM_END_ITEM_COST))
                .and_then(|cost| DayFromEnd::MAX.checked_sub(cost));
            if limit.is_none_or(|limit| item.largest_from_end() > limit) {
                return Err(ErrorKind::DayList);
            }
        }
        Ok(DaySpec::FromEnd(items))
    }

    /// Bit `d - 1` is set when day `d` of a month with `len` days matches.
    #[cfg(feature = "jiff")]
    pub(crate) fn mask(&self, len: u32) -> u32 {
        let mut mask = 0_u32;
        let mut set = |day: u32| {
            if (1..=len).contains(&day) {
                mask |= 1_u32.checked_shl(day.saturating_sub(1)).unwrap_or(0);
            }
        };
        match self {
            DaySpec::FromStart(Component::Any) => (1..=len).for_each(&mut set),
            DaySpec::FromStart(Component::List(items)) => {
                items.iter().for_each(|item| item.for_each_value(&mut set));
            }
            DaySpec::FromEnd(items) => items.iter().for_each(|item| {
                item.for_each_value(|k| {
                    if let Some(day) = len.checked_add(1).and_then(|n| n.checked_sub(k)) {
                        set(day);
                    }
                });
            }),
        }
        mask
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct DateSpec {
    pub(crate) year: Component<Year>,
    pub(crate) month: Component<Month>,
    pub(crate) day: DaySpec,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TimeSpec {
    pub(crate) hour: Component<Hour>,
    pub(crate) minute: Component<Minute>,
    pub(crate) second: Component<Second>,
}

const ALL_DAYS: NonZeroU8 = match NonZeroU8::new(0x7f) {
    Some(bits) => bits,
    None => NonZeroU8::MIN,
};

/// Seven bits, Monday is bit 0. The empty set cannot be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct WeekdaySet(NonZeroU8);

impl WeekdaySet {
    /// Every day of the week: no weekday condition.
    pub(crate) const ALL: Self = Self(ALL_DAYS);

    pub(crate) fn from_bits(bits: u8) -> Option<Self> {
        NonZeroU8::new(bits & ALL_DAYS.get()).map(Self)
    }

    pub(crate) fn bits(self) -> u8 {
        self.0.get()
    }

    /// Whether the day `monday_zero` (Monday is 0) is in the set.
    pub(crate) fn contains(self, monday_zero: u32) -> bool {
        1_u32
            .checked_shl(monday_zero)
            .is_some_and(|bit| u32::from(self.bits()) & bit != 0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Zone {
    /// No time zone in the expression. The caller's local zone applies.
    Local,
    /// `UTC` in any case. Always printed as `UTC`.
    Utc,
    /// A name that passed the format check. Printed as written.
    Named(Box<str>),
}

/// A parsed calendar event expression, always held in normalized form.
///
/// Two expressions that normalize to the same text are equal. Create one with [`str::parse`],
/// print the normalized form with [`Display`](core::fmt::Display).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CalendarEvent {
    pub(crate) weekdays: WeekdaySet,
    pub(crate) date: DateSpec,
    pub(crate) time: TimeSpec,
    pub(crate) zone: Zone,
}
