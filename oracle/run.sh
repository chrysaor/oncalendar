#!/bin/sh
set -eu

dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)

docker build --quiet --tag oncalendar-oracle "$dir" >/dev/null
exec docker run --rm --interactive --network none oncalendar-oracle "$@"
