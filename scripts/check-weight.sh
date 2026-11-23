#!/bin/sh
set -eu

cd "$(dirname -- "$0")/.."

MAX_NO_DEFAULT_DEPS=0
MAX_JIFF_DEPS=2
MAX_DEFAULT_DEPS=2
MAX_CLI_DEPS=4

count_deps() {
	root=$1
	shift
	tree=$(cargo tree --edges normal --prefix none "$@") || { echo "cargo tree failed" >&2; exit 1; }
	printf '%s\n' "$tree" | grep -q "^$root v" || { echo "cargo tree did not list $root" >&2; exit 1; }
	printf '%s\n' "$tree" | sed 's/ (\*)$//' | sort -u | grep -vc "^$root v" || true
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

no_default_deps=$(count_deps oncalendar --no-default-features) || exit 1
echo "no default features"
echo "  dependencies: $no_default_deps (limit $MAX_NO_DEFAULT_DEPS)"
measure no-default --no-default-features

jiff_deps=$(count_deps oncalendar --no-default-features --features jiff) || exit 1
echo "jiff feature only"
echo "  dependencies: $jiff_deps (limit $MAX_JIFF_DEPS)"
measure jiff --no-default-features --features jiff

default_deps=$(count_deps oncalendar) || exit 1
echo "default features"
echo "  dependencies: $default_deps (limit $MAX_DEFAULT_DEPS)"
measure default

cli_deps=$(count_deps oncalendar-cli --manifest-path cli/Cargo.toml) || exit 1
echo "command-line tool"
echo "  dependencies: $cli_deps (limit $MAX_CLI_DEPS)"
start=$(date +%s)
CARGO_TARGET_DIR="$target/cli" cargo build --quiet --release --locked --manifest-path cli/Cargo.toml
end=$(date +%s)
echo "  cold release build: $((end - start)) s (one run)"
echo "  stripped release executable: $(wc -c < "$target/cli/release/oncalendar" | tr -d ' ') bytes"

if [ -d benches ]; then
	echo
	cargo bench --quiet
fi

fail=0
[ "$no_default_deps" -le "$MAX_NO_DEFAULT_DEPS" ] || { echo "FAIL: dependency count without default features is over the limit"; fail=1; }
[ "$jiff_deps" -le "$MAX_JIFF_DEPS" ] || { echo "FAIL: dependency count with only the jiff feature is over the limit"; fail=1; }
[ "$default_deps" -le "$MAX_DEFAULT_DEPS" ] || { echo "FAIL: dependency count with default features is over the limit"; fail=1; }
[ "$cli_deps" -le "$MAX_CLI_DEPS" ] || { echo "FAIL: dependency count of the command-line tool is over the limit"; fail=1; }
[ "$fail" -eq 0 ] && echo && echo "weight gate: PASS"
exit "$fail"
