#!/bin/sh
set -eu

cd "$(dirname -- "$0")/.."

[ $# -ge 1 ] || { echo "usage: scripts/fuzz.sh <seconds per target> [target...]" >&2; exit 2; }
seconds=$1
shift

if ! cargo +nightly fuzz --version >/dev/null 2>&1; then
	echo "FAIL: fuzzing needs cargo-fuzz and a nightly toolchain"
	echo "install them with: rustup toolchain install nightly && cargo install cargo-fuzz --locked"
	exit 1
fi

[ $# -gt 0 ] || set -- $(cargo +nightly fuzz list)

seeds=$(mktemp -d)
trap 'rm -rf "$seeds"' EXIT
mkdir "$seeds/parse_roundtrip" "$seeds/upcoming" "$seeds/from_cron"
grep -v -e '^#' -e '^$' tests/fixtures/cases.tsv | cut -f 3 | sort -u | awk '{ print NR "\t" $0 }' |
	while IFS="$(printf '\t')" read -r number expression; do
		printf '%s' "$expression" > "$seeds/parse_roundtrip/$number"
		{ printf '\000\000\000\000\000\000\000\000\000\000\000\000\000\000'; printf '%s' "$expression"; } > "$seeds/upcoming/$number"
	done
grep -v -e '^#' -e '^$' tests/fixtures/cron.tsv | cut -f 1 | sort -u | awk '{ print NR "\t" $0 }' |
	while IFS="$(printf '\t')" read -r number expression; do
		printf '%s' "$expression" > "$seeds/from_cron/$number"
	done

fail=0
for target in "$@"; do
	[ -d "$seeds/$target" ] || { echo "FAIL: no seeds for the fuzz target $target"; exit 1; }
	mkdir -p "fuzz/corpus/$target"
	printf '\n== %s for %s s\n' "$target" "$seconds"
	if ! cargo +nightly fuzz run "$target" "fuzz/corpus/$target" "$seeds/$target" -- \
		-max_total_time="$seconds" -timeout=10 -rss_limit_mb=2048 -print_final_stats=1; then
		echo "FAIL: $target found a failing input, saved under fuzz/artifacts/$target/"
		fail=1
	fi
done

[ "$fail" -eq 0 ] && printf '\nfuzz: PASS\n'
exit "$fail"
