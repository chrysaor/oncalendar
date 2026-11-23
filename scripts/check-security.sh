#!/bin/sh
# Security gate. Every step must pass. Run from any directory.
#
#   scripts/check-security.sh [--skip-deny]
#
# --skip-deny leaves out step 2. CI uses it because a separate action runs cargo-deny there.
#
# Steps:
#   1. No unsafe code in the library, and none of the panicking constructs that the lints in
#      Cargo.toml name, with and without default features. No lint is switched off in src/.
#      The lints do not see every panic (`assert!`, `split_at`), so step 3 covers the rest.
#   2. No known advisories, no disallowed licenses, no unknown sources (deny.toml).
#   3. The robustness tests pass, with and without default features: random and hostile input
#      never panics and always terminates.
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
cargo test --quiet --no-default-features --test robustness

printf '\nsecurity gate: PASS\n'
