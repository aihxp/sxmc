#!/usr/bin/env bash
set -euo pipefail

BIN="${1:-target/debug/sxmc}"
FIXTURES="${2:-tests/fixtures}"
PORT_HTTP="${SXMC_PORTABLE_FIXTURE_HTTP_PORT:-38180}"
PORT_BEARER="${SXMC_PORTABLE_FIXTURE_BEARER_PORT:-38181}"
BEARER_TOKEN="${SXMC_PORTABLE_FIXTURE_TOKEN:-sxmc-portable-fixture-token}"
TMPDIR="$(mktemp -d)"
PID_HTTP=""
PID_BEARER=""
BAKE_NAME="portable-fixture-mcp-$(basename "${TMPDIR}")"

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

pick_python() {
  if command -v python3 >/dev/null 2>&1; then
    command -v python3
    return
  fi
  if command -v python >/dev/null 2>&1; then
    command -v python
    return
  fi
  echo "python3 or python is required for ${0}" >&2
  exit 1
}

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

PYTHON_BIN="$(pick_python)"

json_check() {
  local file="$1"
  local expr="$2"
  JSON_PATH="$file" JSON_EXPR="$expr" "$PYTHON_BIN" - <<'PY'
import json
import os
from pathlib import Path

value = json.loads(Path(os.environ["JSON_PATH"]).read_text())
ok = bool(eval(os.environ["JSON_EXPR"], {"__builtins__": {}}, {"d": value}))
raise SystemExit(0 if ok else 1)
PY
}

echo "Portable fixture smoke: skill discovery"
"${BIN}" skills list --paths "${FIXTURES}" --json >"${TMPDIR}/skills.json"
json_check "${TMPDIR}/skills.json" "d[0] is not None and d[1] is not None and d[2] is not None and d[3] is not None and [item['name'] for item in d].count('simple-skill') >= 1"

SPEC="$("$PYTHON_BIN" - <<PY
import json
print(json.dumps(["${BIN}", "serve", "--paths", "${FIXTURES}"]))
PY
)"

echo "Portable fixture smoke: stdio MCP flow"
"${BIN}" stdio "${SPEC}" --list >"${TMPDIR}/stdio-list.txt"
grep -q "get_available_skills" "${TMPDIR}/stdio-list.txt"
"${BIN}" stdio "${SPEC}" --prompt simple-skill arguments=portable >"${TMPDIR}/stdio-prompt.txt"
grep -q "Hello portable, welcome to sxmc!" "${TMPDIR}/stdio-prompt.txt"
"${BIN}" stdio "${SPEC}" --resource "skill://skill-with-references/references/style-guide.md" \
  >"${TMPDIR}/stdio-resource.txt"
grep -q "# Style Guide" "${TMPDIR}/stdio-resource.txt"

echo "Portable fixture smoke: baked MCP flow"
"${BIN}" bake create "${BAKE_NAME}" --source "${SPEC}" >/dev/null
"${BIN}" mcp tools "${BAKE_NAME}" >"${TMPDIR}/mcp-tools.txt"
grep -q "get_skill_details" "${TMPDIR}/mcp-tools.txt"
"${BIN}" mcp call "${BAKE_NAME}/get_skill_details" '{"name":"simple-skill"}' --pretty \
  >"${TMPDIR}/mcp-call.json"
grep -q "simple-skill" "${TMPDIR}/mcp-call.json"
"${BIN}" bake remove "${BAKE_NAME}" >/dev/null

echo "Portable fixture smoke: HTTP MCP flow"
"${BIN}" serve --transport http --host 127.0.0.1 --port "${PORT_HTTP}" --paths "${FIXTURES}" \
  >"${TMPDIR}/http.log" 2>&1 &
PID_HTTP=$!
wait_for_health "http://127.0.0.1:${PORT_HTTP}/healthz"
"${BIN}" http "http://127.0.0.1:${PORT_HTTP}/mcp" --list >"${TMPDIR}/http-list.txt"
grep -q "get_skill_details" "${TMPDIR}/http-list.txt"
"${BIN}" http "http://127.0.0.1:${PORT_HTTP}/mcp" --prompt simple-skill arguments=portable \
  >"${TMPDIR}/http-prompt.txt"
grep -q "Hello portable, welcome to sxmc!" "${TMPDIR}/http-prompt.txt"

echo "Portable fixture smoke: bearer-protected HTTP MCP flow"
SXMC_PORTABLE_FIXTURE_TOKEN="${BEARER_TOKEN}" \
  "${BIN}" serve --transport http --host 127.0.0.1 --port "${PORT_BEARER}" \
  --bearer-token env:SXMC_PORTABLE_FIXTURE_TOKEN --paths "${FIXTURES}" \
  >"${TMPDIR}/bearer.log" 2>&1 &
PID_BEARER=$!
wait_for_health "http://127.0.0.1:${PORT_BEARER}/healthz"
"${BIN}" http "http://127.0.0.1:${PORT_BEARER}/mcp" \
  --auth-header "Authorization: Bearer ${BEARER_TOKEN}" --list >"${TMPDIR}/bearer-list.txt"
grep -q "get_skill_related_file" "${TMPDIR}/bearer-list.txt"

echo "Portable fixture smoke checks passed."
