#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    reason = "test code: a panic is a test failure"
)]

mod common;

use common::run;

const READMES: [(&str, &str); 2] = [
    ("cli/README.md", include_str!("../README.md")),
    ("README.md", include_str!("../../README.md")),
];

struct Example {
    command: String,
    output: String,
}

fn examples(readme: &str) -> Vec<Example> {
    let mut examples: Vec<Example> = Vec::new();
    let mut in_console = false;
    for line in readme.lines() {
        match line {
            "```console" => in_console = true,
            "```" => in_console = false,
            _ if !in_console => {}
            _ => match line.strip_prefix("$ ") {
                Some(command) => examples.push(Example {
                    command: command.to_owned(),
                    output: String::new(),
                }),
                None => {
                    let example = examples.last_mut().expect("output follows a command");
                    example.output.push_str(line);
                    example.output.push('\n');
                }
            },
        }
    }
    examples
}

fn words(command: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    let mut started = false;
    for ch in command.chars() {
        match ch {
            '\'' => {
                quoted = !quoted;
                started = true;
            }
            ' ' if !quoted => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            _ => {
                word.push(ch);
                started = true;
            }
        }
    }
    assert!(!quoted, "unbalanced quote in {command:?}");
    if started {
        words.push(word);
    }
    words
}

#[test]
fn the_readme_examples_print_what_the_readmes_show() {
    let mut checked = 0;
    for (name, readme) in READMES {
        for example in examples(readme) {
            let (stdin, command) = match example.command.split_once(" | ") {
                Some((printf, command)) => {
                    let text = printf
                        .strip_prefix("printf '")
                        .and_then(|rest| rest.strip_suffix('\''))
                        .expect("a pipe starts with printf");
                    (Some(text.replace("\\n", "\n")), command.to_owned())
                }
                None => (None, example.command.clone()),
            };
            let args = words(&command);
            let Some(("oncalendar", args)) = args.split_first().map(|(a, rest)| (a.as_str(), rest))
            else {
                panic!("{name}: {:?} does not run oncalendar", example.command);
            };
            let fixed =
                args.iter().any(|arg| arg == "--after") || example.output.starts_with("error:");
            if !fixed || command.contains("&&") {
                continue;
            }
            let mut args: Vec<&str> = args.iter().map(String::as_str).collect();
            args.insert(0, "--bundled-tzdb");
            let result = run(&args, stdin.as_deref().map(str::as_bytes));
            assert_eq!(
                format!("{}{}", result.stdout, result.stderr),
                example.output,
                "{name}: {}",
                example.command
            );
            checked += 1;
        }
    }
    assert!(checked >= 7, "only {checked} README examples were checked");
}
