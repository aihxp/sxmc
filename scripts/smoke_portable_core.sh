#!/usr/bin/env bash
set -euo pipefail

BIN="${1:-target/debug/sxmc}"
ROOT="${2:-.}"
TMPDIR="$(mktemp -d)"

cleanup() {
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

echo "Portable smoke: binary basics"
"${BIN}" --version >/dev/null
"${BIN}" --help >/dev/null

echo "Portable smoke: doctor JSON"
"${BIN}" doctor --format json >"${TMPDIR}/doctor.json"
json_check "${TMPDIR}/doctor.json" "'root' in d and 'recommended_first_moves' in d"

echo "Portable smoke: compact CLI inspection"
"${BIN}" inspect cli cargo --compact --format json >"${TMPDIR}/cargo-compact.json"
json_check "${TMPDIR}/cargo-compact.json" "d['command'] == 'cargo' and 'subcommand_count' in d"

echo "Portable smoke: codebase discovery"
"${BIN}" discover codebase "${ROOT}" --compact --format json >"${TMPDIR}/codebase.json"
json_check "${TMPDIR}/codebase.json" "d['source_type'] == 'codebase' and d['config_count'] >= 1"

echo "Portable smoke: traffic discovery and manifest scaffolding"
cat >"${TMPDIR}/curl-history.txt" <<'EOF'
curl https://api.example.test/v1/widgets
curl -H 'Content-Type: application/json' -d '{"name":"sumac"}' https://api.example.test/v1/widgets
EOF
"${BIN}" discover traffic "${TMPDIR}/curl-history.txt" --output "${TMPDIR}/traffic.json" --format json >/dev/null
json_check "${TMPDIR}/traffic.json" "d['source_type'] == 'traffic' and d['capture_kind'] == 'curl' and d['endpoint_count'] >= 1"
"${BIN}" scaffold discovery-tools --from-snapshot "${TMPDIR}/traffic.json" --root "${TMPDIR}" --mode apply >/dev/null

MANIFEST_DIR="${TMPDIR}/.sxmc/discovery-tools"
if [[ ! -f "${MANIFEST_DIR}/traffic-traffic.json" ]]; then
  echo "expected discovery tool manifest at ${MANIFEST_DIR}/traffic-traffic.json" >&2
  exit 1
fi

SPEC="$("$PYTHON_BIN" - <<PY
import json
print(json.dumps(["${BIN}", "serve", "--discovery-tool-manifest", "${MANIFEST_DIR}"]))
PY
)"

"${BIN}" stdio "${SPEC}" --list-tools >"${TMPDIR}/tool-list.txt"
grep -q "discovery__traffic" "${TMPDIR}/tool-list.txt"

TOOL_NAME="$(MANIFEST_PATH="${MANIFEST_DIR}/traffic-traffic.json" "$PYTHON_BIN" - <<'PY'
import json
import os
from pathlib import Path

manifest = json.loads(Path(os.environ["MANIFEST_PATH"]).read_text())
name = manifest["generated_tools"][0]["name"]
print(f"discovery__{name}")
PY
)"

"${BIN}" stdio "${SPEC}" "${TOOL_NAME}" --pretty >"${TMPDIR}/tool-call.json"
json_check "${TMPDIR}/tool-call.json" "d['kind'] == 'traffic-endpoint' and d['source_type'] == 'traffic'"

echo "Portable smoke checks passed."
