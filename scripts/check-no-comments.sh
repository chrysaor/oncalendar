#!/bin/sh
set -eu

cd "$(dirname -- "$0")/.."

fail=0

report() {
	if [ -n "$2" ]; then
		echo "$1"
		echo "$2"
		fail=1
	fi
}

report "comments in src/ (only /// and //! documentation is allowed):" \
	"$(grep -rnE '^[[:space:]]*//([^/!]|$)|/\*' src || true)"
report "comments in tests, benches, the probe, the command-line tool, the fuzz targets, or the wasm crate:" \
	"$(grep -rnE '^[[:space:]]*//|/\*' tests benches scripts/probe/src cli/src cli/tests fuzz/fuzz_targets wasm/src --include='*.rs' || true)"
report "comments in the wasm demo and its fixture check:" \
	"$(grep -nE '^[[:space:]]*//|/\*|<!--' wasm/check-fixtures.mjs wasm/demo/*.mjs wasm/demo/*.css wasm/demo/*.html || true)"
report "comments in scripts and configuration:" \
	"$(grep -nE '^[[:space:]]*#([^!]|$)|[[:space:]]#[[:space:]]' \
		scripts/*.sh oracle/*.sh oracle/query.pl oracle/Dockerfile \
		scripts/verify \
		Cargo.toml deny.toml .github/workflows/*.yml .github/dependabot.yml .github/ISSUE_TEMPLATE/*.yml fuzz/Cargo.toml \
		scripts/probe/Cargo.toml cli/Cargo.toml cli/clippy.toml wasm/Cargo.toml wasm/.cargo/config.toml || true)"

[ "$fail" -eq 0 ] && echo "no comments: PASS"
exit "$fail"
