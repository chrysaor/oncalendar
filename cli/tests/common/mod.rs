#![allow(
    dead_code,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    reason = "test code: a panic is a test failure"
)]

use std::io::Write;
use std::process::{Command, Stdio};

pub struct Run {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

pub fn run(args: &[&str], stdin: Option<&[u8]>) -> Run {
    let mut child = Command::new(env!("CARGO_BIN_EXE_oncalendar"))
        .args(args)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the executable starts");
    if let Some(input) = stdin {
        let mut pipe = child.stdin.take().expect("stdin is piped");
        let input = input.to_vec();
        std::thread::spawn(move || {
            let _ = pipe.write_all(&input);
        });
    }
    let output = child.wait_with_output().expect("the executable finishes");
    Run {
        code: output
            .status
            .code()
            .expect("the executable exits with a code"),
        stdout: String::from_utf8(output.stdout).expect("stdout is UTF-8"),
        stderr: String::from_utf8(output.stderr).expect("stderr is UTF-8"),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(i64),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(fields) => fields.iter().find(|(name, _)| name == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn str(&self, key: &str) -> Option<&str> {
        match self.get(key) {
            Some(Json::String(text)) => Some(text),
            _ => None,
        }
    }

    pub fn number(&self, key: &str) -> Option<i64> {
        match self.get(key) {
            Some(Json::Number(n)) => Some(*n),
            _ => None,
        }
    }

    pub fn strings(&self, key: &str) -> Option<Vec<String>> {
        match self.get(key) {
            Some(Json::Array(items)) => items
                .iter()
                .map(|item| match item {
                    Json::String(text) => Some(text.clone()),
                    _ => None,
                })
                .collect(),
            _ => None,
        }
    }
}

pub fn parse_json(text: &str) -> Result<Json, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut at = 0;
    let value = value(&chars, &mut at)?;
    if at != chars.len() {
        return Err(format!("trailing input at {at}"));
    }
    Ok(value)
}

fn value(chars: &[char], at: &mut usize) -> Result<Json, String> {
    match chars.get(*at) {
        Some('{') => {
            *at += 1;
            let mut fields = Vec::new();
            if chars.get(*at) == Some(&'}') {
                *at += 1;
                return Ok(Json::Object(fields));
            }
            loop {
                let Json::String(key) = value(chars, at)? else {
                    return Err(format!("object key at {at}"));
                };
                expect(chars, at, ':')?;
                fields.push((key, value(chars, at)?));
                match chars.get(*at) {
                    Some(',') => *at += 1,
                    Some('}') => {
                        *at += 1;
                        return Ok(Json::Object(fields));
                    }
                    _ => return Err(format!("object at {at}")),
                }
            }
        }
        Some('[') => {
            *at += 1;
            let mut items = Vec::new();
            if chars.get(*at) == Some(&']') {
                *at += 1;
                return Ok(Json::Array(items));
            }
            loop {
                items.push(value(chars, at)?);
                match chars.get(*at) {
                    Some(',') => *at += 1,
                    Some(']') => {
                        *at += 1;
                        return Ok(Json::Array(items));
                    }
                    _ => return Err(format!("array at {at}")),
                }
            }
        }
        Some('"') => {
            *at += 1;
            let mut text = String::new();
            loop {
                let ch = *chars.get(*at).ok_or("unterminated string")?;
                *at += 1;
                match ch {
                    '"' => return Ok(Json::String(text)),
                    '\\' => {
                        let escape = *chars.get(*at).ok_or("unterminated escape")?;
                        *at += 1;
                        match escape {
                            '"' => text.push('"'),
                            '\\' => text.push('\\'),
                            '/' => text.push('/'),
                            'n' => text.push('\n'),
                            'r' => text.push('\r'),
                            't' => text.push('\t'),
                            'b' => text.push('\u{8}'),
                            'f' => text.push('\u{c}'),
                            'u' => {
                                let hex: String =
                                    chars.get(*at..*at + 4).ok_or("short \\u")?.iter().collect();
                                *at += 4;
                                let code =
                                    u32::from_str_radix(&hex, 16).map_err(|e| e.to_string())?;
                                text.push(char::from_u32(code).ok_or("surrogate in \\u")?);
                            }
                            other => return Err(format!("escape \\{other}")),
                        }
                    }
                    '\u{0}'..='\u{1f}' => return Err(format!("raw control character at {at}")),
                    other => text.push(other),
                }
            }
        }
        Some('t') => literal(chars, at, "true", Json::Bool(true)),
        Some('f') => literal(chars, at, "false", Json::Bool(false)),
        Some('n') => literal(chars, at, "null", Json::Null),
        Some(c) if *c == '-' || c.is_ascii_digit() => {
            let start = *at;
            *at += 1;
            while chars.get(*at).is_some_and(char::is_ascii_digit) {
                *at += 1;
            }
            let digits: String = chars.get(start..*at).unwrap_or_default().iter().collect();
            digits.parse().map(Json::Number).map_err(|e| e.to_string())
        }
        _ => Err(format!("value at {at}")),
    }
}

fn expect(chars: &[char], at: &mut usize, want: char) -> Result<(), String> {
    if chars.get(*at) == Some(&want) {
        *at += 1;
        Ok(())
    } else {
        Err(format!("expected {want:?} at {at}"))
    }
}

fn literal(chars: &[char], at: &mut usize, word: &str, json: Json) -> Result<Json, String> {
    for want in word.chars() {
        expect(chars, at, want)?;
    }
    Ok(json)
}
