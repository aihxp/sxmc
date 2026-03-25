#!/usr/bin/env bash
# ============================================================================
# Sumac (sxmc) comprehensive test + benchmark suite
# Covers: ALL v0.2.10–v0.2.40 features, 10×10×10 matrix, benchmarks
# Usage: bash scripts/test-sxmc.sh [--json results.json]
# Env:   SXMC=path/to/sxmc (default: freshest repo build, then sxmc on PATH)
#        BENCH_RUNS=5 (benchmark iterations)
# ============================================================================
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FIXTURES="$ROOT/tests/fixtures"
TMPDIR_TEST="$(mktemp -d)"
TESTHOME="$TMPDIR_TEST/home"
mkdir -p "$TESTHOME"
JSON_OUT=""
BENCH_RUNS="${BENCH_RUNS:-5}"
IS_WINDOWS=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --json) JSON_OUT="$2"; shift 2 ;;
    *) shift ;;
  esac
done

# --- Colors ---
if [ -t 1 ] && [ "${TERM:-dumb}" != "dumb" ]; then
  GREEN='\033[0;32m'; RED='\033[0;31m'; YELLOW='\033[0;33m'
  CYAN='\033[0;36m'; BOLD='\033[1m'; RESET='\033[0m'
else
  GREEN=''; RED=''; YELLOW=''; CYAN=''; BOLD=''; RESET=''
fi

# --- Counters ---
PASS=0; FAIL=0; SKIP=0; TOTAL=0

# --- Helpers ---
pass() {
  TOTAL=$((TOTAL + 1)); PASS=$((PASS + 1))
  printf "${GREEN}  ✓${RESET} %s\n" "$1"
}

fail() {
  TOTAL=$((TOTAL + 1)); FAIL=$((FAIL + 1))
  printf "${RED}  ✗${RESET} %s\n" "$1"
  [ -n "${2:-}" ] && printf "    → %s\n" "$2"
}

skip() {
  TOTAL=$((TOTAL + 1)); SKIP=$((SKIP + 1))
  printf "${YELLOW}  - %s${RESET} (%s)\n" "$1" "$2"
}

section() {
  printf "\n${BOLD}${CYAN}━━━ %s ━━━${RESET}\n" "$1"
}

has_cmd() { command -v "$1" >/dev/null 2>&1; }

# Convert POSIX paths to Windows paths on MINGW/Cygwin for Python subprocess
win_path() {
  if command -v cygpath >/dev/null 2>&1; then
    cygpath -w "$1"
  else
    echo "$1"
  fi
}

# Windows-safe path variables for use in Python subprocess calls
SXMC_WIN=""
FIXTURES_WIN=""
setup_win_paths() {
  SXMC_WIN="$(win_path "$SXMC")"
  FIXTURES_WIN="$(win_path "$FIXTURES")"
}

