#!/bin/sh
set -eu

cd "$(dirname -- "$0")/.."

expected='CHANGELOG.md
Cargo.lock
Cargo.toml
Cargo.toml.orig
LICENSE-APACHE
LICENSE-MIT
README.md
src/cron.rs
src/error.rs
src/format.rs
src/lib.rs
src/model.rs
src/next.rs
src/parse.rs
src/zone.rs
tests/cron.rs
tests/fixtures/cases.tsv
tests/fixtures/cron.tsv
tests/fixtures/expected.tsv
tests/normalize.rs
tests/oracle.rs
tests/robustness.rs'

listed=$(cargo package --list --allow-dirty --quiet | grep -vx '\.cargo_vcs_info\.json')

allowed=$(mktemp)
trap 'rm -f "$allowed"' EXIT
printf '%s\n' "$expected" > "$allowed"

if ! printf '%s\n' "$listed" | diff -u "$allowed" -; then
	echo "FAIL: the package file list differs from the allow list (- allowed, + packaged)"
	echo "update the allow list in scripts/check-package.sh or the include list in Cargo.toml"
	exit 1
fi
echo "package files: $(printf '%s\n' "$listed" | grep -c '') as allowed"

cargo package --allow-dirty --quiet

printf '\npackage: PASS\n'
