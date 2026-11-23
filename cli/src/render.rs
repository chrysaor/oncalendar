use std::fmt::Write as _;

use jiff::Timestamp;
use jiff::tz::TimeZone;

use crate::report::{Outcome, Report};

const SNIPPET_WIDTH: usize = 72;

pub enum Human {
    Result(String),
    Error(String),
}

pub fn human(report: &Report, after: Timestamp, local: &TimeZone) -> Human {
    let line = report
        .line
        .map(|line| format!("line {line}: "))
        .unwrap_or_default();
    match &report.outcome {
        Outcome::Invalid { message, offset } => {
            let mut err = format!("error: {line}{message}\n");
            if let Some(offset) = offset {
                err.push_str(&snippet(&report.expression, *offset));
            }
            Human::Error(err)
        }
        Outcome::UnknownZone {
            normalized,
            message,
            offset,
        } => Human::Error(format!(
            "error: {line}{message}\n{}  parsed as {normalized}\n",
            snippet(&report.expression, *offset)
        )),
        Outcome::Checked {
            normalized,
            elapses,
            exhausted,
        } => {
            let mut out = format!("{normalized}\n");
            for &elapse in elapses {
                let _ = writeln!(
                    out,
                    "  {}  {}",
                    wall_clock(elapse, local),
                    relative(after, elapse)
                );
            }
            match (exhausted, elapses.is_empty()) {
                (true, true) => {
                    let _ = writeln!(out, "  never elapses after {}", wall_clock(after, local));
                }
                (true, false) => out.push_str("  no further elapses\n"),
                (false, _) => {}
            }
            Human::Result(out)
        }
    }
}

pub fn json(report: &Report, local: &TimeZone) -> String {
    let mut out = String::from("{");
    if let Some(line) = report.line {
        let _ = write!(out, "\"line\":{line},");
    }
    out.push_str("\"expression\":");
    string(&mut out, &report.expression);
    match &report.outcome {
        Outcome::Invalid { message, offset } => {
            out.push_str(",\"error\":\"invalid-expression\",\"message\":");
            string(&mut out, message);
            if let Some(offset) = offset {
                let _ = write!(out, ",\"offset\":{offset}");
            }
        }
        Outcome::UnknownZone {
            normalized,
            message,
            offset,
        } => {
            out.push_str(",\"normalized\":");
            string(&mut out, normalized);
            out.push_str(",\"error\":\"unknown-time-zone\",\"message\":");
            string(&mut out, message);
            let _ = write!(out, ",\"offset\":{offset}");
        }
        Outcome::Checked {
            normalized,
            elapses,
            ..
        } => {
            out.push_str(",\"normalized\":");
            string(&mut out, normalized);
            out.push_str(",\"elapses\":[");
            for (index, &elapse) in elapses.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                let _ = write!(
                    out,
                    "\"{}\"",
                    elapse.display_with_offset(local.to_offset(elapse))
                );
            }
            out.push(']');
        }
    }
    out.push_str("}\n");
    out
}

fn string(out: &mut String, text: &str) {
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0}'..='\u{1f}' | '\u{7f}' | '\u{2028}' | '\u{2029}' => {
                let _ = write!(out, "\\u{:04x}", u32::from(ch));
            }
            _ => out.push(ch),
        }
    }
    out.push('"');
}

fn wall_clock(instant: Timestamp, local: &TimeZone) -> String {
    let zoned = instant.to_zoned_datetime(local.clone());
    let abbreviation = local.to_offset_info(instant).abbreviation().to_owned();
    let format = if abbreviation.is_empty() || abbreviation.starts_with(['+', '-']) {
        "%a %Y-%m-%d %H:%M:%S%.f %:z"
    } else if abbreviation == "UTC" && zoned.offset().is_zero() {
        "%a %Y-%m-%d %H:%M:%S%.f %Z"
    } else {
        "%a %Y-%m-%d %H:%M:%S%.f %Z (%:z)"
    };
    jiff::fmt::strtime::format(format, &zoned).unwrap_or_else(|_| zoned.to_string())
}

fn relative(from: Timestamp, to: Timestamp) -> String {
    let mut rest = to
        .as_second()
        .saturating_sub(from.as_second())
        .unsigned_abs();
    let mut amounts = Vec::new();
    for (size, unit) in [(86_400, "d"), (3_600, "h"), (60, "m"), (1, "s")] {
        let amount = rest.checked_div(size).unwrap_or_default();
        rest = rest.checked_rem(size).unwrap_or_default();
        if amount > 0 || !amounts.is_empty() {
            amounts.push((amount, unit));
        }
    }
    let parts: Vec<String> = amounts
        .iter()
        .take(2)
        .filter(|(amount, _)| *amount > 0)
        .map(|(amount, unit)| format!("{amount}{unit}"))
        .collect();
    if parts.is_empty() {
        "in <1s".to_owned()
    } else {
        format!("in {}", parts.join(" "))
    }
}

fn snippet(expression: &str, offset: usize) -> String {
    let mut shown = String::new();
    let mut caret = None;
    for (index, ch) in expression.char_indices() {
        if index == offset {
            caret = Some(shown.len());
        }
        if ch == ' ' || (ch.is_ascii_graphic() && ch != '\\') {
            shown.push(ch);
        } else {
            let _ = write!(shown, "{}", ch.escape_default());
        }
    }
    let caret = caret.unwrap_or(shown.len());
    let half = SNIPPET_WIDTH / 2;
    let start = if shown.len() <= SNIPPET_WIDTH {
        0
    } else {
        caret.saturating_sub(half)
    };
    let end = start.saturating_add(SNIPPET_WIDTH).min(shown.len());
    let mut window = shown.get(start..end).unwrap_or_default().to_owned();
    let mut column = caret.saturating_sub(start);
    if start > 0 {
        window.insert_str(0, "...");
        column = column.saturating_add(3);
    }
    if end < shown.len() {
        window.push_str("...");
    }
    format!("  {window}\n  {}^\n", " ".repeat(column))
}
