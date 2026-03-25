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

tool_name_from_manifest() {
  local manifest_path="$1"
  MANIFEST_PATH="$manifest_path" python3 - <<'PY'
import json
import os
from pathlib import Path

manifest = json.loads(Path(os.environ["MANIFEST_PATH"]).read_text())
name = manifest["generated_tools"][0]["name"]
print(f"discovery__{name}")
PY
}

echo "Smoke check: discovery-tool manifests"
printf '%s\n' "curl https://api.example.test/v1/widgets" >"${TMPDIR}/curl-history.txt"
"${BIN}" discover traffic "${TMPDIR}/curl-history.txt" --output "${TMPDIR}/traffic.json" --format json >/dev/null
"${BIN}" scaffold discovery-tools --from-snapshot "${TMPDIR}/traffic.json" --root "${TMPDIR}" --mode apply >/dev/null
DISCOVERY_MANIFEST_DIR="${TMPDIR}/.sxmc/discovery-tools"
DISCOVERY_MANIFEST_PATH="${DISCOVERY_MANIFEST_DIR}/traffic-traffic.json"
if [[ ! -f "${DISCOVERY_MANIFEST_PATH}" ]]; then
  echo "expected discovery tool manifest at ${DISCOVERY_MANIFEST_PATH}" >&2
  exit 1
fi
DISCOVERY_TOOL_NAME="$(tool_name_from_manifest "${DISCOVERY_MANIFEST_PATH}")"

echo "Smoke check: Codex/Cursor/Gemini-style stdio flow"
"${BIN}" stdio "${BIN} serve --paths ${FIXTURES} --discovery-tool-manifest ${DISCOVERY_MANIFEST_DIR}" --list >"${TMPDIR}/stdio.txt"
grep -q "get_available_skills" "${TMPDIR}/stdio.txt"
grep -q "skill_with_scripts__hello" "${TMPDIR}/stdio.txt"
"${BIN}" stdio "${BIN} serve --paths ${FIXTURES} --discovery-tool-manifest ${DISCOVERY_MANIFEST_DIR}" --list-tools >"${TMPDIR}/stdio-tools.txt"
grep -q "${DISCOVERY_TOOL_NAME}" "${TMPDIR}/stdio-tools.txt"
"${BIN}" stdio "${BIN} serve --paths ${FIXTURES}" --prompt simple-skill arguments=smoke \
  >"${TMPDIR}/stdio-prompt.txt"
grep -q "Hello smoke, welcome to sxmc!" "${TMPDIR}/stdio-prompt.txt"
"${BIN}" stdio "${BIN} serve --paths ${FIXTURES}" --resource \
  "skill://skill-with-references/references/style-guide.md" \
  >"${TMPDIR}/stdio-resource.txt"
grep -q "# Style Guide" "${TMPDIR}/stdio-resource.txt"
"${BIN}" stdio "${BIN} serve --paths ${FIXTURES} --discovery-tool-manifest ${DISCOVERY_MANIFEST_DIR}" \
  "${DISCOVERY_TOOL_NAME}" --pretty >"${TMPDIR}/stdio-tool.json"
grep -q '"source_type": "traffic"' "${TMPDIR}/stdio-tool.json"

echo "Smoke check: remote HTTP MCP flow"
"${BIN}" serve --transport http --host 127.0.0.1 --port "${PORT_HTTP}" --paths "${FIXTURES}" \
  --discovery-tool-manifest "${DISCOVERY_MANIFEST_DIR}" \
  >"${TMPDIR}/http.log" 2>&1 &
PID_HTTP=$!
wait_for_health "http://127.0.0.1:${PORT_HTTP}/healthz"
"${BIN}" http "http://127.0.0.1:${PORT_HTTP}/mcp" --list >"${TMPDIR}/http.txt"
grep -q "get_skill_details" "${TMPDIR}/http.txt"
"${BIN}" http "http://127.0.0.1:${PORT_HTTP}/mcp" --list-tools >"${TMPDIR}/http-tools.txt"
grep -q "${DISCOVERY_TOOL_NAME}" "${TMPDIR}/http-tools.txt"
"${BIN}" http "http://127.0.0.1:${PORT_HTTP}/mcp" --prompt simple-skill arguments=smoke \
  >"${TMPDIR}/http-prompt.txt"
grep -q "Hello smoke, welcome to sxmc!" "${TMPDIR}/http-prompt.txt"
"${BIN}" http "http://127.0.0.1:${PORT_HTTP}/mcp" --resource \
  "skill://skill-with-references/references/style-guide.md" \
  >"${TMPDIR}/http-resource.txt"
grep -q "# Style Guide" "${TMPDIR}/http-resource.txt"
"${BIN}" http "http://127.0.0.1:${PORT_HTTP}/mcp" "${DISCOVERY_TOOL_NAME}" --pretty \
  >"${TMPDIR}/http-tool.json"
grep -q '"source_type": "traffic"' "${TMPDIR}/http-tool.json"

echo "Smoke check: bearer-protected remote HTTP MCP flow"
SXMC_SMOKE_TOKEN="${BEARER_TOKEN}" \
  "${BIN}" serve --transport http --host 127.0.0.1 --port "${PORT_BEARER}" \
  --bearer-token env:SXMC_SMOKE_TOKEN --paths "${FIXTURES}" \
  --discovery-tool-manifest "${DISCOVERY_MANIFEST_DIR}" \
  >"${TMPDIR}/bearer.log" 2>&1 &
PID_BEARER=$!
wait_for_health "http://127.0.0.1:${PORT_BEARER}/healthz"
"${BIN}" http "http://127.0.0.1:${PORT_BEARER}/mcp" \
  --auth-header "Authorization: Bearer ${BEARER_TOKEN}" --list \
  >"${TMPDIR}/bearer.txt"
grep -q "get_skill_related_file" "${TMPDIR}/bearer.txt"
"${BIN}" http "http://127.0.0.1:${PORT_BEARER}/mcp" \
  --auth-header "Authorization: Bearer ${BEARER_TOKEN}" --list-tools \
  >"${TMPDIR}/bearer-tools.txt"
grep -q "${DISCOVERY_TOOL_NAME}" "${TMPDIR}/bearer-tools.txt"

echo "Client smoke checks passed."
