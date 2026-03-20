#!/usr/bin/env bash
set -euo pipefail

BIN="${1:-target/debug/sxmc}"
TMPDIR="$(mktemp -d)"

cleanup() {
  rm -rf "${TMPDIR}"
}

trap cleanup EXIT

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Required command not found: $1" >&2
    exit 1
  fi
}

run_and_capture() {
  local name="$1"
  shift
  echo "Smoke check: ${name}"
  "$@" >"${TMPDIR}/${name}.out"
}

require_cmd npx
"${BIN}" --version >/dev/null

run_and_capture everything_list \
  "${BIN}" stdio "npx -y @modelcontextprotocol/server-everything" --list
grep -q "get-sum" "${TMPDIR}/everything_list.out"
grep -q "Prompts" "${TMPDIR}/everything_list.out"

run_and_capture everything_sum \
  "${BIN}" stdio "npx -y @modelcontextprotocol/server-everything" get-sum a=2 b=3 --pretty
grep -q "5" "${TMPDIR}/everything_sum.out"

run_and_capture memory_list \
  "${BIN}" stdio "npx -y @modelcontextprotocol/server-memory" --list
grep -q "read_graph" "${TMPDIR}/memory_list.out"

run_and_capture memory_zero_arg \
  "${BIN}" stdio "npx -y @modelcontextprotocol/server-memory" read_graph --pretty
test -s "${TMPDIR}/memory_zero_arg.out"

run_and_capture filesystem_list \
  "${BIN}" stdio "npx -y @modelcontextprotocol/server-filesystem /tmp" --list
grep -q "list_allowed_directories" "${TMPDIR}/filesystem_list.out"

run_and_capture filesystem_zero_arg \
  "${BIN}" stdio "npx -y @modelcontextprotocol/server-filesystem /tmp" list_allowed_directories --pretty
grep -q "/tmp" "${TMPDIR}/filesystem_zero_arg.out"

run_and_capture sequential_thinking \
  "${BIN}" stdio "npx -y @modelcontextprotocol/server-sequential-thinking" \
  sequentialthinking thought="Step A" thoughtNumber=1 totalThoughts=1 nextThoughtNeeded=false --pretty
test -s "${TMPDIR}/sequential_thinking.out"

run_and_capture github_list \
  "${BIN}" stdio "npx -y @modelcontextprotocol/server-github" --list
test -s "${TMPDIR}/github_list.out"

echo "Real-world MCP smoke checks passed."
