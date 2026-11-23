# Weight and security gates

A user who adds this crate pays for it in dependencies, build time, and binary size, and trusts it with untrusted text. The gates turn both promises into checks that fail.

## Sub-features

- `weight-zero` compiles no other crate with `default-features = false`.
- `weight-default` compiles at most the budgeted number of crates with default features.
- `weight-report` prints cold build time and library size for both configurations.
- `sec-lints` makes `unsafe` code and panicking constructs build errors in the library.
- `sec-supply-chain` passes `cargo deny` for advisories, licenses, bans, and sources.
- `sec-robustness` passes the hostile-input test suite.
- `oracle-drift` confirms the committed expectations still match systemd.
- `features` passes clippy on every feature combination.
- `minimal-versions` passes the tests with every dependency at its lowest allowed version.
- `package` publishes exactly the allowed files, and the packaged crate builds.
- `workflows` passes `actionlint` and `zizmor`.

## How to get to it (user POV)

- `cargo add oncalendar` pulls the default dependency set. `default-features = false` pulls none.
- A user's `cargo deny` and `cargo audit` runs see whatever this crate depends on.

## Driving it with verify

Preconditions:

- `verify doctor` exits 0.

- **All gates.** Run `verify gates`. Every line reads `PASS`.
- **Weight alone.** Run `scripts/check-weight.sh`. It prints the dependency count against its limit for each configuration and ends with `weight gate: PASS`.
- **Security alone.** Run `scripts/check-security.sh`. Its three steps complete and it ends with `security gate: PASS`.
- **Drift alone.** Run `oracle/check.sh`. It prints `oracle: expected.tsv matches systemd ...`.
- **The other gates alone.** Run `scripts/check-features.sh`, `scripts/check-minimal-versions.sh`, `scripts/check-package.sh`, or `scripts/check-workflows.sh`. Each ends with a `PASS` line.
- **Proof.** Quote the PASS lines and the evidence directory that `verify gates` prints. For a weight claim, also quote the dependency counts from `weight.txt`.

## Gotchas

- Build time depends on the machine and its load. The gate fails on dependency count only. When comparing two build times, alternate the two sides at least five times and report the range.
- A new dependency needs a raised limit in `scripts/check-weight.sh`. Raising it is a design decision. Record the reason in `docs/architecture.md`.
- The lints apply to tests too. A test file opts out with its own `#![allow(...)]`. `src/` must not.
- `cargo deny` needs network access for the advisory database on first use.
- The robustness suite needs the `jiff` feature for its scheduling properties. Run it with `--all-features`.
