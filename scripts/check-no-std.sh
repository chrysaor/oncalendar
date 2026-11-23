#!/bin/sh
set -eu

cd "$(dirname -- "$0")/.."

TARGET=thumbv7em-none-eabihf

sysroot=$(rustc --print sysroot)
if [ ! -d "$sysroot/lib/rustlib/$TARGET" ]; then
	echo "FAIL: the target $TARGET is not installed for $(rustc --version)"
	echo "install it with: rustup target add $TARGET"
	exit 1
fi

echo "$(rustc --version), target $TARGET"
cargo build --quiet --no-default-features --target "$TARGET"
cargo build --quiet --no-default-features --features jiff --target "$TARGET"
cargo build --quiet --no-default-features --features cron --target "$TARGET"

printf '\nno_std gate: PASS\n'
