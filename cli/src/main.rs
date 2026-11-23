mod args;
mod clock;
mod render;
mod report;

use std::io::{self, BufRead, IsTerminal, Write};
use std::process::ExitCode;

use args::{Command, Options};
use report::{MAX_EXPRESSION_BYTES, Report, Settings, Status};

const HELP: &str = "\
Check a systemd calendar expression (the value of OnCalendar=), print its
normalized form, and list when it elapses next.

Usage: oncalendar [OPTIONS] [EXPRESSION...]

The words of EXPRESSION are joined with single spaces, so quoting is optional.
With no EXPRESSION, each line of standard input is one expression. Empty lines
and lines that start with # are skipped.

Options:
  -n, --count N      List N elapses per expression, 0 to 10000 [default: 5].
                     0 only checks the expression.
  -a, --after TIME   List elapses strictly after TIME [default: now].
                     TIME is now, @UNIX_SECONDS, an RFC 3339 time such as
                     2026-03-08T06:59:00Z, or a wall-clock time such as
                     '2026-03-08 01:59' with an optional zone name after it.
  -z, --zone NAME    The local time zone, used for expressions that name no
                     zone and for printing [default: the system time zone].
      --bundled-tzdb Look zone names up in the time zone database compiled
                     into this program instead of the system copy. The system
                     copy is used when present, the bundled one otherwise.
      --json         Print one JSON object per expression.
  -h, --help         Print this help.
  -V, --version      Print the version.

Exit status:
  0  every expression is valid and elapses (or --count is 0)
  1  an expression is invalid
  2  usage error
  3  a time zone name is not in the time zone database
  4  an expression never elapses after TIME
  5  standard input could not be read or output could not be written
When expressions fail in different ways, the lowest of these codes wins.
";

fn main() -> ExitCode {
    let status = match args::parse(std::env::args_os().skip(1)) {
        Ok(Command::Help) => emit(HELP),
        Ok(Command::Version) => emit(&format!("oncalendar {}\n", env!("CARGO_PKG_VERSION"))),
        Ok(Command::Run(options)) => run(options),
        Err(problem) => fail(
            Status::Usage,
            &format!("{problem}\nRun 'oncalendar --help' for usage."),
        ),
    };
    ExitCode::from(status as u8)
}

fn run(options: Options) -> Status {
    let stdin = io::stdin();
    if options.expression.is_none() && stdin.is_terminal() {
        return fail(
            Status::Usage,
            "no expression given\nUsage: oncalendar [OPTIONS] [EXPRESSION...], or pipe expressions on standard input.\nRun 'oncalendar --help' for more.",
        );
    }
    let database = clock::database(options.bundled_tzdb);
    let local = match clock::local_zone(options.zone.as_deref(), &database) {
        clock::Local::Found(zone) => zone,
        clock::Local::SystemUndetermined => {
            let _ = writeln!(
                io::stderr(),
                "warning: the system time zone could not be determined, using UTC (pass --zone to choose one)"
            );
            jiff::tz::TimeZone::UTC
        }
        clock::Local::UnknownName => {
            return fail(
                Status::UnknownZone,
                "--zone: the time zone is not in the time zone database (names look like Asia/Seoul or UTC)",
            );
        }
    };
    let after = match options.after.as_deref() {
        None => jiff::Timestamp::now(),
        Some(text) => match clock::after(text, &local, &database) {
            Some(after) => after,
            None => {
                return fail(
                    Status::Usage,
                    "--after: expected now, @UNIX_SECONDS, an RFC 3339 time, or 'YYYY-MM-DD HH:MM[:SS] [ZONE]'\nRun 'oncalendar --help' for usage.",
                );
            }
        },
    };
    let settings = Settings {
        count: options.count,
        after,
        local,
        database,
    };
    let mut sink = Sink {
        out: io::BufWriter::new(io::stdout().lock()),
        json: options.json,
        printed: false,
        status: Status::Ok,
    };
    let result = match options.expression {
        Some(expression) => sink.emit(
            &report::from_bytes(None, expression.into_bytes(), &settings),
            &settings,
        ),
        None => each_line(&mut stdin.lock(), |number, bytes| {
            sink.emit(
                &report::from_bytes(Some(number), bytes, &settings),
                &settings,
            )
        }),
    };
    let result = result.and_then(|()| sink.out.flush());
    match result {
        Ok(()) => sink.status,
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => sink.status,
        Err(error) => fail(
            Status::Io,
            &format!("cannot read standard input or write the output: {error}"),
        ),
    }
}

struct Sink<W: Write> {
    out: W,
    json: bool,
    printed: bool,
    status: Status,
}

impl<W: Write> Sink<W> {
    fn emit(&mut self, report: &Report, settings: &Settings) -> io::Result<()> {
        self.status = self.status.and(report.status());
        if self.json {
            return self
                .out
                .write_all(render::json(report, &settings.local).as_bytes());
        }
        let separator = if self.printed { "\n" } else { "" };
        self.printed = true;
        match render::human(report, settings.after, &settings.local) {
            render::Human::Result(text) => write!(self.out, "{separator}{text}"),
            render::Human::Error(text) => {
                self.out.flush()?;
                let _ = write!(io::stderr(), "{separator}{text}");
                Ok(())
            }
        }
    }
}

fn each_line(
    input: &mut impl BufRead,
    mut handle: impl FnMut(usize, Vec<u8>) -> io::Result<()>,
) -> io::Result<()> {
    let mut number: usize = 0;
    while let Some(mut line) = read_line(input, MAX_EXPRESSION_BYTES.saturating_add(1))? {
        number = number.saturating_add(1);
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        if line.is_empty() || line.first() == Some(&b'#') {
            continue;
        }
        handle(number, line)?;
    }
    Ok(())
}

fn read_line(input: &mut impl BufRead, keep: usize) -> io::Result<Option<Vec<u8>>> {
    let mut line = Vec::new();
    let mut read_any = false;
    loop {
        let chunk = match input.fill_buf() {
            Ok(chunk) => chunk,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        if chunk.is_empty() {
            return Ok(read_any.then_some(line));
        }
        read_any = true;
        let newline = chunk.iter().position(|&byte| byte == b'\n');
        let content = chunk
            .get(..newline.unwrap_or(chunk.len()))
            .unwrap_or_default();
        let room = keep.saturating_sub(line.len());
        line.extend_from_slice(content.get(..room.min(content.len())).unwrap_or_default());
        let used = newline.map_or(chunk.len(), |at| at.saturating_add(1));
        input.consume(used);
        if newline.is_some() {
            return Ok(Some(line));
        }
    }
}

fn emit(text: &str) -> Status {
    let mut out = io::stdout().lock();
    match out.write_all(text.as_bytes()).and_then(|()| out.flush()) {
        Ok(()) => Status::Ok,
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Status::Ok,
        Err(_) => Status::Io,
    }
}

fn fail(status: Status, message: &str) -> Status {
    let _ = writeln!(io::stderr(), "error: {message}");
    status
}
