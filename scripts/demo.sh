#!/usr/bin/env bash
set -euo pipefail

BIN="${1:-target/debug/sxmc}"
FIXTURES="${2:-tests/fixtures}"

echo '$ '"$BIN"' skills list --paths '"$FIXTURES"
"$BIN" skills list --paths "$FIXTURES"
echo

echo '$ '"$BIN"' doctor'
"$BIN" doctor
echo

echo '$ '"$BIN"' stdio "'"$BIN"' serve --paths '"$FIXTURES"'" --list-tools --limit 5'
"$BIN" stdio "$BIN serve --paths $FIXTURES" --list-tools --limit 5
echo

echo '$ '"$BIN"' api https://petstore3.swagger.io/api/v3/openapi.json --list'
"$BIN" api https://petstore3.swagger.io/api/v3/openapi.json --list
echo

echo '$ '"$BIN"' inspect cli gh --format toon'
"$BIN" inspect cli gh --format toon
