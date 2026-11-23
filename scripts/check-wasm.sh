#!/bin/sh
set -eu

cd "$(dirname -- "$0")/.."

TARGET=wasm32-unknown-unknown
MAX_WASM_DEPS=3

sysroot=$(rustc --print sysroot)
if [ ! -d "$sysroot/lib/rustlib/$TARGET" ]; then
	echo "FAIL: the target $TARGET is not installed for $(rustc --version)"
	echo "install it with: rustup target add $TARGET"
	exit 1
fi
if ! command -v node >/dev/null 2>&1; then
	echo "FAIL: node is not installed"
	echo "install Node.js 18 or later from https://nodejs.org/ or with your package manager"
	exit 1
fi

unsafe_uses=$(grep -rn 'unsafe' wasm/src | grep -vE '^[^:]+:[0-9]+:#\[unsafe\(no_mangle\)\]$' || true)
if [ -n "$unsafe_uses" ]; then
	echo "FAIL: wasm/src uses unsafe for something other than #[unsafe(no_mangle)] on an export"
	echo "$unsafe_uses"
	exit 1
fi

echo "$(rustc --version), target $TARGET, node $(node --version)"
cd wasm
cargo fmt --check
cargo clippy --quiet --all-targets -- --deny warnings
cargo build --quiet --release

tree=$(cargo tree --color never --edges normal --prefix none) || { echo "cargo tree failed"; exit 1; }
deps=$(printf '%s\n' "$tree" | sed 's/ (\*)$//' | sort -u | grep -vc -e '^oncalendar-wasm v' -e '^oncalendar v' || true)
echo "dependencies besides oncalendar: $deps (limit $MAX_WASM_DEPS)"

cp "target/$TARGET/release/oncalendar_wasm.wasm" demo/oncalendar.wasm
echo "demo/oncalendar.wasm: $(wc -c < demo/oncalendar.wasm | tr -d ' ') bytes, $(gzip -9 -c demo/oncalendar.wasm | wc -c | tr -d ' ') bytes with gzip -9"

node check-fixtures.mjs demo/oncalendar.wasm ../tests/fixtures/expected.tsv

[ "$deps" -le "$MAX_WASM_DEPS" ] || { echo "FAIL: the wasm crate has more dependencies than the limit"; exit 1; }
printf '\nwasm gate: PASS\n'
