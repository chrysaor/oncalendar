use std::ffi::OsString;

pub const DEFAULT_COUNT: usize = 5;
pub const MAX_COUNT: usize = 10_000;

pub enum Command {
    Help,
    Version,
    Run(Options),
}

pub struct Options {
    pub count: usize,
    pub after: Option<String>,
    pub zone: Option<String>,
    pub bundled_tzdb: bool,
    pub json: bool,
    pub expression: Option<String>,
}

pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command, String> {
    let mut options = Options {
        count: DEFAULT_COUNT,
        after: None,
        zone: None,
        bundled_tzdb: false,
        json: false,
        expression: None,
    };
    let mut words = Vec::new();
    let mut only_words = false;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let arg = arg
            .into_string()
            .map_err(|_| "arguments must be valid UTF-8".to_owned())?;
        if only_words || arg == "-" || !arg.starts_with('-') {
            words.push(arg);
            continue;
        }
        if arg == "--" {
            only_words = true;
            continue;
        }
        let (name, inline) = match arg.split_once('=') {
            Some((name, value)) if name.starts_with("--") => (name, Some(value.to_owned())),
            _ => (arg.as_str(), None),
        };
        match name {
            "-h" | "--help" => return Ok(Command::Help),
            "-V" | "--version" => return Ok(Command::Version),
            "--json" => options.json = flag(name, inline)?,
            "--bundled-tzdb" => options.bundled_tzdb = flag(name, inline)?,
            "-n" | "--count" => options.count = count(&value(name, inline, &mut args)?)?,
            "-a" | "--after" => options.after = Some(value(name, inline, &mut args)?),
            "-z" | "--zone" => options.zone = Some(value(name, inline, &mut args)?),
            _ => {
                return Err(format!(
                    "unknown option {name:?} (put -- before an expression that starts with -)"
                ));
            }
        }
    }
    if !words.is_empty() {
        options.expression = Some(words.join(" "));
    }
    Ok(Command::Run(options))
}

fn flag(name: &str, inline: Option<String>) -> Result<bool, String> {
    match inline {
        None => Ok(true),
        Some(_) => Err(format!("{name} takes no value")),
    }
}

fn value(
    name: &str,
    inline: Option<String>,
    rest: &mut impl Iterator<Item = OsString>,
) -> Result<String, String> {
    if let Some(value) = inline {
        return Ok(value);
    }
    rest.next()
        .ok_or_else(|| format!("{name} needs a value"))?
        .into_string()
        .map_err(|_| "arguments must be valid UTF-8".to_owned())
}

fn count(text: &str) -> Result<usize, String> {
    match text.parse::<usize>() {
        Ok(count) if count <= MAX_COUNT => Ok(count),
        _ => Err(format!(
            "--count takes a whole number from 0 to {MAX_COUNT}"
        )),
    }
}
