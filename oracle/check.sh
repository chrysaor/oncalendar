#!/bin/sh
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
