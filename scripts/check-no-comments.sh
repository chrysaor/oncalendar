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
report "comments in tests, benches, the probe, or the fuzz targets:" \
	"$(grep -rnE '^[[:space:]]*//|/\*' tests benches .claude/skills/verify-oncalendar/probe/src fuzz/fuzz_targets --include='*.rs' || true)"
report "comments in scripts and configuration:" \
	"$(grep -nE '^[[:space:]]*#([^!]|$)|[[:space:]]#[[:space:]]' \
		scripts/*.sh oracle/*.sh oracle/query.pl oracle/Dockerfile \
		.claude/skills/verify-oncalendar/bin/verify \
		Cargo.toml deny.toml .github/workflows/*.yml .github/dependabot.yml .github/ISSUE_TEMPLATE/*.yml fuzz/Cargo.toml \
		.claude/skills/verify-oncalendar/probe/Cargo.toml || true)"

[ "$fail" -eq 0 ] && echo "no comments: PASS"
exit "$fail"
