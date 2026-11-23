#!/bin/sh
# Weight report and gate. Run from any directory.
#
#   scripts/check-weight.sh
#
# Two configurations are measured: no default features (parse and normalize only) and default
# features (adds `upcoming`, which needs jiff).
#
# Gate (fails the script): the number of crates a user compiles, other than this one, exceeds
# the limit for its configuration.
#   - no default features: MAX_NO_DEFAULT_DEPS (none at all)
#   - default features:    MAX_DEFAULT_DEPS (jiff and jiff-core, as resolved for this host;
#                          a Windows or wasm target also pulls jiff's bundled tz database)
#
# Report (printed, not gated, because wall time depends on the machine):
#   - cold release build time per configuration, in a fresh target directory
#   - size of the release rlib per configuration
#   - benchmark results, when benches/ exists
set -eu

cd "$(dirname -- "$0")/.."

MAX_NO_DEFAULT_DEPS=0
MAX_DEFAULT_DEPS=2

# Counts the crates other than this one in the normal (non-dev, non-build) dependency graph.
# A failing `cargo tree` stops the script: an empty graph must not read as zero dependencies.
count_deps() {
	tree=$(cargo tree --edges normal --prefix none "$@") || { echo "cargo tree failed" >&2; exit 1; }
	printf '%s\n' "$tree" | grep -q '^oncalendar v' || { echo "cargo tree did not list this crate" >&2; exit 1; }
	printf '%s\n' "$tree" | sed 's/ (\*)$//' | sort -u | grep -vc '^oncalendar v' || true
}

target=$(mktemp -d)
trap 'rm -rf "$target"' EXIT

# Builds one configuration from cold and prints its time and rlib size.
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
