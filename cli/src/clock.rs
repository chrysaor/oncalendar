use jiff::Timestamp;
use jiff::civil::DateTime;
use jiff::tz::{TimeZone, TimeZoneDatabase};

pub fn database(bundled: bool) -> TimeZoneDatabase {
    if bundled {
        TimeZoneDatabase::bundled()
    } else {
        jiff::tz::db().clone()
    }
}

pub enum Local {
    Found(TimeZone),
    UnknownName,
    SystemUndetermined,
}

pub fn local_zone(name: Option<&str>, database: &TimeZoneDatabase) -> Local {
    match (
        name,
        name.map_or_else(TimeZone::try_system, |name| database.get(name)),
    ) {
        (_, Ok(zone)) => Local::Found(zone),
        (Some(_), Err(_)) => Local::UnknownName,
        (None, Err(_)) => Local::SystemUndetermined,
    }
}

pub fn after(text: &str, local: &TimeZone, database: &TimeZoneDatabase) -> Option<Timestamp> {
    let text = text.trim();
    if text == "now" {
        return Some(Timestamp::now());
    }
    if let Some(seconds) = text.strip_prefix('@') {
        return Timestamp::from_second(seconds.parse().ok()?).ok();
    }
    if let Ok(instant) = text.parse::<Timestamp>() {
        return Some(instant);
    }
    if let Some((civil, name)) = text.rsplit_once(' ') {
        if let Ok(zone) = database.get(name) {
            return civil_in(civil, zone);
        }
    }
    civil_in(text, local.clone())
}

fn civil_in(text: &str, zone: TimeZone) -> Option<Timestamp> {
    let civil: DateTime = text.trim_end().parse().ok()?;
    Some(civil.to_zoned_datetime(zone).ok()?.timestamp())
}
