#!/bin/sh
set -eu

cd "$(dirname -- "$0")/.."

step() { printf '\n== %s\n' "$1"; }

step "1/3 lints: unsafe code and panicking constructs are build errors"
grep -q '^unsafe_code = "forbid"' Cargo.toml || { echo "Cargo.toml no longer forbids unsafe code"; exit 1; }
cargo clippy --quiet --lib --all-features -- --deny warnings
cargo clippy --quiet --lib --no-default-features -- --deny warnings
if grep -rnE '#!?\[[[:space:]]*(allow|expect|cfg_attr)[[:space:]]*\(' src; then
	echo "src/ switches a lint off; the library must not"
	exit 1
fi

step "2/3 cargo-deny: advisories, licenses, bans, sources"
if [ "${1:-}" = "--skip-deny" ]; then
	echo "skipped"
else
	cargo deny --all-features check
fi

step "3/3 robustness tests"
cargo test --quiet --all-features --test robustness
cargo test --quiet --all-features --test cron
cargo test --quiet --no-default-features --test robustness

printf '\nsecurity gate: PASS\n'
