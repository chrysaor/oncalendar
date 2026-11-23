#!/bin/sh
# Builds the oracle image and runs the cases on stdin through the real systemd.
#
#   oracle/run.sh [ITERATIONS] < cases.tsv > expected.tsv
#
# See oracle/query.pl for the row format.
set -eu

dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)

docker build --quiet --tag oncalendar-oracle "$dir" >/dev/null
exec docker run --rm --interactive --network none oncalendar-oracle "$@"
