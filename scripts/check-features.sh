#!/bin/sh
set -eu

cd "$(dirname -- "$0")/.."

if ! cargo hack --version >/dev/null 2>&1; then
	echo "FAIL: cargo-hack is not installed"
	echo "install it with: cargo install cargo-hack --locked"
	exit 1
fi

cargo hack clippy --feature-powerset --all-targets -- --deny warnings

printf '\nfeature powerset: PASS\n'
