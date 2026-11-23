#!/bin/sh
# Regenerates the expectations from real systemd and fails if they differ from the committed file.
#
#   oracle/check.sh
#
# The first line names the systemd build and is compared too, so a different build is noticed
# even when it prints the same rows.
set -eu

cd "$(dirname -- "$0")/.."

fresh=$(mktemp)
trap 'rm -f "$fresh"' EXIT

oracle/run.sh < tests/fixtures/cases.tsv > "$fresh"

if diff -u tests/fixtures/expected.tsv "$fresh"; then
	echo "oracle: expected.tsv matches $(head -n 1 "$fresh" | sed 's/^# oracle: //')"
else
	echo "oracle: tests/fixtures/expected.tsv is stale. Regenerate it with:"
	echo "  oracle/run.sh < tests/fixtures/cases.tsv > tests/fixtures/expected.tsv"
	exit 1
fi
