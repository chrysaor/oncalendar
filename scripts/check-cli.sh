#!/bin/sh
set -eu

cd "$(dirname -- "$0")/../cli"

expected='Cargo.lock
Cargo.toml
Cargo.toml.orig
LICENSE-APACHE
LICENSE-MIT
README.md
src/args.rs
src/clock.rs
src/main.rs
src/render.rs
src/report.rs'

step() { printf '\n== %s\n' "$1"; }

step "1/4 format and lints"
cargo fmt --check
cargo clippy --quiet --locked --all-targets -- --deny warnings
grep -q '^unsafe_code = "forbid"' Cargo.toml || { echo "cli/Cargo.toml no longer forbids unsafe code"; exit 1; }
if grep -rnE '#!?\[[[:space:]]*(allow|expect|cfg_attr)[[:space:]]*\(' src; then
	echo "cli/src/ switches a lint off; the tool must not"
	exit 1
fi

step "2/4 tests, including the fixture rows from systemd"
cargo test --quiet --locked

step "3/4 packaged files"
listed=$(cargo package --list --allow-dirty --quiet | grep -vx '\.cargo_vcs_info\.json')
allowed=$(mktemp)
trap 'rm -f "$allowed"' EXIT
printf '%s\n' "$expected" > "$allowed"
if ! printf '%s\n' "$listed" | diff -u "$allowed" -; then
	echo "FAIL: the package file list differs from the allow list (- allowed, + packaged)"
	echo "update the allow list in scripts/check-cli.sh or the include list in cli/Cargo.toml"
	exit 1
fi
echo "package files: $(printf '%s\n' "$listed" | grep -c '') as allowed"

step "4/4 the packaged tool builds against the published library"
if ! output=$(cargo package --quiet --locked --allow-dirty 2>&1); then
	printf '%s\n' "$output"
	echo "FAIL: the packaged tool does not build against the oncalendar release on crates.io"
	exit 1
fi
echo "the packaged tool builds against the oncalendar release on crates.io"

printf '\ncli: PASS\n'
