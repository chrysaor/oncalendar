#!/bin/sh
set -eu

cd "$(dirname -- "$0")/.."

for tool in actionlint zizmor; do
	if ! command -v "$tool" >/dev/null 2>&1; then
		echo "FAIL: $tool is not installed"
		echo "install actionlint with: go install github.com/rhysd/actionlint/cmd/actionlint@v1.7.12"
		echo "install zizmor with: pipx install zizmor==1.33.1"
		exit 1
	fi
done

actionlint -version | head -n 1
actionlint
echo "actionlint: no findings"

zizmor --version
zizmor --no-progress .github

printf '\nworkflows: PASS\n'