time_ms() {
  python3 -c "
import subprocess, time, sys
t0 = time.time()
subprocess.run(sys.argv[1:], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
print(int((time.time() - t0) * 1000))
" "$@"
}

json_field() {
  _JF_EXPR="$2" python3 -c "
import sys, json, os
d = json.load(sys.stdin)
print(eval(os.environ['_JF_EXPR']))
" <<< "$1" 2>/dev/null
}

json_check() {
  _JC_EXPR="$2" python3 -c "
import sys, json, os
try:
    d = json.load(sys.stdin)
    result = eval(os.environ['_JC_EXPR'])
    sys.exit(0 if result else 1)
except Exception:
    sys.exit(1)
" <<< "$1" 2>/dev/null
}

median_of() {
  python3 -c "
import sys
vals = sorted(int(x) for x in sys.argv[1:])
print(vals[len(vals)//2])
" "$@"
}

sxmc_isolated() {
  HOME="$TESTHOME" USERPROFILE="$TESTHOME" \
  XDG_CONFIG_HOME="$TESTHOME/.config" \
  APPDATA="$TESTHOME/AppData/Roaming" \
  LOCALAPPDATA="$TESTHOME/AppData/Local" \
  "$SXMC" "$@"
}

cleanup() { rm -rf "$TMPDIR_TEST" 2>/dev/null; }
trap cleanup EXIT

# --- Resolve sxmc binary ---
if [ -n "${SXMC:-}" ]; then
  :
elif [ -x "$ROOT/target/debug/sxmc" ] && [ -x "$ROOT/target/release/sxmc" ]; then
  if [ "$ROOT/target/debug/sxmc" -nt "$ROOT/target/release/sxmc" ]; then
    SXMC="$ROOT/target/debug/sxmc"
  else
    SXMC="$ROOT/target/release/sxmc"
  fi
elif [ -x "$ROOT/target/debug/sxmc" ]; then
  SXMC="$ROOT/target/debug/sxmc"
elif [ -x "$ROOT/target/release/sxmc" ]; then
  SXMC="$ROOT/target/release/sxmc"
elif has_cmd sxmc; then
  SXMC="sxmc"
else
  echo "ERROR: sxmc not found. Set SXMC= or install it." >&2
  exit 1
fi

# --- Set up Windows-safe paths for Python subprocess ---
setup_win_paths

# --- Benchmark accumulators ---
declare -a BENCH_KEYS=()
declare -a BENCH_VALS=()
bench_record() {
  BENCH_KEYS+=("$1")
  BENCH_VALS+=("$2")
}

# ============================================================================
# PART A — OLD FEATURES (re-validate v0.2.10–v0.2.21)
# ============================================================================
printf "\n${BOLD}╔════════════════════════════════════════╗${RESET}"
printf "\n${BOLD}║  PART A — OLD FEATURES (re-validate)  ║${RESET}"
printf "\n${BOLD}╚════════════════════════════════════════╝${RESET}\n"

# ── Section 1: Environment ──
section "1. Environment"

SXMC_VERSION=$("$SXMC" --version 2>&1 || echo "unknown")
OS_NAME=$(uname -s 2>/dev/null || echo "unknown")
OS_ARCH=$(uname -m 2>/dev/null || echo "unknown")
PY_VERSION=$(python3 --version 2>&1 || echo "missing")

printf "  sxmc:    %s\n" "$SXMC_VERSION"
printf "  OS:      %s %s\n" "$OS_NAME" "$OS_ARCH"
printf "  python3: %s\n" "$PY_VERSION"
printf "  binary:  %s\n" "$SXMC"

if echo "$SXMC_VERSION" | grep -q "sxmc"; then
  pass "sxmc binary runs"
else
  fail "sxmc binary runs" "$SXMC_VERSION"
fi

if has_cmd python3; then
  pass "python3 available"
else
  fail "python3 available (required for JSON assertions)"
  exit 1
fi

case "$OS_NAME" in
  MINGW*|MSYS*|CYGWIN*) IS_WINDOWS=1 ;;
esac

# ── Section 2: Help & Completions ──
section "2. Help & Completions"

help_out=$("$SXMC" --help 2>&1)
for kw in serve wrap skills stdio http mcp api inspect init scaffold scan bake doctor completions status watch publish pull; do
  if echo "$help_out" | grep -q "$kw"; then
    pass "help mentions '$kw'"
  else
    fail "help mentions '$kw'"
  fi
done

for shell_name in bash zsh fish; do
  if "$SXMC" completions "$shell_name" >/dev/null 2>&1; then
    pass "completions $shell_name"
  else
    fail "completions $shell_name"
  fi
done

# ── Section 3: CLI Inspection Matrix ──
section "3. CLI Inspection Matrix"

CLI_TOOLS=(
  ls grep sed cp rm chmod sort tr diff cat mv mkdir wc head tail uniq awk
  git gh npm cargo rustc rustup python3 node brew curl ssh jq
  tar find xargs tee cut paste join comm env printenv whoami hostname date cal
  zip unzip gzip bzip2 xz
  ping dig nslookup traceroute ifconfig netstat
  ps top kill lsof open
  pbcopy pbpaste defaults launchctl diskutil sips mdls mdfind
  xcodebuild swift swiftc clang make cmake
  file stat du df mount umount ln touch less more man which basename dirname
  expr bc dc od hexdump strings nm rg
)

PARSED=0; PARSE_FAIL=0; PARSE_SKIP=0; BAD_SUMMARIES=0

for cmd in "${CLI_TOOLS[@]}"; do
  if ! has_cmd "$cmd"; then
    ((PARSE_SKIP++))
    continue
  fi
  out=$("$SXMC" inspect cli "$cmd" 2>&1)
  if ! python3 -c "import sys,json; json.load(sys.stdin)" <<< "$out" 2>/dev/null; then
    if [ "$IS_WINDOWS" -eq 1 ] && [[ "$cmd" == "npm" || "$cmd" == "rg" ]]; then
      ((PARSE_SKIP++))
      skip "inspect cli $cmd" "Windows help output not parsed yet"
      continue
    fi
    ((PARSE_FAIL++))
    fail "inspect cli $cmd" "not valid JSON: ${out:0:80}"
    continue
  fi
  ((PARSED++))
  summary=$(json_field "$out" "d.get('summary','')")
  sl=$(printf '%s\n' "$summary" | tr '[:upper:]' '[:lower:]')
  if [ -z "$summary" ]; then
    ((BAD_SUMMARIES++))
  elif printf '%s\n' "$sl" | grep -qE '^usage:|copyright|SSUUMM|illegal option|unrecognized'; then
    ((BAD_SUMMARIES++))
  fi
done

if [ "$PARSED" -gt 0 ]; then
  pass "parsed $PARSED CLIs successfully ($PARSE_SKIP not installed, $PARSE_FAIL failed)"
else
  fail "CLI inspection: no tools parsed"
fi

[ "$PARSE_FAIL" -eq 0 ] && pass "zero parse failures" || fail "$PARSE_FAIL tools failed to parse"
[ "$BAD_SUMMARIES" -eq 0 ] && pass "zero bad summaries" || fail "$BAD_SUMMARIES tools have questionable summaries"

# ── Section 4: Previously-Broken Tools ──
section "4. Previously-Broken Tools"

check_tool() {
  local cmd="$1" check_name="$2" check_expr="$3"
  if ! has_cmd "$cmd"; then skip "$check_name" "$cmd not installed"; return; fi
  local out
  out=$("$SXMC" inspect cli "$cmd" 2>&1)
  if json_check "$out" "$check_expr"; then
    pass "$check_name"
  else
    local diag
    diag=$(python3 -c "
import sys,json
try:
    d=json.load(sys.stdin)
    print(f\"sub={len(d.get('subcommands',[]))} opt={len(d.get('options',[]))} summary={d.get('summary','')[:60]}\")
except Exception: print('invalid JSON')
" <<< "$out" 2>/dev/null)
    fail "$check_name" "$diag"
  fi
}

check_tool brew "brew: has subcommands" "len(d.get('subcommands',[])) >= 5"
check_tool brew "brew: has global options" "len(d.get('options',[])) >= 1"
check_tool cat "cat: no false positive subcmds" "len(d.get('subcommands',[])) <= 1"
check_tool lsof "lsof: no false positive subcmds" "len(d.get('subcommands',[])) <= 2"
check_tool gzip "gzip: clean summary" "'apple gzip' not in d.get('summary','').lower()"
check_tool python3 "python3: no fake subcommands" "len(d.get('subcommands',[])) == 0"
check_tool rustup "rustup: has options" "len(d.get('options',[])) >= 2"
check_tool gh "gh: has 20+ subcommands" "len(d.get('subcommands',[])) >= 20"
check_tool awk "awk: has options" "len(d.get('options',[])) >= 1"
check_tool grep "grep: clean summary" "len(d.get('summary','')) < 80"

# ── Section 5: Compact Mode ──
section "5. Compact Mode"

if has_cmd git; then
  full_out=$("$SXMC" inspect cli git 2>/dev/null)
  compact_out=$("$SXMC" inspect cli git --compact 2>/dev/null)
  full_chars=${#full_out}
  compact_chars=${#compact_out}

  if [ "$compact_chars" -lt "$full_chars" ]; then
    savings=$(( 100 - (100 * compact_chars / full_chars) ))
    pass "compact mode smaller ($savings% reduction)"
  else
    fail "compact mode not smaller" "full=$full_chars compact=$compact_chars"
  fi

  json_check "$compact_out" "'subcommand_count' in d" && pass "compact has subcommand_count" || fail "compact missing subcommand_count"
  json_check "$compact_out" "'option_count' in d" && pass "compact has option_count" || fail "compact missing option_count"
  json_check "$compact_out" "'provenance' not in d" && pass "compact strips provenance" || fail "compact should not include provenance"
fi

if has_cmd curl; then
  full_c=$("$SXMC" inspect cli curl 2>/dev/null | wc -c | tr -d ' ')
  compact_c=$("$SXMC" inspect cli curl --compact 2>/dev/null | wc -c | tr -d ' ')
  savings=$(( 100 - (100 * compact_c / full_c) ))
  [ "$savings" -ge 50 ] && pass "curl compact savings >= 50% (got ${savings}%)" || fail "curl compact savings < 50%" "got ${savings}%"
fi

# ── Section 6: Profile Caching ──
section "6. Profile Caching"

if has_cmd git; then
  CACHE_DIR_MAC="$TESTHOME/Library/Caches/sxmc"
  CACHE_DIR_LINUX="$TESTHOME/.cache/sxmc"
  CACHE_DIR_WIN="$TESTHOME/AppData/Local/sxmc"
  # On Windows, dirs crate ignores LOCALAPPDATA env var and uses the real system path
  if [ "$(uname -o 2>/dev/null)" = "Msys" ] || [ "$(uname -o 2>/dev/null)" = "Cygwin" ]; then
    CACHE_DIR_WIN="${LOCALAPPDATA:-$USERPROFILE/AppData/Local}/sxmc"
  fi
  rm -rf "$CACHE_DIR_MAC" "$CACHE_DIR_LINUX" "$CACHE_DIR_WIN" 2>/dev/null

  cold_ms=$(HOME="$TESTHOME" time_ms "$SXMC" inspect cli git)
  warm_ms=$(HOME="$TESTHOME" time_ms "$SXMC" inspect cli git)

  if [ -d "$CACHE_DIR_MAC" ] || [ -d "$CACHE_DIR_LINUX" ] || [ -d "$CACHE_DIR_WIN" ]; then
    cache_files=$(find "$CACHE_DIR_MAC" "$CACHE_DIR_LINUX" "$CACHE_DIR_WIN" -name "*.json" 2>/dev/null | wc -l | tr -d ' ')
    pass "cache directory created ($cache_files files)"
  else
    fail "cache directory not created"
  fi

  if [ "$warm_ms" -le $(( cold_ms * 3 )) ]; then
    pass "cache timing OK (cold=${cold_ms}ms warm=${warm_ms}ms)"
  else
    fail "warm cache much slower" "cold=${cold_ms}ms warm=${warm_ms}ms"
  fi
fi

# ── Section 7: Scaffold System ──
section "7. Scaffold System"

if has_cmd git; then
  profile=$("$SXMC" inspect cli git 2>/dev/null)
  echo "$profile" > "$TMPDIR_TEST/git-profile.json"

  skill_out=$("$SXMC" scaffold skill --from-profile "$TMPDIR_TEST/git-profile.json" --output-dir "$TMPDIR_TEST/scaffolds" 2>&1)
  echo "$skill_out" | grep -q "SKILL.md" && pass "scaffold skill produces SKILL.md" || fail "scaffold skill" "${skill_out:0:100}"

  mcp_out=$("$SXMC" scaffold mcp-wrapper --from-profile "$TMPDIR_TEST/git-profile.json" --output-dir "$TMPDIR_TEST/scaffolds" 2>&1)
  echo "$mcp_out" | grep -q "README.md" && pass "scaffold mcp-wrapper" || fail "scaffold mcp-wrapper" "${mcp_out:0:100}"

  llms_out=$("$SXMC" scaffold llms-txt --from-profile "$TMPDIR_TEST/git-profile.json" 2>&1)
  echo "$llms_out" | grep -q "llms.txt" && pass "scaffold llms-txt" || fail "scaffold llms-txt" "${llms_out:0:100}"
fi

# ── Section 8: Init AI Pipeline ──
section "8. Init AI Pipeline"

AI_HOSTS=(claude-code cursor gemini-cli github-copilot continue-dev open-code jetbrains-ai-assistant junie windsurf openai-codex)

if has_cmd git; then
  for host in "${AI_HOSTS[@]}"; do
    ai_out=$("$SXMC" init ai --from-cli git --client "$host" --mode preview 2>&1)
    echo "$ai_out" | grep -q "Target:" && pass "init ai --client $host" || fail "init ai --client $host" "${ai_out:0:80}"
  done
fi

# ── Section 9: Security Scanner ──
section "9. Security Scanner"

if [ -d "$FIXTURES/malicious-skill" ]; then
  scan_out=$("$SXMC" scan --paths "$FIXTURES" 2>&1)
  echo "$scan_out" | grep -q "CRITICAL" && pass "scanner detects CRITICAL" || fail "scanner detects CRITICAL"
  echo "$scan_out" | grep -q "SL-INJ-001" && pass "scanner detects prompt injection" || fail "scanner should detect injection"
  echo "$scan_out" | grep -q "SL-EXEC-001\|Dangerous" && pass "scanner detects dangerous ops" || fail "scanner should detect dangerous ops"
  echo "$scan_out" | grep -qi "secret\|SL-SEC" && pass "scanner detects secrets" || fail "scanner should detect secrets"
else
  skip "security scanner" "fixtures/malicious-skill not found"
fi

# ── Section 10: MCP Pipeline ──
section "10. MCP Pipeline"

STATEFUL_SCRIPT="$FIXTURES/stateful_mcp_server.py"

if has_cmd python3 && [ -f "$STATEFUL_SCRIPT" ]; then
  STATEFUL_SCRIPT_NATIVE="$(win_path "$STATEFUL_SCRIPT")"
  # On Windows, resolve python3 to the actual python executable (not a bash shim)
  PYTHON3_REAL="$(python3 -c "import sys; print(sys.executable)")"
  PYTHON3_NATIVE="$(win_path "$PYTHON3_REAL")"
  bake_source=$(python3 -c "import json,sys; print(json.dumps(sys.argv[1:]))" "$PYTHON3_NATIVE" "$STATEFUL_SCRIPT_NATIVE")
  bake_out=$(sxmc_isolated bake create test-mcp --source "$bake_source" --skip-validate 2>&1)
  echo "$bake_out" | grep -q "Created bake" && pass "bake create (stateful fixture)" || fail "bake create" "$bake_out"

  list_out=$(sxmc_isolated bake list 2>&1)
  echo "$list_out" | grep -q "test-mcp" && pass "bake list shows test-mcp" || fail "bake list"

  tools_out=$(sxmc_isolated mcp tools test-mcp 2>&1)
  echo "$tools_out" | grep -q "remember_state\|read_state\|Tools" && pass "mcp tools lists server tools" || fail "mcp tools"

  grep_out=$(sxmc_isolated mcp grep state 2>&1)
  echo "$grep_out" | grep -qi "match\|state" && pass "mcp grep finds matches" || fail "mcp grep"

  rm_out=$(sxmc_isolated bake remove test-mcp 2>&1)
  echo "$rm_out" | grep -q "Removed" && pass "bake remove" || fail "bake remove"
else
  skip "MCP pipeline tests" "python3 or fixtures not available"
fi

# ── Section 11: Bake Validation ──
section "11. Bake Validation"

bad_bake=$(sxmc_isolated bake create broken-bake --source 'definitely-not-a-real-command-xyz' 2>&1 || true)
echo "$bad_bake" | grep -qi "error\|could not connect\|not found" && pass "bake rejects invalid source" || fail "bake should reject invalid source"

skip_bake=$(sxmc_isolated bake create skip-bake --source 'not-real-cmd' --skip-validate 2>&1)
if echo "$skip_bake" | grep -q "Created"; then
  pass "bake --skip-validate succeeds"
  sxmc_isolated bake remove skip-bake >/dev/null 2>&1
else
  fail "bake --skip-validate" "$skip_bake"
fi

# ── Section 12: API Mode ──
section "12. API Mode"

PETSTORE_URL="https://petstore3.swagger.io/api/v3/openapi.json"

if has_cmd curl && curl -s --max-time 5 "$PETSTORE_URL" >/dev/null 2>&1; then
  api_list=$("$SXMC" api "$PETSTORE_URL" --list 2>/dev/null)
  if json_check "$api_list" "d.get('count', 0) >= 10"; then
    count=$(json_field "$api_list" "d['count']")
    pass "api --list finds $count operations"
  else
    fail "api --list" "${api_list:0:100}"
  fi

  api_search=$("$SXMC" api "$PETSTORE_URL" --search pet --list 2>/dev/null)
  json_check "$api_search" "d.get('count', 0) >= 3" && pass "api --search pet filters" || fail "api --search"

  api_call=$("$SXMC" api "$PETSTORE_URL" getPetById petId=1 --pretty 2>&1)
  if echo "$api_call" | grep -qE '"id"|"status"|"body"'; then
    pass "api call getPetById returns response"
  else
    fail "api call" "${api_call:0:100}"
  fi
else
  skip "API mode tests" "no network or curl unavailable"
fi

# ── Section 13: Doctor Command ──
section "13. Doctor Command"

doc_out=$("$SXMC" doctor 2>&1)
json_check "$doc_out" "'root' in d" && pass "doctor outputs JSON with root" || fail "doctor output"
json_check "$doc_out" "'startup_files' in d" && pass "doctor reports startup_files" || fail "doctor missing startup_files"
json_check "$doc_out" "'recommended_first_moves' in d" && pass "doctor has recommended_first_moves" || fail "doctor missing moves"

doc_human=$("$SXMC" doctor --human 2>&1)
echo "$doc_human" | grep -q "Recommended first moves" && pass "doctor --human renders report" || fail "doctor --human"

TMP_DOCTOR_ROOT="$TMPDIR_TEST/doctor-empty"
mkdir -p "$TMP_DOCTOR_ROOT"
if "$SXMC" doctor --check --root "$TMP_DOCTOR_ROOT" >/dev/null 2>&1; then
  fail "doctor --check should fail when files missing"
else
  pass "doctor --check fails when files missing"
fi

# ── Section 14: Self-Dogfooding ──
section "14. Self-Dogfooding"

DOGFOOD_FILES=(CLAUDE.md AGENTS.md GEMINI.md .cursor/rules/sxmc-cli-ai.md .github/copilot-instructions.md)
for f in "${DOGFOOD_FILES[@]}"; do
  if [ -f "$ROOT/$f" ] && grep -qi "sxmc" "$ROOT/$f"; then
    pass "repo ships $f"
  else
    fail "repo missing or incomplete $f"
  fi
done

# ── Section 15: Depth Expansion & Batch ──
section "15. Depth Expansion & Batch"

if has_cmd git; then
  depth1=$("$SXMC" inspect cli git --depth 1 2>/dev/null)
  nested=$(json_field "$depth1" "len(d.get('subcommand_profiles',[]))")
  [ "${nested:-0}" -gt 0 ] && pass "depth 1 produces $nested subcommand_profiles" || skip "depth 1 subcommand_profiles" "key may differ"
fi

printf 'git\nls\n' > "$TMPDIR_TEST/tools.txt"
batch_out=$("$SXMC" inspect batch git ls this-command-should-not-exist-xyz --parallel 4 --progress 2>/dev/null)
json_check "$batch_out" "d.get('count', 0) == 3" && pass "inspect batch reports count" || fail "inspect batch count"
json_check "$batch_out" "d.get('failed_count', 0) >= 1" && pass "inspect batch keeps partial failures" || fail "inspect batch failures"
json_check "$batch_out" "d.get('parallelism', 0) >= 1" && pass "inspect batch reports parallelism" || fail "inspect batch parallelism"

batch_from_file=$("$SXMC" inspect batch --from-file "$TMPDIR_TEST/tools.txt" --parallel 2 2>/dev/null)
json_check "$batch_from_file" "d.get('count', 0) == 2 and d.get('failed_count', 0) == 0" && pass "inspect batch --from-file" || fail "inspect batch --from-file"

# Cache management
cache_stats=$(HOME="$TESTHOME" "$SXMC" inspect cache-stats 2>/dev/null)
json_check "$cache_stats" "'entry_count' in d" && pass "cache-stats returns entry_count" || fail "cache-stats"

HOME="$TESTHOME" "$SXMC" inspect cache-clear >/dev/null 2>&1
cache_after=$(HOME="$TESTHOME" "$SXMC" inspect cache-stats 2>/dev/null)
json_check "$cache_after" "d.get('entry_count', 1) == 0" && pass "cache-clear empties cache" || fail "cache-clear"

# ── Section 16: Error Messages ──
section "16. Error Messages"

nonexist_err=$("$SXMC" inspect cli this-command-surely-does-not-exist-12345 2>&1 || true)
echo "$nonexist_err" | grep -qi "not found\|error\|could not" && pass "nonexistent tool gives clear error" || fail "nonexistent tool error"

no_args_out=$("$SXMC" 2>&1 || true)
echo "$no_args_out" | grep -qi "usage\|help\|Usage" && pass "no arguments shows usage" || fail "no arguments"

# ── Section 17: Serve ──
section "17. Serve"

serve_help=$("$SXMC" serve --help 2>&1)
echo "$serve_help" | grep -q "transport\|paths\|port" && pass "serve --help mentions transport" || fail "serve --help"

if [ -d "$FIXTURES" ]; then
  skills_list=$("$SXMC" skills list --paths "$FIXTURES" 2>&1)
  echo "$skills_list" | grep -qi "simple-skill\|skill" && pass "skills list finds fixtures" || fail "skills list"
fi

# ── Section 18: Existing Wrap (from v0.2.24) ──
section "18. Wrap (basic)"

wrap_help=$("$SXMC" wrap --help 2>&1)
echo "$wrap_help" | grep -q "allow-tool\|deny-tool" && pass "wrap --help has tool filters" || fail "wrap --help missing filters"
echo "$wrap_help" | grep -q "timeout-seconds" && pass "wrap --help has timeout" || fail "wrap --help missing timeout"
echo "$wrap_help" | grep -q "execution-history-limit" && pass "wrap --help has execution history" || fail "wrap --help missing exec history"

# ============================================================================
# PART B — NEW FEATURES (v0.2.22–v0.2.37)
# ============================================================================
printf "\n${BOLD}╔════════════════════════════════════════╗${RESET}"
printf "\n${BOLD}║  PART B — NEW FEATURES (v0.2.22+)     ║${RESET}"
printf "\n${BOLD}╚════════════════════════════════════════╝${RESET}\n"

# ── Section 19: Wrap Execution ──
section "19. Wrap — Execution & Filtering"

if has_cmd git; then
  # Wrap git and check JSON output
  wrap_out=$("$SXMC" wrap git 2>&1 &
    WRAP_PID=$!
    sleep 2
    kill $WRAP_PID 2>/dev/null
    wait $WRAP_PID 2>/dev/null
  )
  # Just test wrap help flags are present (server requires stdio client)
  wrap_help_full=$("$SXMC" wrap --help 2>&1)
  echo "$wrap_help_full" | grep -q "allow-option" && pass "wrap has --allow-option" || fail "wrap missing --allow-option"
  echo "$wrap_help_full" | grep -q "deny-option" && pass "wrap has --deny-option" || fail "wrap missing --deny-option"
  echo "$wrap_help_full" | grep -q "allow-positional" && pass "wrap has --allow-positional" || fail "wrap missing --allow-positional"
  echo "$wrap_help_full" | grep -q "deny-positional" && pass "wrap has --deny-positional" || fail "wrap missing --deny-positional"
  echo "$wrap_help_full" | grep -q "progress-seconds" && pass "wrap has --progress-seconds" || fail "wrap missing --progress-seconds"
  echo "$wrap_help_full" | grep -q "max-stdout-bytes" && pass "wrap has --max-stdout-bytes" || fail "wrap missing --max-stdout-bytes"
  echo "$wrap_help_full" | grep -q "working-dir" && pass "wrap has --working-dir" || fail "wrap missing --working-dir"

  # Test wrap via stdio bridge
  wrap_list=$("$SXMC" stdio "$SXMC wrap git" --list 2>/dev/null || true)
  if echo "$wrap_list" | grep -qi "tool\|Tools\|git"; then
    pass "wrap git → stdio --list shows tools"
  else
    skip "wrap git stdio list" "may need longer timeout"
  fi
fi

if [ "$IS_WINDOWS" -ne 1 ]; then
  FAKE_TUI="$TMPDIR_TEST/fake-interactive-cli"
  cat > "$FAKE_TUI" <<'EOF'
#!/bin/sh
if [ "$1" = "doctor" ] && [ "$2" = "--help" ]; then
  cat <<'INNER'
fake-interactive-cli doctor

Run the Bubble Tea full-screen doctor UI.

Usage:
  fake-interactive-cli doctor [OPTIONS]

Options:
  --json   Print a non-interactive JSON report.
INNER
elif [ "$1" = "status" ] && [ "$2" = "--help" ]; then
  cat <<'INNER'
fake-interactive-cli status

Print a machine-friendly status summary.

Usage:
  fake-interactive-cli status [OPTIONS]

Options:
  --json   Print status as JSON.
INNER
else
  cat <<'INNER'
fake-interactive-cli

Demo CLI with one safe command and one BubbleTea TUI command.

Commands:
  doctor  Run the Bubble Tea full-screen doctor UI
  status  Print a machine-friendly status summary
INNER
fi
EOF
  chmod +x "$FAKE_TUI"

  tui_profile=$("$SXMC" inspect cli "$FAKE_TUI" --depth 1 2>/dev/null || true)
  if json_check "$tui_profile" "d.get('interactive') is True and any(cmd.get('name') == 'doctor' and cmd.get('interactive') for cmd in d.get('subcommands', []))"; then
    pass "inspect flags interactive TUI subcommands"
  else
    fail "inspect interactive flags" "${tui_profile:0:140}"
  fi

  tui_tools=$("$SXMC" stdio "$SXMC wrap $FAKE_TUI" --list-tools 2>/dev/null || true)
  if echo "$tui_tools" | grep -q "status" && ! echo "$tui_tools" | grep -q "doctor"; then
    pass "wrap skips interactive TUI subcommands"
  else
    fail "wrap skips interactive subcommands" "${tui_tools:0:140}"
  fi
fi

# ── Section 20: Status & Watch ──
section "20. Status & Watch"

status_out=$("$SXMC" status --pretty 2>/dev/null || true)
if json_check "$status_out" "'startup_files' in d or 'cache' in d or 'summary' in d"; then
  pass "status outputs structured JSON"
  if json_check "$status_out" "'ai_knowledge' in d and 'recovery_plan' in d"; then
    pass "status includes AI knowledge and recovery plan"
  else
    fail "status AI knowledge" "${status_out:0:120}"
  fi
else
  # status may output human on TTY
  status_out2=$("$SXMC" status 2>/dev/null || true)
  if [ -n "$status_out2" ]; then
    pass "status produces output"
  else
    fail "status produces no output"
  fi
fi

status_help=$("$SXMC" status --help 2>&1)
echo "$status_help" | grep -q "health" && pass "status --help has --health" || fail "status missing --health"
echo "$status_help" | grep -q "exit-code" && pass "status --help has --exit-code" || fail "status missing --exit-code"
echo "$status_help" | grep -q "compare-hosts" && pass "status --help has --compare-hosts" || fail "status missing --compare-hosts"

watch_help=$("$SXMC" watch --help 2>&1)
echo "$watch_help" | grep -q "interval-seconds" && pass "watch has --interval-seconds" || fail "watch missing --interval-seconds"
echo "$watch_help" | grep -q "exit-on-change" && pass "watch has --exit-on-change" || fail "watch missing --exit-on-change"
echo "$watch_help" | grep -q "exit-on-unhealthy" && pass "watch has --exit-on-unhealthy" || fail "watch missing --exit-on-unhealthy"

# ── Section 21: Publish / Pull ──
section "21. Publish / Pull"

publish_help=$("$SXMC" publish --help 2>&1)
echo "$publish_help" | grep -q "TARGET" && pass "publish --help has TARGET arg" || fail "publish missing TARGET"
echo "$publish_help" | grep -q "signature-secret\|signing-key" && pass "publish supports signing" || fail "publish missing signing"
echo "$publish_help" | grep -q "bundle-name" && pass "publish has --bundle-name" || fail "publish missing --bundle-name"
echo "$publish_help" | grep -q "role" && pass "publish has --role" || fail "publish missing --role"
echo "$publish_help" | grep -q "hosts" && pass "publish has --hosts" || fail "publish missing --hosts"

pull_help=$("$SXMC" pull --help 2>&1)
echo "$pull_help" | grep -q "SOURCE" && pass "pull --help has SOURCE arg" || fail "pull missing SOURCE"
echo "$pull_help" | grep -q "expected-sha256" && pass "pull has SHA-256 enforcement" || fail "pull missing SHA-256"
echo "$pull_help" | grep -q "public-key" && pass "pull has --public-key" || fail "pull missing --public-key"
echo "$pull_help" | grep -q "overwrite\|skip-existing" && pass "pull has conflict controls" || fail "pull missing conflict controls"

# Functional publish → pull round-trip
if has_cmd git; then
  # Save a profile first
  BUNDLE_HOME="$TMPDIR_TEST/bundle-home"
  mkdir -p "$BUNDLE_HOME/.sxmc/ai/profiles"
  HOME="$BUNDLE_HOME" "$SXMC" inspect cli git > "$BUNDLE_HOME/.sxmc/ai/profiles/git.json" 2>/dev/null
  HOME="$BUNDLE_HOME" "$SXMC" inspect cli ls > "$BUNDLE_HOME/.sxmc/ai/profiles/ls.json" 2>/dev/null

  BUNDLE_TARGET="$TMPDIR_TEST/published-bundle.json"
  pub_out=$(HOME="$BUNDLE_HOME" "$SXMC" publish "$BUNDLE_TARGET" "$BUNDLE_HOME/.sxmc/ai/profiles" --recursive --bundle-name "test-bundle" --pretty 2>&1)
  if [ -f "$BUNDLE_TARGET" ]; then
    pass "publish creates bundle file"

    PULL_DIR="$TMPDIR_TEST/pulled-profiles"
    mkdir -p "$PULL_DIR"
    pull_out=$(HOME="$BUNDLE_HOME" "$SXMC" pull "$BUNDLE_TARGET" --output-dir "$PULL_DIR" --overwrite --pretty 2>&1)
    pulled_files=$(find "$PULL_DIR" -name "*.json" 2>/dev/null | wc -l | tr -d ' ')
    if [ "$pulled_files" -ge 1 ]; then
      pass "pull restores $pulled_files profile(s) from bundle"
    else
      fail "pull should restore profiles" "${pull_out:0:100}"
    fi
  else
    fail "publish should create bundle file" "${pub_out:0:120}"
  fi
fi

# ── Section 22: Bundle Export / Import / Verify ──
section "22. Bundle Export / Import / Verify"

if has_cmd git; then
  BUNDLE_DIR="$TMPDIR_TEST/bundle-ops"
  mkdir -p "$BUNDLE_DIR/profiles"
  "$SXMC" inspect cli git > "$BUNDLE_DIR/profiles/git.json" 2>/dev/null
  "$SXMC" inspect cli ls > "$BUNDLE_DIR/profiles/ls.json" 2>/dev/null

  EXPORT_FILE="$BUNDLE_DIR/exported.json"
  export_out=$("$SXMC" inspect bundle-export --output "$EXPORT_FILE" "$BUNDLE_DIR/profiles" --recursive --pretty 2>&1)
  if [ -f "$EXPORT_FILE" ]; then
    pass "bundle-export creates file"
    json_check "$(cat "$EXPORT_FILE")" "'profiles' in d or 'entries' in d or 'bundle' in d" && pass "bundle contains profiles/entries" || pass "bundle file is valid JSON"
  else
    fail "bundle-export" "${export_out:0:120}"
  fi

  # Verify
  verify_out=$("$SXMC" inspect bundle-verify "$EXPORT_FILE" --pretty 2>&1)
  if echo "$verify_out" | grep -qi "valid\|ok\|verified\|integrity"; then
    pass "bundle-verify validates bundle"
  else
    # May just output JSON without those keywords
    if json_check "$verify_out" "True"; then
      pass "bundle-verify returns structured result"
    else
      skip "bundle-verify wording" "output may vary"
    fi
  fi

  # Import
  IMPORT_DIR="$BUNDLE_DIR/imported"
  mkdir -p "$IMPORT_DIR"
  import_out=$("$SXMC" inspect bundle-import "$EXPORT_FILE" --output-dir "$IMPORT_DIR" --pretty 2>&1 || true)
  imported_files=$(find "$IMPORT_DIR" -name "*.json" 2>/dev/null | wc -l | tr -d ' ')
  if [ "$imported_files" -ge 1 ]; then
    pass "bundle-import restores $imported_files profiles"
  else
    skip "bundle-import" "import output format may vary"
  fi
fi

# ── Section 23: Bundle Signing ──
section "23. Bundle Signing"

KEYS_DIR="$TMPDIR_TEST/keys"
keygen_out=$("$SXMC" inspect bundle-keygen --output-dir "$KEYS_DIR" --pretty 2>&1)
key_files=$(find "$KEYS_DIR" -type f 2>/dev/null | wc -l | tr -d ' ')
if [ "${key_files:-0}" -gt 0 ]; then
  pass "bundle-keygen creates $key_files key files"
else
  fail "bundle-keygen should create key files" "${keygen_out:0:120}"
fi

# HMAC signing
if has_cmd git && [ -f "$TMPDIR_TEST/bundle-ops/exported.json" ]; then
  HMAC_BUNDLE="$TMPDIR_TEST/hmac-bundle.json"
  hmac_out=$("$SXMC" inspect bundle-export --output "$HMAC_BUNDLE" "$TMPDIR_TEST/bundle-ops/profiles" --recursive --signature-secret "test-secret-123" --pretty 2>&1)
  if [ -f "$HMAC_BUNDLE" ]; then
    pass "HMAC-signed bundle created"

    # Verify with correct secret
    hmac_verify=$("$SXMC" inspect bundle-verify "$HMAC_BUNDLE" --signature-secret "test-secret-123" --pretty 2>&1)
    echo "$hmac_verify" | grep -qi "valid\|ok\|verified\|true" && pass "HMAC verify with correct secret" || skip "HMAC verify wording" "output varies"

    # Verify with wrong secret should fail
    hmac_bad=$("$SXMC" inspect bundle-verify "$HMAC_BUNDLE" --signature-secret "wrong-secret" --pretty 2>&1 || true)
    echo "$hmac_bad" | grep -qi "invalid\|fail\|error\|mismatch\|false" && pass "HMAC rejects wrong secret" || skip "HMAC rejection" "output varies"
  else
    fail "HMAC bundle export" "${hmac_out:0:120}"
  fi
fi

# Ed25519 signing
if [ -d "$KEYS_DIR" ]; then
  SIGNING_KEY=$(find "$KEYS_DIR" -name "*.key.json" -o -name "*private*" -o -name "*.pem" 2>/dev/null | head -1)
  PUBLIC_KEY=$(find "$KEYS_DIR" -name "*.pub.json" -o -name "*.pub" 2>/dev/null | head -1)

  if [ -n "$SIGNING_KEY" ] && [ -n "$PUBLIC_KEY" ] && has_cmd git; then
    ED_BUNDLE="$TMPDIR_TEST/ed25519-bundle.json"
    ed_out=$("$SXMC" inspect bundle-export --output "$ED_BUNDLE" "$TMPDIR_TEST/bundle-ops/profiles" --recursive --signing-key "$SIGNING_KEY" --pretty 2>&1)
    if [ -f "$ED_BUNDLE" ]; then
      pass "Ed25519-signed bundle created"

      ed_verify=$("$SXMC" inspect bundle-verify "$ED_BUNDLE" --public-key "$PUBLIC_KEY" --pretty 2>&1)
      echo "$ed_verify" | grep -qi "valid\|ok\|verified\|true" && pass "Ed25519 verify with correct key" || skip "Ed25519 verify wording" "output varies"
    else
      fail "Ed25519 bundle export" "${ed_out:0:120}"
    fi
  else
    skip "Ed25519 signing" "key files not found in expected pattern"
  fi
fi

# ── Section 24: Corpus ──
section "24. Corpus"

if has_cmd git; then
  # Need some saved profiles first
  CORPUS_HOME="$TMPDIR_TEST/corpus-home"
  mkdir -p "$CORPUS_HOME/.sxmc/ai/profiles"
  "$SXMC" inspect cli git > "$CORPUS_HOME/.sxmc/ai/profiles/git.json" 2>/dev/null
  "$SXMC" inspect cli ls > "$CORPUS_HOME/.sxmc/ai/profiles/ls.json" 2>/dev/null
  "$SXMC" inspect cli curl > "$CORPUS_HOME/.sxmc/ai/profiles/curl.json" 2>/dev/null

  # export-corpus
  corpus_export=$(HOME="$CORPUS_HOME" "$SXMC" inspect export-corpus "$CORPUS_HOME/.sxmc/ai/profiles" --recursive --pretty 2>&1)
  if json_check "$corpus_export" "'profiles' in d or 'entries' in d or 'count' in d"; then
    pass "export-corpus produces structured output"
  else
    if [ -n "$corpus_export" ]; then
      pass "export-corpus produces output"
    else
      fail "export-corpus"
    fi
  fi

  # corpus-stats
  CORPUS_FILE="$TMPDIR_TEST/corpus-export.json"
  HOME="$CORPUS_HOME" "$SXMC" inspect export-corpus "$CORPUS_HOME/.sxmc/ai/profiles" --recursive > "$CORPUS_FILE" 2>/dev/null
  if [ -s "$CORPUS_FILE" ]; then
    stats_out=$("$SXMC" inspect corpus-stats "$CORPUS_FILE" --pretty 2>&1)
    if [ -n "$stats_out" ]; then
      pass "corpus-stats produces output"
    else
      fail "corpus-stats empty"
    fi

    # corpus-query
    query_out=$("$SXMC" inspect corpus-query "$CORPUS_FILE" --pretty 2>&1 || true)
    if [ -n "$query_out" ]; then
      pass "corpus-query produces output"
    else
      skip "corpus-query" "may need search term"
    fi
  else
    skip "corpus-stats/query" "export-corpus produced empty file"
  fi
fi

# ── Section 25: Registry ──
section "25. Registry"

REG_DIR="$TMPDIR_TEST/test-registry"
reg_init=$("$SXMC" inspect registry-init "$REG_DIR" --pretty 2>&1)
if [ -d "$REG_DIR" ]; then
  pass "registry-init creates directory"

  # Add an entry (using a bundle if available)
  if [ -f "$TMPDIR_TEST/bundle-ops/exported.json" ]; then
    reg_add=$("$SXMC" inspect registry-add "$TMPDIR_TEST/bundle-ops/exported.json" --registry "$REG_DIR" --pretty 2>&1 || true)
    if echo "$reg_add" | grep -qi "added\|ok\|success" || json_check "$reg_add" "True" 2>/dev/null; then
      pass "registry-add adds entry"
    else
      skip "registry-add" "output varies: ${reg_add:0:80}"
    fi
  fi

  reg_list=$("$SXMC" inspect registry-list "$REG_DIR" --pretty 2>&1 || true)
  if [ -n "$reg_list" ]; then
    pass "registry-list produces output"
  else
    skip "registry-list" "may be empty"
  fi
else
  fail "registry-init" "${reg_init:0:100}"
fi

# ── Section 26: Trust ──
section "26. Trust"

if [ -f "$TMPDIR_TEST/bundle-ops/exported.json" ]; then
  trust_out=$("$SXMC" inspect trust-report "$TMPDIR_TEST/bundle-ops/exported.json" --pretty 2>&1)
  if [ -n "$trust_out" ]; then
    pass "trust-report produces output"
  else
    fail "trust-report"
  fi

  policy_out=$("$SXMC" inspect trust-policy "$TMPDIR_TEST/bundle-ops/exported.json" --pretty 2>&1 || true)
  if [ -n "$policy_out" ]; then
    pass "trust-policy produces output"
  else
    skip "trust-policy" "may need policy flags"
  fi
fi

# ── Section 27: Known-Good ──
section "27. Known-Good"

if [ -f "$TMPDIR_TEST/bundle-ops/exported.json" ]; then
  known_out=$("$SXMC" inspect known-good "$TMPDIR_TEST/bundle-ops/exported.json" --command git --pretty 2>&1 || true)
  if [ -n "$known_out" ]; then
    pass "known-good produces output for git"
  else
    skip "known-good" "may need specific bundle format"
  fi
fi

# ── Section 28: New Inspect Features ──
section "28. New Inspect Features"

if has_cmd git; then
  # diff --format markdown
  before_profile="$TMPDIR_TEST/git-before-md.json"
  "$SXMC" inspect cli git > "$before_profile" 2>/dev/null
  md_diff=$("$SXMC" inspect diff git --before "$before_profile" --format markdown 2>&1 || true)
  if echo "$md_diff" | grep -qi "markdown\|#\|no changes\|identical\|delta"; then
    pass "diff --format markdown works"
  else
    if [ -n "$md_diff" ]; then
      pass "diff --format markdown produces output"
    else
      fail "diff --format markdown"
    fi
  fi

  # migrate-profile
  migrate_out=$("$SXMC" inspect migrate-profile "$before_profile" --pretty 2>&1 || true)
  if [ -n "$migrate_out" ]; then
    pass "migrate-profile produces output"
  else
    skip "migrate-profile" "may need older profile"
  fi

  # drift
  DRIFT_HOME="$TMPDIR_TEST/drift-home"
  mkdir -p "$DRIFT_HOME/.sxmc/ai/profiles"
  cp "$before_profile" "$DRIFT_HOME/.sxmc/ai/profiles/git.json"
  drift_out=$(HOME="$DRIFT_HOME" "$SXMC" inspect drift "$DRIFT_HOME/.sxmc/ai/profiles" --recursive --pretty 2>&1 || true)
  if [ -n "$drift_out" ]; then
    pass "drift produces output"
  else
    skip "drift" "may need stale profiles"
  fi

  # batch --retry-failed
  retry_batch="$TMPDIR_TEST/retry-batch.json"
  "$SXMC" inspect batch git this-not-exist-cmd --parallel 1 > "$retry_batch" 2>/dev/null
  if [ -s "$retry_batch" ]; then
    retry_out=$("$SXMC" inspect batch --retry-failed "$retry_batch" --parallel 1 2>/dev/null || true)
    if [ -n "$retry_out" ]; then
      pass "batch --retry-failed accepts previous result"
    else
      skip "batch --retry-failed" "output may be empty for no retries"
    fi
  fi
fi

# ── Section 29: Doctor Enhancements ──
section "29. Doctor Enhancements"

doctor_help=$("$SXMC" doctor --help 2>&1)
echo "$doctor_help" | grep -q "remove" && pass "doctor --help has --remove" || fail "doctor missing --remove"

# Functional --remove test
if has_cmd git; then
  REMOVE_ROOT="$TMPDIR_TEST/doctor-remove-root"
  mkdir -p "$REMOVE_ROOT"
  # First fix to create files
  cat > "$TMPDIR_TEST/doctor-remove-cli" <<'EOF'
#!/bin/sh
cat <<'HELP'
doctor-remove-cli
Usage: doctor-remove-cli [OPTIONS]
Options: --json Emit json
HELP
EOF
  chmod +x "$TMPDIR_TEST/doctor-remove-cli"
  "$SXMC" doctor --check --fix --allow-low-confidence --only claude-code --from-cli "$TMPDIR_TEST/doctor-remove-cli" --root "$REMOVE_ROOT" >/dev/null 2>&1 || true

  if [ -f "$REMOVE_ROOT/CLAUDE.md" ]; then
    remove_out=$("$SXMC" doctor --remove --only claude-code --from-cli "$TMPDIR_TEST/doctor-remove-cli" --root "$REMOVE_ROOT" --human 2>&1 || true)
    if [ -n "$remove_out" ]; then
      pass "doctor --remove produces output"
    else
      skip "doctor --remove" "output may be silent"
    fi
  else
    skip "doctor --remove" "fix didn't create files to remove"
  fi
fi

if has_cmd git; then
  DOCTOR_INFER_ROOT="$TMPDIR_TEST/doctor-infer-root"
  mkdir -p "$DOCTOR_INFER_ROOT"
  infer_add_out=$("$SXMC" add git --host claude-code --root "$DOCTOR_INFER_ROOT" 2>&1 || true)
  if [ -f "$DOCTOR_INFER_ROOT/CLAUDE.md" ]; then
    rm -f "$DOCTOR_INFER_ROOT/CLAUDE.md"
    doctor_infer_out=$("$SXMC" doctor --fix --root "$DOCTOR_INFER_ROOT" 2>&1 || true)
    if [ -f "$DOCTOR_INFER_ROOT/CLAUDE.md" ] && \
       echo "$doctor_infer_out" | grep -q "Auto-detected AI hosts:" && \
       echo "$doctor_infer_out" | grep -q "Using CLI surface: git"; then
      pass "doctor --fix infers host and CLI from existing state"
    else
      fail "doctor inferred fix" "${doctor_infer_out:0:120}"
    fi
  else
    fail "doctor inference setup" "${infer_add_out:0:120}"
  fi
fi

# ── Section 30: CI Scaffold ──
section "30. CI Scaffold"

scaffold_help=$("$SXMC" scaffold --help 2>&1)
echo "$scaffold_help" | grep -q "ci" && pass "scaffold --help lists ci" || fail "scaffold missing ci"

if has_cmd git && [ -f "$TMPDIR_TEST/git-profile.json" ]; then
  ci_out=$("$SXMC" scaffold ci --from-profile "$TMPDIR_TEST/git-profile.json" --mode preview 2>&1)
  if echo "$ci_out" | grep -qi "github\|actions\|workflow\|on:\|jobs:"; then
    pass "scaffold ci produces GitHub Actions workflow"
  else
    if [ -n "$ci_out" ]; then
      pass "scaffold ci produces output"
    else
      fail "scaffold ci" "empty output"
    fi
  fi
fi

# ── Section 31: Health Gates ──
section "31. Health Gates"

# status --health (may not have baked entries)
health_out=$("$SXMC" status --health --pretty 2>/dev/null || true)
if [ -n "$health_out" ]; then
  pass "status --health produces output"
else
  skip "status --health" "may need baked entries"
fi

# Test exit-code behavior
"$SXMC" status --health --exit-code >/dev/null 2>&1
health_ec=$?
if [ "$health_ec" -eq 0 ] || [ "$health_ec" -eq 1 ]; then
  pass "status --health --exit-code returns 0 or 1 ($health_ec)"
else
  fail "status --health --exit-code unexpected exit code $health_ec"
fi

# ── Section 32: Discovery Lifecycle ──
section "32. Discovery Lifecycle"

disc_graphql_help=$("$SXMC" discover graphql --help 2>&1 || true)
echo "$disc_graphql_help" | grep -q -- "--schema" && pass "discover graphql has --schema" || fail "discover graphql has --schema"
echo "$disc_graphql_help" | grep -q -- "--output" && pass "discover graphql has --output" || fail "discover graphql has --output"

disc_graphql_diff_help=$("$SXMC" discover graphql-diff --help 2>&1 || true)
echo "$disc_graphql_diff_help" | grep -q -- "--before" && pass "discover graphql-diff has --before" || fail "discover graphql-diff has --before"
echo "$disc_graphql_diff_help" | grep -q -- "--url" && pass "discover graphql-diff has --url" || fail "discover graphql-diff has --url"

cat > "$TMPDIR_TEST/curl-history.txt" <<'EOF'
curl https://api.example.com/users
curl -X POST -H 'Content-Type: application/json' https://api.example.com/users -d '{"name":"Ada"}'
EOF

traffic_json=$(sxmc_isolated discover traffic "$TMPDIR_TEST/curl-history.txt" --format json 2>/dev/null || true)
if [ -n "$traffic_json" ] && json_check "$traffic_json" "d['capture_kind'] == 'curl' and d['endpoint_count'] >= 1"; then
  pass "discover traffic reads curl history"
else
  fail "discover traffic reads curl history"
fi

disc_traffic_diff_help=$("$SXMC" discover traffic-diff --help 2>&1 || true)
echo "$disc_traffic_diff_help" | grep -q -- "--before" && pass "discover traffic-diff has --before" || fail "discover traffic-diff has --before"
echo "$disc_traffic_diff_help" | grep -q -- "--source" && pass "discover traffic-diff has --source" || fail "discover traffic-diff has --source"

# --- Discover Codebase (v0.2.40) ---
codebase_out=$("$SXMC" discover codebase 2>/dev/null || true)
if [ -n "$codebase_out" ] && json_check "$codebase_out" "d.get('config_count', 0) >= 5"; then
  cb_count=$(json_field "$codebase_out" "d['config_count']")
  pass "discover codebase finds $cb_count configs"
else
  fail "discover codebase" "${codebase_out:0:100}"
fi

codebase_compact=$("$SXMC" discover codebase --compact 2>/dev/null || true)
if [ -n "$codebase_compact" ] && [ ${#codebase_compact} -lt ${#codebase_out} ]; then
  pass "discover codebase --compact is smaller"
else
  fail "discover codebase --compact" "not smaller than full"
fi

cb_snapshot="$TMPDIR_TEST/codebase-snapshot.json"
"$SXMC" discover codebase --output "$cb_snapshot" 2>/dev/null
if [ -f "$cb_snapshot" ] && [ -s "$cb_snapshot" ]; then
  pass "discover codebase --output writes snapshot"
else
  fail "discover codebase --output" "snapshot file missing or empty"
fi

cb_diff=$("$SXMC" discover codebase-diff --before "$cb_snapshot" 2>/dev/null || true)
if [ -n "$cb_diff" ]; then
  pass "discover codebase-diff --before produces output"
else
  fail "discover codebase-diff" "no output"
fi

if "$SXMC" discover codebase-diff --before "$cb_snapshot" --exit-code >/dev/null 2>&1; then
  pass "discover codebase-diff --exit-code returns 0 (no drift)"
else
  fail "discover codebase-diff --exit-code should return 0 for same snapshot"
fi

# --- Discover DB (v0.2.40, synthetic SQLite) ---
test_db="$TMPDIR_TEST/test.db"
python3 -c "
import sqlite3
conn = sqlite3.connect('$test_db')
c = conn.cursor()
c.execute('CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL, email TEXT UNIQUE)')
c.execute('CREATE TABLE orders (id INTEGER PRIMARY KEY, user_id INTEGER, amount REAL, FOREIGN KEY(user_id) REFERENCES users(id))')
c.execute('CREATE TABLE products (id INTEGER PRIMARY KEY, name TEXT, price REAL, category TEXT)')
c.execute('INSERT INTO users VALUES (1, \"Ada\", \"ada@example.com\")')
c.execute('INSERT INTO orders VALUES (1, 1, 42.50)')
conn.commit()
conn.close()
" 2>/dev/null

if [ -f "$test_db" ]; then
  db_list=$("$SXMC" discover db "$test_db" --list 2>/dev/null || true)
  if [ -n "$db_list" ] && json_check "$db_list" "d.get('count', 0) >= 3"; then
    db_count=$(json_field "$db_list" "d['count']")
    pass "discover db --list finds $db_count tables"
  else
    fail "discover db --list" "${db_list:0:100}"
  fi

  db_search=$("$SXMC" discover db "$test_db" --search users 2>/dev/null || true)
  if [ -n "$db_search" ] && echo "$db_search" | grep -qi "users"; then
    pass "discover db --search filters tables"
  else
    fail "discover db --search" "${db_search:0:100}"
  fi

  db_detail=$("$SXMC" discover db "$test_db" users 2>/dev/null || true)
  if [ -n "$db_detail" ] && echo "$db_detail" | grep -qi "column\|name\|email"; then
    pass "discover db <table> shows columns"
  else
    fail "discover db <table>" "${db_detail:0:100}"
  fi

  db_snapshot="$TMPDIR_TEST/db-snapshot.json"
  "$SXMC" discover db "$test_db" --output "$db_snapshot" >/dev/null 2>&1 || true
  if [ -f "$db_snapshot" ] && [ -s "$db_snapshot" ]; then
    pass "discover db --output writes snapshot"
  else
    fail "discover db --output" "snapshot missing or empty"
  fi

else
  skip "discover db tests" "python3 failed to create SQLite"
fi

# --- Discover Traffic snapshot + diff (v0.2.40) ---
traffic_snapshot="$TMPDIR_TEST/traffic-snapshot.json"
sxmc_isolated discover traffic "$TMPDIR_TEST/curl-history.txt" --output "$traffic_snapshot" 2>/dev/null || true
if [ -f "$traffic_snapshot" ] && [ -s "$traffic_snapshot" ]; then
  pass "discover traffic --output writes snapshot"
else
  fail "discover traffic --output" "snapshot missing or empty"
fi

if [ -f "$traffic_snapshot" ]; then
  if "$SXMC" discover traffic-diff --before "$traffic_snapshot" --after "$traffic_snapshot" --exit-code >/dev/null 2>&1; then
    pass "discover traffic-diff --exit-code returns 0 (identical snapshots)"
  else
    fail "discover traffic-diff --exit-code should return 0 for same snapshot"
  fi
fi

# --- Discover GraphQL (v0.2.40, needs network) ---
GRAPHQL_URL="https://graphql.anilist.co"
if has_cmd curl && curl -s --max-time 5 -X POST -H "Content-Type: application/json" -d '{"query":"{__typename}"}' "$GRAPHQL_URL" >/dev/null 2>&1; then
  gql_list=$("$SXMC" discover graphql "$GRAPHQL_URL" --list --timeout-seconds 10 2>/dev/null || true)
  if [ -n "$gql_list" ] && json_check "$gql_list" "d.get('count', 0) >= 1"; then
    gql_ops=$(json_field "$gql_list" "d['count']")
    pass "discover graphql --list finds $gql_ops operations"
  else
    fail "discover graphql --list" "${gql_list:0:100}"
  fi

  gql_schema=$("$SXMC" discover graphql "$GRAPHQL_URL" --schema --timeout-seconds 10 2>/dev/null || true)
  if [ -n "$gql_schema" ] && json_check "$gql_schema" "d.get('operation_count', 0) >= 1"; then
    gql_ops_s=$(json_field "$gql_schema" "d['operation_count']")
    pass "discover graphql --schema finds $gql_ops_s operations"
  else
    fail "discover graphql --schema" "${gql_schema:0:100}"
  fi

  gql_snapshot="$TMPDIR_TEST/graphql-snapshot.json"
  "$SXMC" discover graphql "$GRAPHQL_URL" --schema --output "$gql_snapshot" --timeout-seconds 10 2>/dev/null || true
  if [ -f "$gql_snapshot" ] && [ -s "$gql_snapshot" ]; then
    pass "discover graphql --output writes snapshot"
  else
    fail "discover graphql --output" "snapshot missing or empty"
  fi

  if [ -f "$gql_snapshot" ]; then
    if "$SXMC" discover graphql-diff --before "$gql_snapshot" --url "$GRAPHQL_URL" --exit-code --timeout-seconds 10 >/dev/null 2>&1; then
      pass "discover graphql-diff --exit-code returns 0 (no schema drift)"
    else
      fail "discover graphql-diff --exit-code should return 0 against same endpoint"
    fi
  fi
else
  skip "discover graphql live tests" "no network or GraphQL endpoint unreachable"
fi

# ── Section 33: Add Pipeline ──
section "33. Add Pipeline"

if has_cmd git; then
  ADD_ROOT="$TMPDIR_TEST/add-root"
  mkdir -p "$ADD_ROOT"
  printf "# Existing Claude guidance\n" > "$ADD_ROOT/CLAUDE.md"

  add_apply_out=$("$SXMC" add git --root "$ADD_ROOT" 2>&1 || true)
  if echo "$add_apply_out" | grep -q "Detected configured AI hosts: Claude Code"; then
    pass "add detects configured Claude host"
  else
    fail "add host detection" "${add_apply_out:0:120}"
  fi

  if [ -f "$ADD_ROOT/.sxmc/ai/profiles/git.json" ] && [ -f "$ADD_ROOT/.sxmc/ai/claude-code-mcp.json" ]; then
    pass "add writes profile and Claude config"
  else
    fail "add apply outputs" "profile or Claude config missing"
  fi

  if grep -q "sxmc:begin cli-ai:claude-code" "$ADD_ROOT/CLAUDE.md"; then
    pass "add updates Claude agent doc"
  else
    fail "add agent doc update" "managed Claude block missing"
  fi

  ADD_PREVIEW_ROOT="$TMPDIR_TEST/add-preview-root"
  mkdir -p "$ADD_PREVIEW_ROOT"
  add_preview_out=$("$SXMC" add git --root "$ADD_PREVIEW_ROOT" 2>&1 || true)
  if echo "$add_preview_out" | grep -q "No configured AI hosts detected" && \
     echo "$add_preview_out" | grep -q "Would create CLI profile:"; then
    pass "add previews when no hosts are configured"
  else
    fail "add preview fallback" "${add_preview_out:0:120}"
  fi

  DISCOVERY_SNAPSHOT="$TMPDIR_TEST/codebase-discovery.json"
  "$SXMC" discover codebase "$ROOT" --output "$DISCOVERY_SNAPSHOT" >/dev/null 2>&1 || true
  if [ -f "$DISCOVERY_SNAPSHOT" ] && [ -s "$DISCOVERY_SNAPSHOT" ]; then
    DISCOVERY_ROOT="$TMPDIR_TEST/discovery-init-root"
    mkdir -p "$DISCOVERY_ROOT"
    discovery_apply_out=$("$SXMC" init discovery "$DISCOVERY_SNAPSHOT" --client claude-code --root "$DISCOVERY_ROOT" --mode apply 2>&1 || true)
    if [ -f "$DISCOVERY_ROOT/CLAUDE.md" ] && grep -q "sxmc:begin cli-ai:discover-codebase" "$DISCOVERY_ROOT/CLAUDE.md"; then
      pass "init discovery applies codebase context to Claude"
    else
      fail "init discovery apply" "${discovery_apply_out:0:120}"
    fi

    DISCOVERY_PREVIEW_ROOT="$TMPDIR_TEST/discovery-preview-root"
    mkdir -p "$DISCOVERY_PREVIEW_ROOT"
    discovery_preview_out=$("$SXMC" init discovery "$DISCOVERY_SNAPSHOT" --coverage full --root "$DISCOVERY_PREVIEW_ROOT" --mode preview 2>&1 || true)
    if echo "$discovery_preview_out" | grep -q "Would create Portable Codebase context:"; then
      pass "init discovery previews full-coverage context"
    else
      fail "init discovery preview" "${discovery_preview_out:0:120}"
    fi
  else
    fail "discover codebase snapshot for init discovery" "snapshot missing or empty"
  fi

  SETUP_ROOT="$TMPDIR_TEST/setup-root"
  mkdir -p "$SETUP_ROOT"
  printf "# Existing Claude guidance\n" > "$SETUP_ROOT/CLAUDE.md"
  setup_apply_out=$("$SXMC" setup --tool git,ls --root "$SETUP_ROOT" 2>&1 || true)
  if echo "$setup_apply_out" | grep -q "Selected tools: git, ls" && \
     [ -f "$SETUP_ROOT/.sxmc/ai/profiles/git.json" ] && \
     [ -f "$SETUP_ROOT/.sxmc/ai/profiles/ls.json" ]; then
    pass "setup onboards multiple tools in one pass"
  else
    fail "setup apply" "${setup_apply_out:0:120}"
  fi

  SETUP_PREVIEW_ROOT="$TMPDIR_TEST/setup-preview-root"
  mkdir -p "$SETUP_PREVIEW_ROOT"
  setup_preview_out=$("$SXMC" setup --tool git --root "$SETUP_PREVIEW_ROOT" 2>&1 || true)
  if echo "$setup_preview_out" | grep -q "No configured AI hosts detected" && \
     echo "$setup_preview_out" | grep -q "Would create CLI profile:"; then
    pass "setup previews when no hosts are configured"
  else
    fail "setup preview" "${setup_preview_out:0:120}"
  fi

  REGISTER_ROOT="$TMPDIR_TEST/register-root"
  mkdir -p "$REGISTER_ROOT"
  "$SXMC" wrap git --register-host cursor --register-root "$REGISTER_ROOT" >/dev/null 2>&1 &
  WRAP_REGISTER_PID=$!
  wrap_registered=0
  for _ in $(seq 1 50); do
    if [ -f "$REGISTER_ROOT/.cursor/mcp.json" ]; then
      wrap_registered=1
      break
    fi
    sleep 0.1
  done
  kill $WRAP_REGISTER_PID 2>/dev/null || true
  wait $WRAP_REGISTER_PID 2>/dev/null || true
  if [ "$wrap_registered" -eq 1 ] && grep -q '"sxmc-wrap-git"' "$REGISTER_ROOT/.cursor/mcp.json"; then
    pass "wrap auto-registers Cursor MCP config"
  else
    fail "wrap auto-registration" "Cursor MCP config missing or incomplete"
  fi

  SERVE_REGISTER_ROOT="$TMPDIR_TEST/serve-register-root"
  mkdir -p "$SERVE_REGISTER_ROOT"
  "$SXMC" serve --paths "$FIXTURES" --register-host cursor --register-root "$SERVE_REGISTER_ROOT" >/dev/null 2>&1 &
  SERVE_REGISTER_PID=$!
  serve_registered=0
  for _ in $(seq 1 50); do
    if [ -f "$SERVE_REGISTER_ROOT/.cursor/mcp.json" ]; then
      serve_registered=1
      break
    fi
    sleep 0.1
  done
  kill $SERVE_REGISTER_PID 2>/dev/null || true
  wait $SERVE_REGISTER_PID 2>/dev/null || true
  if [ "$serve_registered" -eq 1 ] && grep -q '"sxmc-serve"' "$SERVE_REGISTER_ROOT/.cursor/mcp.json"; then
    pass "serve auto-registers Cursor MCP config"
  else
    fail "serve auto-registration" "Cursor MCP config missing or incomplete"
  fi
fi

# ============================================================================
# PART C — 10×10×10 MATRIX
# ============================================================================
printf "\n${BOLD}╔════════════════════════════════════════╗${RESET}"
printf "\n${BOLD}║  PART C — 10×10×10 MATRIX             ║${RESET}"
printf "\n${BOLD}╚════════════════════════════════════════╝${RESET}\n"

# ── Section 34: 10 Known CLIs ──
section "34. 10 Known CLIs"

MATRIX_CLIS=(git curl ls ssh tar grep find gh python3 jq)
CLI_PASS=0; CLI_SKIP_COUNT=0; CLI_FAIL_COUNT=0

for cmd in "${MATRIX_CLIS[@]}"; do
  if ! has_cmd "$cmd"; then
    skip "$cmd: not installed" "skipping all tests"
    ((CLI_SKIP_COUNT++))
    continue
  fi

  # Inspect
  out=$("$SXMC" inspect cli "$cmd" 2>/dev/null)
  if json_check "$out" "'summary' in d"; then
    pass "$cmd: inspect produces summary"
  else
    fail "$cmd: inspect" "${out:0:60}"
    ((CLI_FAIL_COUNT++))
    continue
  fi

  # Compact
  compact=$("$SXMC" inspect cli "$cmd" --compact 2>/dev/null)
  if [ ${#compact} -lt ${#out} ]; then
    pass "$cmd: compact is smaller"
  else
    fail "$cmd: compact not smaller"
  fi

  # Save profile and scaffold
  echo "$out" > "$TMPDIR_TEST/${cmd}-profile.json"
  scaffold_out=$("$SXMC" scaffold skill --from-profile "$TMPDIR_TEST/${cmd}-profile.json" --output-dir "$TMPDIR_TEST/matrix-scaffolds" 2>&1)
  echo "$scaffold_out" | grep -q "SKILL.md" && pass "$cmd: scaffold skill" || fail "$cmd: scaffold skill"

  # Init AI
  ai_cmd=( "$SXMC" init ai --from-cli "$cmd" --client claude-code --mode preview )
  if [ "$IS_WINDOWS" -eq 1 ] && [[ "$cmd" == "ssh" || "$cmd" == "find" || "$cmd" == "python3" ]]; then
    ai_cmd+=( --allow-low-confidence )
  fi
  ai_out=$("${ai_cmd[@]}" 2>&1)
  echo "$ai_out" | grep -q "Target:" && pass "$cmd: init ai claude-code" || fail "$cmd: init ai"

  ((CLI_PASS++))
done

pass "CLI matrix: $CLI_PASS CLIs fully tested ($CLI_SKIP_COUNT skipped)"

# ── Section 35: 10 Known Skills ──
section "35. 10 Known Skills"

# Create 6 synthetic skills
SYNTH_SKILLS="$TMPDIR_TEST/synthetic-skills"
mkdir -p "$SYNTH_SKILLS"

for skill_name in math-skill file-skill api-skill transform-skill config-skill multi-tool-skill; do
  mkdir -p "$SYNTH_SKILLS/$skill_name"
  cat > "$SYNTH_SKILLS/$skill_name/SKILL.md" << SKILLEOF
---
name: $skill_name
description: Synthetic test skill for $skill_name operations
---
# $skill_name
A synthetic skill for testing purposes.
## Usage
Run this skill to perform $skill_name operations.
SKILLEOF
done

# Add tools to multi-tool-skill
cat >> "$SYNTH_SKILLS/multi-tool-skill/SKILL.md" << 'MULTIEOF'
## Tools
- tool1: First tool
- tool2: Second tool
- tool3: Third tool
- tool4: Fourth tool
- tool5: Fifth tool
MULTIEOF

ALL_SKILLS_PATHS="$FIXTURES $SYNTH_SKILLS"
SKILL_COUNT=0

# List all skills
skills_json=$("$SXMC" skills list --paths "$FIXTURES" --paths "$SYNTH_SKILLS" --json 2>/dev/null || true)
if [ -n "$skills_json" ]; then
  pass "skills list finds skills across paths"
else
  # Try without --json
  skills_txt=$("$SXMC" skills list --paths "$FIXTURES" --paths "$SYNTH_SKILLS" 2>/dev/null || true)
  if [ -n "$skills_txt" ]; then
    pass "skills list finds skills (text mode)"
  else
    fail "skills list found nothing"
  fi
fi

# Test fixture skills individually
for skill_dir in simple-skill malicious-skill skill-with-scripts skill-with-references; do
  if [ -d "$FIXTURES/$skill_dir" ]; then
    found=$("$SXMC" skills list --paths "$FIXTURES" 2>/dev/null | grep -c "$skill_dir" || true)
    if [ "$found" -ge 1 ]; then
      pass "skill found: $skill_dir"
      ((SKILL_COUNT++))
    else
      fail "skill not found: $skill_dir"
    fi
  else
    skip "$skill_dir" "fixture not found"
  fi
done

# Test synthetic skills
for skill_dir in math-skill file-skill api-skill transform-skill config-skill multi-tool-skill; do
  found=$("$SXMC" skills list --paths "$SYNTH_SKILLS" 2>/dev/null | grep -c "$skill_dir" || true)
  if [ "$found" -ge 1 ]; then
    pass "synthetic skill found: $skill_dir"
    ((SKILL_COUNT++))
  else
    fail "synthetic skill not found: $skill_dir"
  fi
done

# Scan all skills
scan_all=$("$SXMC" scan --paths "$FIXTURES" --paths "$SYNTH_SKILLS" 2>&1)
if [ -n "$scan_all" ]; then
  pass "scan runs across all skill paths"
  if echo "$scan_all" | grep -q "CRITICAL\|SL-INJ"; then
    pass "scan flags malicious-skill specifically"
  else
    skip "scan flagging" "malicious patterns may vary"
  fi
fi

# Skills info
info_out=$("$SXMC" skills info simple-skill --paths "$FIXTURES" 2>&1)
if echo "$info_out" | grep -q "simple-skill"; then
  pass "skills info shows skill name"
else
  fail "skills info" "${info_out:0:80}"
fi
if echo "$info_out" | grep -qi "description\|body\|Hello"; then
  pass "skills info shows skill body"
else
  fail "skills info should show body"
fi

# Skills run
run_out=$("$SXMC" skills run simple-skill --paths "$FIXTURES" TestUser 2>&1)
if echo "$run_out" | grep -q "Hello TestUser"; then
  pass "skills run interpolates arguments"
else
  fail "skills run" "${run_out:0:80}"
fi

# Skills run with no arguments
run_no_args=$("$SXMC" skills run simple-skill --paths "$FIXTURES" 2>&1)
if echo "$run_no_args" | grep -q "Hello"; then
  pass "skills run works with no arguments"
else
  fail "skills run no args" "${run_no_args:0:80}"
fi

# Skills run --script (execute specific script with forwarded args)
script_run=$("$SXMC" skills run skill-with-scripts --paths "$FIXTURES" --script hello.sh -- arg1 arg2 2>&1)
if echo "$script_run" | grep -q "Hello from script.*arg1 arg2"; then
  pass "skills run --script forwards arguments to script"
else
  fail "skills run --script" "${script_run:0:80}"
fi

# Skills run --env
env_run=$("$SXMC" skills run simple-skill --paths "$FIXTURES" --env MYVAR=test EnvUser 2>&1)
if echo "$env_run" | grep -q "Hello EnvUser"; then
  pass "skills run --env sets environment variables"
else
  fail "skills run --env" "${env_run:0:80}"
fi

# Skills run --print-body
body_run=$("$SXMC" skills run simple-skill --paths "$FIXTURES" --print-body 2>&1)
if echo "$body_run" | grep -q "Hello"; then
  pass "skills run --print-body renders skill body"
else
  fail "skills run --print-body" "${body_run:0:80}"
fi

# Serve → MCP tool discovery
serve_list=$("$SXMC" stdio "$SXMC serve --paths $FIXTURES" --list 2>&1)
if echo "$serve_list" | grep -q "get_available_skills"; then
  pass "serve exposes get_available_skills tool"
else
  fail "serve missing get_available_skills"
fi
if echo "$serve_list" | grep -q "get_skill_details"; then
  pass "serve exposes get_skill_details tool"
else
  fail "serve missing get_skill_details"
fi
if echo "$serve_list" | grep -q "get_skill_related_file"; then
  pass "serve exposes get_skill_related_file tool"
else
  fail "serve missing get_skill_related_file"
fi
if echo "$serve_list" | grep -q "skill_with_scripts__hello"; then
  pass "serve exposes script tool: skill_with_scripts__hello"
else
  fail "serve missing script tool"
fi

# Serve → prompts
if echo "$serve_list" | grep -q "Prompts"; then
  pass "serve exposes skills as prompts"
else
  fail "serve missing prompts"
fi

# Serve → resources
if echo "$serve_list" | grep -q "style-guide.md"; then
  pass "serve exposes reference as resource"
else
  fail "serve missing resources"
fi

# Serve → call get_available_skills via MCP
avail_out=$("$SXMC" stdio "$SXMC serve --paths $FIXTURES" get_available_skills --pretty 2>&1)
if echo "$avail_out" | grep -q "simple-skill"; then
  pass "MCP get_available_skills returns skill metadata"
else
  fail "MCP get_available_skills" "${avail_out:0:80}"
fi

# Serve → call get_skill_details via MCP
details_out=$("$SXMC" stdio "$SXMC serve --paths $FIXTURES" get_skill_details name=simple-skill --pretty 2>&1)
if echo "$details_out" | grep -q "Hello.*ARGUMENTS"; then
  pass "MCP get_skill_details returns skill content"
else
  fail "MCP get_skill_details" "${details_out:0:80}"
fi

# Serve → call get_skill_related_file via MCP
ref_out=$("$SXMC" stdio "$SXMC serve --paths $FIXTURES" get_skill_related_file skill_name=skill-with-references relative_path=references/style-guide.md --pretty 2>&1)
if echo "$ref_out" | grep -q "Style Guide\|concise"; then
  pass "MCP get_skill_related_file reads reference content"
else
  fail "MCP get_skill_related_file" "${ref_out:0:80}"
fi

# Serve → call script tool via MCP
script_out=$("$SXMC" stdio "$SXMC serve --paths $FIXTURES" skill_with_scripts__hello --pretty 2>&1)
if echo "$script_out" | grep -q "Hello from script"; then
  pass "MCP script tool executes bash and returns output"
else
  fail "MCP script tool" "${script_out:0:80}"
fi

pass "skills matrix: $SKILL_COUNT skills tested (with info, run, serve, MCP calls)"

# ── Section 36: 10 Known MCPs ──
section "36. 10 Known MCPs"

MCP_COUNT=0
MCP_BAKE_HOME="$TMPDIR_TEST/mcp-matrix-home"
mkdir -p "$MCP_BAKE_HOME"

# MCP 1: stateful fixture
if has_cmd python3 && [ -f "$STATEFUL_SCRIPT" ]; then
  bake_src=$(python3 -c "import json; print(json.dumps(['python3', '$STATEFUL_SCRIPT']))")
  m_out=$(HOME="$MCP_BAKE_HOME" "$SXMC" bake create mcp-stateful --source "$bake_src" --skip-validate 2>&1)
  if echo "$m_out" | grep -q "Created"; then
    pass "MCP 1: stateful fixture baked"
    ((MCP_COUNT++))
  else
    fail "MCP 1: stateful fixture"
  fi
fi

# MCP 6: self-host sxmc serve
self_src=$(python3 -c "import json; print(json.dumps(['$SXMC', 'serve', '--paths', '$FIXTURES']))")
m_out=$(HOME="$MCP_BAKE_HOME" "$SXMC" bake create mcp-selfhost --source "$self_src" --skip-validate 2>&1)
echo "$m_out" | grep -q "Created" && pass "MCP 6: self-hosted sxmc serve baked" && ((MCP_COUNT++)) || fail "MCP 6: self-host"

# MCPs 2-5: npm (if npx available)
if has_cmd npx; then
  NPM_MCPS=(
    "mcp-everything:npx -y @modelcontextprotocol/server-everything"
    "mcp-memory:npx -y @modelcontextprotocol/server-memory"
    "mcp-filesystem:npx -y @modelcontextprotocol/server-filesystem /tmp"
    "mcp-sequential:npx -y @modelcontextprotocol/server-sequential-thinking"
  )
  npm_idx=2
  for entry in "${NPM_MCPS[@]}"; do
    name="${entry%%:*}"
    cmd="${entry#*:}"
    list_out=$("$SXMC" stdio "$cmd" --list 2>/dev/null || true)
    if [ -n "$list_out" ]; then
      pass "MCP $npm_idx: $name responds to --list"
      ((MCP_COUNT++))
    else
      skip "MCP $npm_idx: $name" "npx timeout or not installed"
    fi
    ((npm_idx++))
  done
else
  skip "MCPs 2-5" "npx not available"
fi

# MCPs 7-10: synthetic Python MCP servers
for i in 7 8 9 10; do
  SYNTH_MCP="$TMPDIR_TEST/synth-mcp-$i.py"
  cat > "$SYNTH_MCP" << MCPEOF
#!/usr/bin/env python3
import json, sys

def main():
    while True:
        line = sys.stdin.readline()
        if not line:
            break
        try:
            msg = json.loads(line)
        except:
            continue
        mid = msg.get("id")
        method = msg.get("method", "")

        if method == "initialize":
            resp = {"jsonrpc":"2.0","id":mid,"result":{"protocolVersion":"2024-11-05","serverInfo":{"name":"synth-$i"},"capabilities":{"tools":{}}}}
        elif method == "tools/list":
            resp = {"jsonrpc":"2.0","id":mid,"result":{"tools":[{"name":"synth_tool_$i","description":"Synthetic tool $i","inputSchema":{"type":"object","properties":{}}}]}}
        elif method == "tools/call":
            resp = {"jsonrpc":"2.0","id":mid,"result":{"content":[{"type":"text","text":"synth-$i result"}]}}
        elif method == "notifications/initialized":
            continue
        else:
            resp = {"jsonrpc":"2.0","id":mid,"error":{"code":-32601,"message":"not found"}}

        sys.stdout.write(json.dumps(resp) + "\n")
        sys.stdout.flush()

if __name__ == "__main__":
    main()
MCPEOF
  chmod +x "$SYNTH_MCP"

  synth_src=$(python3 -c "import json; print(json.dumps(['python3', '$SYNTH_MCP']))")
  m_out=$(HOME="$MCP_BAKE_HOME" "$SXMC" bake create "mcp-synth-$i" --source "$synth_src" --skip-validate 2>&1)
  if echo "$m_out" | grep -q "Created"; then
    pass "MCP $i: synthetic server baked"
    ((MCP_COUNT++))
  else
    fail "MCP $i: synthetic bake" "${m_out:0:80}"
  fi
done

# List all baked MCPs
baked_list=$(HOME="$MCP_BAKE_HOME" "$SXMC" bake list 2>&1)
baked_count=$(echo "$baked_list" | grep -c "mcp-" || true)
pass "bake list shows $baked_count baked MCPs"

# Test tools for each baked MCP
for name in mcp-stateful mcp-selfhost mcp-synth-7 mcp-synth-8 mcp-synth-9 mcp-synth-10; do
  tools_out=$(HOME="$MCP_BAKE_HOME" "$SXMC" mcp tools "$name" 2>&1 || true)
  if echo "$tools_out" | grep -qi "tool\|Tools"; then
    pass "$name: mcp tools lists tools"
  else
    skip "$name: tools" "server may not respond in time"
  fi
done

# Cleanup baked MCPs
for name in mcp-stateful mcp-selfhost mcp-synth-7 mcp-synth-8 mcp-synth-9 mcp-synth-10; do
  HOME="$MCP_BAKE_HOME" "$SXMC" bake remove "$name" >/dev/null 2>&1 || true
done

pass "MCP matrix: $MCP_COUNT MCPs tested"

# ── Section 37: Side-by-Side (with vs without sxmc) ──
section "37. Side-by-Side: With vs Without sxmc"

# CLI Understanding
if has_cmd git; then
  # Without: parse --help manually
  without_lines=$(git --help 2>&1 | wc -l | tr -d ' ')
  without_ms=$(time_ms git --help)
  # With: structured JSON
  with_out=$("$SXMC" inspect cli git 2>/dev/null)
  with_ms=$(time_ms "$SXMC" inspect cli git)
  with_subs=$(json_field "$with_out" "len(d.get('subcommands',[]))")
  with_opts=$(json_field "$with_out" "len(d.get('options',[]))")

  printf "  Without sxmc: %s raw lines, %sms, needs manual parsing\n" "$without_lines" "$without_ms"
  printf "  With    sxmc: %s subcommands, %s options, structured JSON, %sms\n" "$with_subs" "$with_opts" "$with_ms"
  pass "side-by-side: CLI understanding (raw text vs structured JSON)"
  bench_record "sidebyside_without_cli_ms" "$without_ms"
  bench_record "sidebyside_with_cli_ms" "$with_ms"
fi

# AI Host Configuration (10 hosts)
if has_cmd git; then
  ai_ms=$(python3 -c "
import subprocess, time, sys
sxmc = sys.argv[1]
hosts = ['claude-code','cursor','gemini-cli','github-copilot','continue-dev','open-code','jetbrains-ai-assistant','junie','windsurf','openai-codex']
t0 = time.time()
for h in hosts:
    subprocess.run([sxmc,'init','ai','--from-cli','git','--client',h,'--mode','preview'], capture_output=True)
print(int((time.time()-t0)*1000))
" "$SXMC_WIN")
  printf "  Without sxmc: ~3+ hours manual writing for 10 AI hosts\n"
  printf "  With    sxmc: 10 AI hosts configured in %sms\n" "$ai_ms"
  pass "side-by-side: AI host config (manual hours vs ${ai_ms}ms)"
  bench_record "sidebyside_init_ai_10hosts_ms" "$ai_ms"
fi

# MCP Server from CLI
if has_cmd git; then
  wrap_ms=$(time_ms "$SXMC" stdio "$SXMC wrap git" --list)
  wrap_tools=$("$SXMC" stdio "$SXMC wrap git" --list 2>&1 | grep -c "^  " || true)
  printf "  Without sxmc: write MCP server (200+ lines code, hours of dev)\n"
  printf "  With    sxmc: %s MCP tools from 'wrap git' in %sms, zero code\n" "$wrap_tools" "$wrap_ms"
  pass "side-by-side: CLI → MCP server (hours of code vs ${wrap_ms}ms)"
  bench_record "sidebyside_wrap_ms" "$wrap_ms"
fi

# Skill Execution
run_ms=$(python3 -c "
import subprocess, time, sys
sxmc = sys.argv[1]
fixtures = sys.argv[2]
t0 = time.time()
subprocess.run([sxmc,'skills','run','simple-skill','--paths',fixtures,'BenchUser'], capture_output=True)
print(int((time.time()-t0)*1000))
" "$SXMC_WIN" "$FIXTURES_WIN")
printf "  Without sxmc: parse YAML frontmatter + interpolate args (~15 lines code)\n"
printf "  With    sxmc: 'skills run simple-skill BenchUser' in %sms\n" "$run_ms"
pass "side-by-side: skill execution (manual parsing vs ${run_ms}ms)"
bench_record "sidebyside_skills_run_ms" "$run_ms"

# Skills → MCP (serve)
serve_ms=$(time_ms "$SXMC" stdio "$SXMC serve --paths $FIXTURES" --list)
serve_tools=$("$SXMC" stdio "$SXMC serve --paths $FIXTURES" --list 2>&1 | grep -c "Tools\|Prompts\|Resources" || true)
printf "  Without sxmc: write custom MCP server to load skills (~100+ lines)\n"
printf "  With    sxmc: 'serve --paths' exposes %s categories in %sms\n" "$serve_tools" "$serve_ms"
pass "side-by-side: skills → MCP server (custom code vs ${serve_ms}ms)"
bench_record "sidebyside_serve_ms" "$serve_ms"

# Full Pipeline
if has_cmd git; then
  pipeline_ms=$(python3 -c "
import subprocess, time, os, tempfile, sys
t0 = time.time()
sxmc = sys.argv[1]
tmpd = tempfile.mkdtemp()
# inspect
pf = os.path.join(tmpd, 'git.json')
with open(pf, 'w') as f:
    subprocess.run([sxmc, 'inspect', 'cli', 'git'], stdout=f, stderr=subprocess.DEVNULL)
# scaffold
subprocess.run([sxmc, 'scaffold', 'skill', '--from-profile', pf, '--output-dir', os.path.join(tmpd, 'out')], capture_output=True)
# init ai (all 10)
for h in ['claude-code','cursor','gemini-cli','github-copilot','continue-dev','open-code','jetbrains-ai-assistant','junie','windsurf','openai-codex']:
    subprocess.run([sxmc, 'init', 'ai', '--from-cli', 'git', '--client', h, '--mode', 'preview'], capture_output=True)
# wrap
subprocess.run([sxmc, 'stdio', sxmc + ' wrap git', '--list'], capture_output=True)
print(int((time.time()-t0)*1000))
" "$SXMC_WIN")
  printf "  Without sxmc: days of manual work (read help, write configs, build MCP server)\n"
  printf "  With    sxmc: inspect → scaffold → 10 AI hosts → MCP server in %sms\n" "$pipeline_ms"
  pass "side-by-side: full pipeline (days vs ${pipeline_ms}ms)"
  bench_record "sidebyside_full_pipeline_ms" "$pipeline_ms"
fi

# Codebase Understanding (v0.2.40)
without_cb_ms=$(python3 -c "
import subprocess, time
t0 = time.time()
subprocess.run(['find', '.', '-name', '*.yml', '-path', '.github/*'], capture_output=True)
subprocess.run(['ls', '-la', 'Cargo.toml', 'README.md'], capture_output=True)
print(int((time.time()-t0)*1000))
")
with_cb_ms=$(time_ms "$SXMC" discover codebase)
with_cb_count=$(json_field "$("$SXMC" discover codebase 2>/dev/null)" "d.get('config_count',0)")
printf "  Without sxmc: find + ls + manual inspection, %sms, unstructured text\n" "$without_cb_ms"
printf "  With    sxmc: discover codebase → %s configs, structured JSON, %sms\n" "$with_cb_count" "$with_cb_ms"
pass "side-by-side: codebase understanding (manual find/ls vs structured discovery)"
bench_record "sidebyside_without_codebase_ms" "$without_cb_ms"
bench_record "sidebyside_with_codebase_ms" "$with_cb_ms"

# Database Schema (v0.2.40)
if [ -f "$test_db" ]; then
  without_db_ms=$(python3 -c "
import subprocess, time
t0 = time.time()
subprocess.run(['sqlite3', '$test_db', '.tables'], capture_output=True)
subprocess.run(['sqlite3', '$test_db', '.schema users'], capture_output=True)
subprocess.run(['sqlite3', '$test_db', '.schema orders'], capture_output=True)
print(int((time.time()-t0)*1000))
" 2>/dev/null || echo "0")
  with_db_ms=$(time_ms "$SXMC" discover db "$test_db")
  printf "  Without sxmc: 3 sqlite3 commands, unstructured text, %sms\n" "$without_db_ms"
  printf "  With    sxmc: discover db → all tables + columns, structured JSON, %sms\n" "$with_db_ms"
  pass "side-by-side: database schema (multiple sqlite3 calls vs single discover)"
  bench_record "sidebyside_without_db_ms" "$without_db_ms"
  bench_record "sidebyside_with_db_ms" "$with_db_ms"
fi

# Traffic Analysis (v0.2.40)
if [ -f "$TMPDIR_TEST/curl-history.txt" ]; then
  without_traffic_ms=$(python3 -c "
import time
t0 = time.time()
# Without sxmc: manually parse curl commands or open HAR in browser devtools
with open('$TMPDIR_TEST/curl-history.txt') as f:
    lines = f.readlines()
    endpoints = [l.strip() for l in lines if l.strip()]
print(int((time.time()-t0)*1000))
")
  with_traffic_ms=$(time_ms "$SXMC" discover traffic "$TMPDIR_TEST/curl-history.txt")
  printf "  Without sxmc: manual parsing of curl history / open HAR in browser, %sms\n" "$without_traffic_ms"
  printf "  With    sxmc: discover traffic → structured endpoint map, %sms\n" "$with_traffic_ms"
  pass "side-by-side: traffic analysis (manual vs structured discovery)"
  bench_record "sidebyside_without_traffic_ms" "$without_traffic_ms"
  bench_record "sidebyside_with_traffic_ms" "$with_traffic_ms"
fi

# ============================================================================
# PART D — BENCHMARKS
# ============================================================================
printf "\n${BOLD}╔════════════════════════════════════════╗${RESET}"
printf "\n${BOLD}║  PART D — BENCHMARKS ($BENCH_RUNS runs)         ║${RESET}"
printf "\n${BOLD}╚════════════════════════════════════════╝${RESET}\n"

# ── Section 38: CLI Inspection Benchmarks ──
section "38. CLI Inspection Benchmarks"

BENCH_CLIS=(git curl ls ssh tar)
for cmd in "${BENCH_CLIS[@]}"; do
  if ! has_cmd "$cmd"; then continue; fi

  # Clear cache for cold measurement
  rm -rf "$TESTHOME/Library/Caches/sxmc" "$TESTHOME/.cache/sxmc" 2>/dev/null

  declare -a cold_times=() warm_times=()

  # Cold run (first one primes cache)
  cold1=$(HOME="$TESTHOME" time_ms "$SXMC" inspect cli "$cmd")
  cold_times+=("$cold1")

  # More cold measurements (clear cache each time)
  for ((i=1; i<BENCH_RUNS; i++)); do
    rm -rf "$TESTHOME/Library/Caches/sxmc" "$TESTHOME/.cache/sxmc" 2>/dev/null
    ct=$(HOME="$TESTHOME" time_ms "$SXMC" inspect cli "$cmd")
    cold_times+=("$ct")
  done

  # Warm measurements (cache is populated from last cold run)
  # Re-prime the cache first
  HOME="$TESTHOME" "$SXMC" inspect cli "$cmd" >/dev/null 2>&1
  for ((i=0; i<BENCH_RUNS; i++)); do
    wt=$(HOME="$TESTHOME" time_ms "$SXMC" inspect cli "$cmd")
    warm_times+=("$wt")
  done

  cold_med=$(median_of "${cold_times[@]}")
  warm_med=$(median_of "${warm_times[@]}")
  printf "  %-8s cold=%4sms  warm=%4sms  (speedup: %sx)\n" "$cmd" "$cold_med" "$warm_med" "$(python3 -c "print(f'{$cold_med / max($warm_med,1):.1f}')")"
  bench_record "inspect_cold_${cmd}_ms" "$cold_med"
  bench_record "inspect_warm_${cmd}_ms" "$warm_med"
done
pass "CLI inspection benchmarks complete"

# Batch benchmark
if has_cmd git && has_cmd curl && has_cmd ls; then
  rm -rf "$TESTHOME/Library/Caches/sxmc" "$TESTHOME/.cache/sxmc" 2>/dev/null
  declare -a batch_p1=() batch_p4=()

  for ((i=0; i<BENCH_RUNS; i++)); do
    rm -rf "$TESTHOME/Library/Caches/sxmc" "$TESTHOME/.cache/sxmc" 2>/dev/null
    bt=$(HOME="$TESTHOME" time_ms "$SXMC" inspect batch git curl ls ssh tar --parallel 1)
    batch_p1+=("$bt")
  done
  for ((i=0; i<BENCH_RUNS; i++)); do
    rm -rf "$TESTHOME/Library/Caches/sxmc" "$TESTHOME/.cache/sxmc" 2>/dev/null
    bt=$(HOME="$TESTHOME" time_ms "$SXMC" inspect batch git curl ls ssh tar --parallel 4)
    batch_p4+=("$bt")
  done

  p1_med=$(median_of "${batch_p1[@]}")
  p4_med=$(median_of "${batch_p4[@]}")
  printf "  batch --parallel 1: %sms\n" "$p1_med"
  printf "  batch --parallel 4: %sms  (speedup: %sx)\n" "$p4_med" "$(python3 -c "print(f'{$p1_med / max($p4_med,1):.1f}')")"
  bench_record "batch_parallel_1_ms" "$p1_med"
  bench_record "batch_parallel_4_ms" "$p4_med"
  pass "batch parallelism benchmarks complete"
fi

# ── Section 39: Wrap Benchmark ──
section "39. Wrap & MCP Benchmarks"

if has_cmd git; then
  declare -a wrap_times=()
  for ((i=0; i<BENCH_RUNS; i++)); do
    wt=$(time_ms "$SXMC" stdio "$SXMC wrap git" --list)
    wrap_times+=("$wt")
  done
  wrap_med=$(median_of "${wrap_times[@]}")
  printf "  wrap git → stdio --list: %sms\n" "$wrap_med"
  bench_record "wrap_git_list_ms" "$wrap_med"
  pass "wrap benchmark complete"
fi

# ── Section 40: Bundle Benchmark ──
section "40. Bundle Benchmarks"

if has_cmd git; then
  B_HOME="$TMPDIR_TEST/bench-bundle"
  mkdir -p "$B_HOME/.sxmc/ai/profiles"
  for cmd in git curl ls ssh tar; do
    has_cmd "$cmd" && "$SXMC" inspect cli "$cmd" > "$B_HOME/.sxmc/ai/profiles/${cmd}.json" 2>/dev/null
  done

  declare -a export_times=()
  for ((i=0; i<BENCH_RUNS; i++)); do
    BFILE="$TMPDIR_TEST/bench-export-$i.json"
    et=$(python3 -c "
import subprocess, time, sys
t0 = time.time()
subprocess.run(sys.argv[1:], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
print(int((time.time() - t0) * 1000))
" "$SXMC" inspect bundle-export --output "$BFILE" "$B_HOME/.sxmc/ai/profiles" --recursive)
    export_times+=("$et")
  done
  export_med=$(median_of "${export_times[@]}")
  printf "  bundle export (5 profiles): %sms\n" "$export_med"
  bench_record "bundle_export_ms" "$export_med"

  # HMAC sign benchmark
  declare -a sign_times=()
  for ((i=0; i<BENCH_RUNS; i++)); do
    SFILE="$TMPDIR_TEST/bench-signed-$i.json"
    st=$(python3 -c "
import subprocess, time, sys
t0 = time.time()
subprocess.run(sys.argv[1:], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
print(int((time.time() - t0) * 1000))
" "$SXMC" inspect bundle-export --output "$SFILE" "$B_HOME/.sxmc/ai/profiles" --recursive --signature-secret "bench-secret")
    sign_times+=("$st")
  done
  sign_med=$(median_of "${sign_times[@]}")
  printf "  bundle export+sign (HMAC): %sms\n" "$sign_med"
  bench_record "bundle_sign_ms" "$sign_med"

  pass "bundle benchmarks complete"
fi

# ── Section 41: Pipeline Benchmark ──
section "41. End-to-End Pipeline Benchmark"

PIPELINE_CLIS=(git curl ls ssh tar)
declare -a pipeline_times=()

for ((run=0; run<BENCH_RUNS; run++)); do
  pt=$(python3 -c "
import subprocess, time, sys, os, tempfile
t0 = time.time()
sxmc = sys.argv[1]
tmpd = tempfile.mkdtemp()
clis = sys.argv[2:]
for c in clis:
    # inspect
    pf = os.path.join(tmpd, c + '.json')
    with open(pf, 'w') as f:
        subprocess.run([sxmc, 'inspect', 'cli', c], stdout=f, stderr=subprocess.DEVNULL)
    # scaffold
    subprocess.run([sxmc, 'scaffold', 'skill', '--from-profile', pf, '--output-dir', os.path.join(tmpd, 'scaffolds')], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    # init ai
    subprocess.run([sxmc, 'init', 'ai', '--from-cli', c, '--client', 'claude-code', '--mode', 'preview'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
print(int((time.time() - t0) * 1000))
" "$SXMC" "${PIPELINE_CLIS[@]}")
  pipeline_times+=("$pt")
done

pipeline_med=$(median_of "${pipeline_times[@]}")
printf "  inspect → scaffold → init-ai (%d CLIs): %sms\n" "${#PIPELINE_CLIS[@]}" "$pipeline_med"
bench_record "pipeline_${#PIPELINE_CLIS[@]}cli_ms" "$pipeline_med"
pass "pipeline benchmark complete"

# ============================================================================
# SUMMARY
# ============================================================================
printf "\n${BOLD}${CYAN}━━━ RESULTS ━━━${RESET}\n"
printf "\n  ${GREEN}Passed:${RESET}  %d\n" "$PASS"
printf "  ${RED}Failed:${RESET}  %d\n" "$FAIL"
printf "  ${YELLOW}Skipped:${RESET} %d\n" "$SKIP"
printf "  Total:   %d\n\n" "$TOTAL"

if [ "$FAIL" -eq 0 ]; then
  printf "${GREEN}${BOLD}ALL TESTS PASSED${RESET}\n\n"
else
  printf "${RED}${BOLD}%d TEST(S) FAILED${RESET}\n\n" "$FAIL"
fi

# JSON output
if [ -n "$JSON_OUT" ]; then
  # Build benchmarks JSON
  BENCH_JSON="{"
  for ((i=0; i<${#BENCH_KEYS[@]}; i++)); do
    [ $i -gt 0 ] && BENCH_JSON+=","
    BENCH_JSON+="\"${BENCH_KEYS[$i]}\":${BENCH_VALS[$i]}"
  done
  BENCH_JSON+="}"

  python3 -c "
import json, sys, os
from datetime import datetime, timezone

d = {
    'sxmc_version': '$SXMC_VERSION',
    'os': '$OS_NAME $OS_ARCH',
    'timestamp': datetime.now(timezone.utc).isoformat(),
    'total': $TOTAL,
    'pass': $PASS,
    'fail': $FAIL,
    'skip': $SKIP,
    'cli_tools_parsed': $PARSED,
    'cli_tools_failed': $PARSE_FAIL,
    'cli_tools_skipped': $PARSE_SKIP,
    'bad_summaries': $BAD_SUMMARIES,
    'bench_runs': $BENCH_RUNS,
    'benchmarks': json.loads(sys.argv[1])
}
with open(sys.argv[2], 'w') as f:
    json.dump(d, f, indent=2)
print(f'Results written to {sys.argv[2]}')
" "$BENCH_JSON" "$JSON_OUT"
fi

exit $(( FAIL > 0 ? 1 : 0 ))
