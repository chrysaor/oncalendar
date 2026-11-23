use std::io::{self, BufRead, Write};

use jiff::Timestamp;
use jiff::civil::DateTime;
use jiff::tz::TimeZone;
use oncalendar::CalendarEvent;

fn main() -> io::Result<()> {
    let iterations: usize = std::env::args()
        .nth(1)
        .and_then(|arg| arg.parse().ok())
        .unwrap_or(5);
    let stdout = io::stdout();
    let mut out = stdout.lock();
    writeln!(out, "# oncalendar: public API via probe, iterations={iterations}")?;
    for line in io::stdin().lock().lines() {
        let line = line?;
        if line.is_empty() || line.starts_with('#') {
            writeln!(out, "{line}")?;
            continue;
        }
        let mut fields = line.splitn(3, '\t');
        let (Some(zone), Some(base), Some(expression)) =
            (fields.next(), fields.next(), fields.next())
        else {
            writeln!(out, "{line}\tbad-input\t-\t-")?;
            continue;
        };
        let (status, normalized, elapses) = run_case(zone, base, expression, iterations);
        writeln!(
            out,
            "{zone}\t{base}\t{expression}\t{status}\t{normalized}\t{elapses}"
        )?;
    }
    Ok(())
}

fn run_case(
    zone: &str,
    base: &str,
    expression: &str,
    iterations: usize,
) -> (&'static str, String, String) {
    let error = || ("error", "-".to_string(), "-".to_string());
    let Ok(event) = expression.parse::<CalendarEvent>() else {
        return error();
    };
    let (Ok(local), Some(after)) = (TimeZone::get(zone), parse_base(base)) else {
        return ("bad-input", "-".to_string(), "-".to_string());
    };
    let Ok(upcoming) = event.upcoming(after, &local) else {
        return error();
    };
    let elapses: Vec<String> = upcoming
        .take(iterations)
        .map(|t| match Timestamp::from_second(t.as_second()) {
            Ok(seconds) => seconds.to_string(),
            Err(_) => "out-of-range".to_string(),
        })
        .collect();
    let elapses = if elapses.is_empty() {
        "never".to_string()
    } else {
        elapses.join(",")
    };
    ("ok", event.to_string(), elapses)
}

fn parse_base(base: &str) -> Option<Timestamp> {
    let (civil, zone) = base.rsplit_once(' ')?;
    let civil: DateTime = civil.parse().ok()?;
    let zone = TimeZone::get(zone).ok()?;
    Some(civil.to_zoned_datetime(zone).ok()?.timestamp())
}
