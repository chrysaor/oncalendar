# Contributing to oncalendar

This guide covers the rules every change follows and the commands that check them. The design and the reasons behind it are in [docs/architecture.md](docs/architecture.md).

## Before you start

You do not need every tool in this guide to send a change. Locally, run `cargo test --all-features` and, for a change to parsing or elapses, `scripts/verify compare`. CI runs the rest on every pull request.

If something is unclear, or a tool does not work for you, open an issue and ask. A question is welcome before any code, and so is a report that systemd gives a different result, even without a fix.

## The clean-room rule

systemd is licensed LGPL-2.1-or-later, and this crate is `MIT OR Apache-2.0`. Code or test tables derived from LGPL source would bring LGPL terms into the crate. Other implementations of this grammar have their own licenses, and reading them makes it hard to show that this code was written independently. The rule exists to keep the license of the crate clean, so it is not open to exceptions.

You may use these two sources:

- The manual page `systemd.time(7)`, section "Calendar Events".
- The output of `systemd-analyze calendar`, through the oracle in `oracle/`.

Do not do these things:

- Open, search, quote, or port the systemd source code or its tests.
- Open the source of any other implementation of this grammar, in any language.
- Paste code from those sources into an issue or a pull request.
- Copy text from the manual into the repository. Describe the grammar in your own words. Short example expressions are fine.

When the manual does not answer a question, ask systemd with new fixture rows, as in the next section. Do not guess, and do not look at another implementation.

## Start a new behavior with fixture rows

The expected results come from systemd, never from this crate.

1. Add rows to `tests/fixtures/cases.tsv`. Each row has three tab-separated columns: the local time zone, the base time, and the expression, such as `UTC`, `2026-01-01 00:00:00 UTC`, and `Mon..Fri 09:00`. Put the rows under a `#` section title that says what they check.
2. Regenerate the expected results:

    ```sh
    oracle/run.sh < tests/fixtures/cases.tsv > tests/fixtures/expected.tsv
    ```

3. Run `cargo test --all-features` and see the new rows fail.
4. Change the code until the tests pass.
5. Run the comparison against systemd and check that it reports `differing: 0`:

    ```sh
    scripts/verify compare
    ```

6. Commit `cases.tsv`, `expected.tsv`, and the code together.

Never edit `tests/fixtures/expected.tsv` by hand. `oracle/check.sh` fails in CI when the file does not match systemd.

To try one case before you add it, run `verify one` with the local zone, the base time, and the expression:

```sh
scripts/verify one UTC '2026-01-01 00:00:00 UTC' 'Mon..Fri *-*-* 09:00'
```

When a difference from systemd stays in the crate, list it under "Known differences from systemd" in `README.md` and in `docs/architecture.md`.

## No comments

Code, scripts, and configuration carry no comments. Names and structure say what the code does. The reasons go in `docs/architecture.md` or in the commit message.

The one exception is `///` and `//!` documentation in `src/`, because it is the published API reference. The `#` lines in `tests/fixtures/*.tsv` are section titles, not comments. `scripts/check-no-comments.sh` enforces the rule.

## Rules for the library code

- Input is untrusted. The library has no `unsafe` code, does not panic, bounds its input, and does not repeat input in error messages.
- Do not add `#[allow]`, `#[expect]`, or `#[cfg_attr]` in `src/`. `scripts/check-security.sh` rejects them.
- Parsing reads no file and no environment variable. Only the modules behind the `jiff` feature, `zone` and `next`, may use the time zone database.
- With `default-features = false`, the crate compiles no other crate. A new dependency needs a raised limit in `scripts/check-weight.sh` and a reason in `docs/architecture.md`.
- Everything in the repository is in English.

## Check your change

Run the tests in both feature configurations:

```sh
cargo test --all-features
cargo test --no-default-features
```

Run every gate:

```sh
scripts/verify gates
```

The gates are formatting, the comment check, clippy on every feature combination, the tests with the lowest allowed dependency versions, the package check, the CI configuration check, the tests, the security gate, the weight gate, the `no_std` build, the command-line tool check, the WebAssembly build and its fixture check, and the oracle check. Each gate is also a script that runs on its own:

