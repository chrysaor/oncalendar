#!/bin/sh
set -eu

cd "$(dirname -- "$0")/.."

if ! cargo minimal-versions --version >/dev/null 2>&1 || ! cargo hack --version >/dev/null 2>&1; then
	echo "FAIL: cargo-minimal-versions and cargo-hack are not both installed"
	echo "install them with: cargo install cargo-minimal-versions cargo-hack --locked"
	exit 1
fi

echo "$(rustc --version)"
cargo minimal-versions test --all-features

printf '\nminimal versions: PASS\n'
