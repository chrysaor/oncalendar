#!/bin/sh
set -eu

cd "$(dirname -- "$0")/.."

MAX_NO_DEFAULT_DEPS=0
MAX_DEFAULT_DEPS=2

count_deps() {
	tree=$(cargo tree --edges normal --prefix none "$@") || { echo "cargo tree failed" >&2; exit 1; }
	printf '%s\n' "$tree" | grep -q '^oncalendar v' || { echo "cargo tree did not list this crate" >&2; exit 1; }
	printf '%s\n' "$tree" | sed 's/ (\*)$//' | sort -u | grep -vc '^oncalendar v' || true
}

target=$(mktemp -d)
trap 'rm -rf "$target"' EXIT

measure() {
	name=$1
	shift
	dir="$target/$name"
	start=$(date +%s)
	CARGO_TARGET_DIR="$dir" cargo build --quiet --release "$@"
	end=$(date +%s)
	rlib=$(find "$dir/release" -maxdepth 1 -name 'liboncalendar*.rlib' | head -n 1)
	echo "  cold release build: $((end - start)) s (one run, load: $(uptime | sed 's/.*load averages*: //'))"
	echo "  release rlib size: $(wc -c < "$rlib" | tr -d ' ') bytes"
}

no_default_deps=$(count_deps --no-default-features) || exit 1
echo "no default features"
echo "  dependencies: $no_default_deps (limit $MAX_NO_DEFAULT_DEPS)"
measure no-default --no-default-features

default_deps=$(count_deps) || exit 1
echo "default features"
echo "  dependencies: $default_deps (limit $MAX_DEFAULT_DEPS)"
measure default

if [ -d benches ]; then
	echo
	cargo bench --quiet
fi

fail=0
[ "$no_default_deps" -le "$MAX_NO_DEFAULT_DEPS" ] || { echo "FAIL: dependency count without default features is over the limit"; fail=1; }
[ "$default_deps" -le "$MAX_DEFAULT_DEPS" ] || { echo "FAIL: dependency count with default features is over the limit"; fail=1; }
[ "$fail" -eq 0 ] && echo && echo "weight gate: PASS"
exit "$fail"