| Gate | Command |
| --- | --- |
| Formatting | `cargo fmt --check` |
| No comments | `scripts/check-no-comments.sh` |
| Every feature combination | `scripts/check-features.sh` |
| Lowest dependency versions | `scripts/check-minimal-versions.sh` |
| Packaged files | `scripts/check-package.sh` |
| CI configuration | `scripts/check-workflows.sh` |
| Lints, licenses, and robustness | `scripts/check-security.sh` |
| Dependency count | `scripts/check-weight.sh` |
| `no_std` build | `scripts/check-no-std.sh` |
| Command-line tool in `cli/` | `scripts/check-cli.sh` |
| WebAssembly build and fixtures | `scripts/check-wasm.sh` |
| Fixtures match systemd | `oracle/check.sh` |

Check that the documentation builds and that its examples run. The examples in `README.md` run as tests too.

```sh
RUSTDOCFLAGS="--deny warnings" cargo doc --no-deps --all-features
RUSTDOCFLAGS="--deny warnings" cargo doc --no-deps --no-default-features
cargo test --doc --all-features
cargo test --doc --no-default-features --features jiff
```

After a change to the parser or to the elapse search, run the fuzz targets for 60 seconds each:

```sh
scripts/fuzz.sh 60
```

A failing input is saved under `fuzz/artifacts/<target>/`.

If you add a file that belongs in the published crate, add it to `include` in `Cargo.toml` and to the allow list in `scripts/check-package.sh`. For the command-line tool, the include list is in `cli/Cargo.toml` and the allow list in `scripts/check-cli.sh`. Files under `wasm/` never belong in either package.

## The command-line tool

`cli/` is a separate crate, `oncalendar-cli`, with its own `[workspace]`. Run its tests with `cargo test --manifest-path cli/Cargo.toml`. They run the executable against every row of `tests/fixtures/expected.tsv` and rerun the examples in `cli/README.md` and `README.md`, so a change to an example's output means a change to the README text. The tool uses only the public API of the library, at the version in `cli/Cargo.toml`. The design is under "Command-line tool" in [docs/architecture.md](docs/architecture.md#command-line-tool).

## The WebAssembly crate

The WebAssembly crate in `wasm/` follows the same rules as the library, with one exception: its exported functions carry `#[unsafe(no_mangle)]`. `scripts/check-wasm.sh` rejects any other `unsafe`. To try the demo page, run `scripts/check-wasm.sh`, then serve `wasm/demo/` with any static file server, such as `python3 -m http.server --directory wasm/demo 8000`.

## Tools

| Tool | Used by | Install |
| --- | --- | --- |
| Rust stable with `rustfmt` and `clippy` | everything | `rustup component add rustfmt clippy` |
| Rust 1.85 | the MSRV check in CI | `rustup toolchain install 1.85` |
| `thumbv7em-none-eabihf` target | `scripts/check-no-std.sh` | `rustup target add thumbv7em-none-eabihf` |
| `wasm32-unknown-unknown` target | `scripts/check-wasm.sh` | `rustup target add wasm32-unknown-unknown` |
| Node.js 18 or later | `scripts/check-wasm.sh` | [nodejs.org](https://nodejs.org/) or your package manager |
| Docker | `oracle/run.sh`, `oracle/check.sh`, `verify` | A running Docker daemon |
| `cargo-deny` | `scripts/check-security.sh` | `cargo install cargo-deny --locked` |
| `cargo-hack` 0.6.46 | `scripts/check-features.sh`, `scripts/check-minimal-versions.sh` | `cargo install cargo-hack --locked` |
| `cargo-minimal-versions` 0.1.39 | `scripts/check-minimal-versions.sh` | `cargo install cargo-minimal-versions --locked` |
| `actionlint` 1.7.12 | `scripts/check-workflows.sh` | `go install github.com/rhysd/actionlint/cmd/actionlint@v1.7.12` |
| `zizmor` 1.33.1 | `scripts/check-workflows.sh` | `pipx install zizmor==1.33.1` |
| Nightly Rust and `cargo-fuzz` 0.13.2 | `scripts/fuzz.sh` | `rustup toolchain install nightly` and `cargo install cargo-fuzz --locked` |

The versions are the ones CI installs. `scripts/verify doctor` checks cargo, Docker, the oracle, `cargo-deny`, `cargo-hack`, `cargo-minimal-versions`, `actionlint`, `zizmor`, the `wasm32-unknown-unknown` target, and Node.js.

## Pull requests

Fill in the checklist in the pull request template. A change to parsing, normalization, or elapses needs the `verify compare` result with `differing: 0`, not only passing unit tests.

By submitting a contribution, you agree that it is dual licensed under `MIT OR Apache-2.0`, as the License section of `README.md` says.
