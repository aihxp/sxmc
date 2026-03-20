#!/usr/bin/env bash
set -euo pipefail

BIN="${1:-target/debug/sxmc}"
FIXTURES="${2:-tests/fixtures}"
PORT_HTTP="${SXMC_SMOKE_HTTP_PORT:-38080}"
PORT_BEARER="${SXMC_SMOKE_BEARER_PORT:-38081}"
BEARER_TOKEN="${SXMC_SMOKE_TOKEN:-sxmc-smoke-token}"
TMPDIR="$(mktemp -d)"
PID_HTTP=""
PID_BEARER=""

cleanup() {
  if [[ -n "${PID_HTTP}" ]]; then
    kill "${PID_HTTP}" >/dev/null 2>&1 || true
    wait "${PID_HTTP}" >/dev/null 2>&1 || true
  fi
  if [[ -n "${PID_BEARER}" ]]; then
    kill "${PID_BEARER}" >/dev/null 2>&1 || true
    wait "${PID_BEARER}" >/dev/null 2>&1 || true
  fi
  rm -rf "${TMPDIR}"
}

trap cleanup EXIT

wait_for_health() {
  local url="$1"
  for _ in $(seq 1 40); do
    if curl --silent --fail "${url}" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.25
  done
  echo "Timed out waiting for ${url}" >&2
  return 1
}

echo "Smoke check: Codex/Cursor/Gemini-style stdio flow"
"${BIN}" stdio "${BIN} serve --paths ${FIXTURES}" --list >"${TMPDIR}/stdio.txt"
grep -q "get_available_skills" "${TMPDIR}/stdio.txt"
grep -q "skill_with_scripts__hello" "${TMPDIR}/stdio.txt"

echo "Smoke check: remote HTTP MCP flow"
"${BIN}" serve --transport http --host 127.0.0.1 --port "${PORT_HTTP}" --paths "${FIXTURES}" \
  >"${TMPDIR}/http.log" 2>&1 &
PID_HTTP=$!
wait_for_health "http://127.0.0.1:${PORT_HTTP}/healthz"
"${BIN}" http "http://127.0.0.1:${PORT_HTTP}/mcp" --list >"${TMPDIR}/http.txt"
grep -q "get_skill_details" "${TMPDIR}/http.txt"

echo "Smoke check: bearer-protected remote HTTP MCP flow"
SXMC_SMOKE_TOKEN="${BEARER_TOKEN}" \
  "${BIN}" serve --transport http --host 127.0.0.1 --port "${PORT_BEARER}" \
  --bearer-token env:SXMC_SMOKE_TOKEN --paths "${FIXTURES}" \
  >"${TMPDIR}/bearer.log" 2>&1 &
PID_BEARER=$!
wait_for_health "http://127.0.0.1:${PORT_BEARER}/healthz"
"${BIN}" http "http://127.0.0.1:${PORT_BEARER}/mcp" \
  --auth-header "Authorization: Bearer ${BEARER_TOKEN}" --list \
  >"${TMPDIR}/bearer.txt"
grep -q "get_skill_related_file" "${TMPDIR}/bearer.txt"

echo "Client smoke checks passed."
