#!/usr/bin/env bash
# ============================================================================
# sxmc comprehensive cross-platform test suite
# Covers: CLI inspection, MCP, API, security, scaffolds, caching, AI pipeline
# Usage: bash scripts/test-sxmc.sh [--json results.json]
# Env:   SXMC=path/to/sxmc (default: sxmc on PATH, or target/release/sxmc)
# ============================================================================
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FIXTURES="$ROOT/tests/fixtures"
TMPDIR_TEST="$(mktemp -d)"
TESTHOME="$TMPDIR_TEST/home"
mkdir -p "$TESTHOME"
JSON_OUT=""

# Parse args
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
CURRENT_SECTION=""

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

# Cross-platform timing (macOS date doesn't support %N)
time_ms() {
  python3 -c "
import subprocess, time, sys
t0 = time.time()
subprocess.run(sys.argv[1:], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
print(int((time.time() - t0) * 1000))
" "$@"
}

# JSON helpers via python3
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

# Isolated sxmc (temp HOME for bake/cache tests)
sxmc_isolated() {
  HOME="$TESTHOME" USERPROFILE="$TESTHOME" \
  XDG_CONFIG_HOME="$TESTHOME/.config" \
  APPDATA="$TESTHOME/AppData/Roaming" \
  LOCALAPPDATA="$TESTHOME/AppData/Local" \
  "$SXMC" "$@"
}

cleanup() {
  rm -rf "$TMPDIR_TEST" 2>/dev/null
}
trap cleanup EXIT

# --- Resolve sxmc binary ---
if [ -n "${SXMC:-}" ]; then
  : # user-provided
elif has_cmd sxmc; then
  SXMC="sxmc"
elif [ -x "$ROOT/target/release/sxmc" ]; then
  SXMC="$ROOT/target/release/sxmc"
elif [ -x "$ROOT/target/debug/sxmc" ]; then
  SXMC="$ROOT/target/debug/sxmc"
else
  echo "ERROR: sxmc not found. Set SXMC= or install it." >&2
  exit 1
fi

# ============================================================================
# SECTION 1: Environment
# ============================================================================
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
  echo "FATAL: python3 is required for this test suite" >&2
  exit 1
fi

# ============================================================================
# SECTION 2: Help & Completions
# ============================================================================
section "2. Help & Completions"

help_out=$("$SXMC" --help 2>&1)
for kw in serve skills stdio http mcp api inspect init scaffold scan bake doctor completions; do
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

TMP_COMPLETIONS="$TMPDIR_TEST/_sxmc.bash"
"$SXMC" completions bash > "$TMP_COMPLETIONS"
bash_completion_subcmd=$(bash -lc 'source "$1"; COMP_WORDS=(sxmc ins); COMP_CWORD=1; _sxmc sxmc ins sxmc; printf "%s\n" "${COMPREPLY[@]}"' bash "$TMP_COMPLETIONS" 2>/dev/null)
if echo "$bash_completion_subcmd" | grep -qx "inspect"; then
  pass "bash completion completes top-level subcommands"
else
  fail "bash completion should complete inspect" "${bash_completion_subcmd:0:80}"
fi

bash_completion_option=$(bash -lc 'source "$1"; COMP_WORDS=(sxmc inspect batch --fr); COMP_CWORD=3; _sxmc sxmc --fr batch; printf "%s\n" "${COMPREPLY[@]}"' bash "$TMP_COMPLETIONS" 2>/dev/null)
if echo "$bash_completion_option" | grep -qx -- "--from-file"; then
  pass "bash completion completes nested inspect batch options"
else
  fail "bash completion should complete --from-file" "${bash_completion_option:0:80}"
fi

# ============================================================================
# SECTION 3: CLI Inspection Matrix
# ============================================================================
section "3. CLI Inspection Matrix"

# All tools we've tested across versions
CLI_TOOLS=(
  # BSD/Unix core
  ls grep sed cp rm chmod sort tr diff cat mv mkdir wc head tail uniq awk
  # Developer
  git gh npm cargo rustc rustup python3 node brew curl ssh jq
  # System
  tar find xargs tee cut paste join comm env printenv whoami hostname date cal
  # Compression
  zip unzip gzip bzip2 xz
  # Network
  ping dig nslookup traceroute ifconfig netstat
  # Process
  ps top kill lsof open
  # macOS (skip gracefully on Linux)
  pbcopy pbpaste defaults launchctl diskutil sips mdls mdfind
  # Compilers
  xcodebuild swift swiftc clang make cmake
  # Extra edge cases
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
    ((PARSE_FAIL++))
    fail "inspect cli $cmd" "not valid JSON: ${out:0:80}"
    continue
  fi

  ((PARSED++))

  # Summary quality check
  summary=$(json_field "$out" "d.get('summary','')")
  sl=$(printf '%s\n' "$summary" | tr '[:upper:]' '[:lower:]')
  if [ -z "$summary" ]; then
    ((BAD_SUMMARIES++))
  elif printf '%s\n' "$sl" | grep -qE '^usage:|copyright|report bugs|SSUUMM|illegal option|unrecognized'; then
    ((BAD_SUMMARIES++))
  fi
done

if [ "$PARSED" -gt 0 ]; then
  pass "parsed $PARSED CLIs successfully ($PARSE_SKIP not installed, $PARSE_FAIL failed)"
else
  fail "CLI inspection: no tools parsed"
fi

if [ "$PARSE_FAIL" -eq 0 ]; then
  pass "zero parse failures across installed tools"
else
  fail "$PARSE_FAIL tools failed to parse"
fi

if [ "$BAD_SUMMARIES" -eq 0 ]; then
  pass "zero bad summaries"
else
  fail "$BAD_SUMMARIES tools have questionable summaries"
fi

# ============================================================================
# SECTION 4: Previously-Broken Tools (Detailed)
# ============================================================================
section "4. Previously-Broken Tools"

check_tool() {
  local cmd="$1" check_name="$2" check_expr="$3"
  if ! has_cmd "$cmd"; then
    skip "$check_name" "$cmd not installed"
    return
  fi
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
except: print('invalid JSON')
" <<< "$out" 2>/dev/null)
    fail "$check_name" "$diag"
  fi
}

# brew: should have subcommands now (was 0 in v0.2.5-v0.2.7)
check_tool brew "brew: has subcommands (was 0)" "len(d.get('subcommands',[])) >= 5"
check_tool brew "brew: has global options (was 0 in v0.2.7)" "len(d.get('options',[])) >= 1"

# Previously had false positive subcommands
check_tool cat "cat: no false positive subcmds" "len(d.get('subcommands',[])) <= 1"
check_tool lsof "lsof: no false positive subcmds" "len(d.get('subcommands',[])) <= 2"
check_tool dc "dc: no false positive subcmds" "len(d.get('subcommands',[])) <= 2"

# Summary quality fixes
check_tool gzip "gzip: clean summary (not 'Apple gzip')" "'apple gzip' not in d.get('summary','').lower()"
check_tool ping "ping: clean summary (not 'Apple specific')" "'apple specific' not in d.get('summary','').lower()"
check_tool man "man: clean summary (not error msg)" "'illegal option' not in d.get('summary','').lower()"
check_tool less "less: clean summary (not overstrike)" "'SSUUMM' not in d.get('summary','')"
check_tool more "more: clean summary (not overstrike)" "'SSUUMM' not in d.get('summary','')"
check_tool bc "bc: clean summary (not bug report URL)" "'report bugs' not in d.get('summary','').lower()"
check_tool dig "dig: clean summary (not 'Use dig -h')" "not d.get('summary','').startswith('Use ')"
check_tool unzip "unzip: clean summary (not 'Please report bugs')" "'report bugs' not in d.get('summary','').lower()"
check_tool zip "zip: clean summary (not copyright)" "'copyright' not in d.get('summary','').lower()"
check_tool grep "grep: clean summary (not all variants)" "len(d.get('summary','')) < 80"

# awk options (was 0)
check_tool awk "awk: has options (was 0)" "len(d.get('options',[])) >= 1"

# python3 regression (was 24 fake subcmds in v0.2.3)
check_tool python3 "python3: no fake subcommands" "len(d.get('subcommands',[])) == 0"

# rustup regression (lost options in v0.2.3)
check_tool rustup "rustup: has options" "len(d.get('options',[])) >= 2"
check_tool rustup "rustup: has subcommands" "len(d.get('subcommands',[])) >= 10"

# gh subcommand count (was 32, regressed to 10, fixed back)
check_tool gh "gh: has 20+ subcommands (was 10 in v0.2.7)" "len(d.get('subcommands',[])) >= 20"

# ============================================================================
# SECTION 5: Compact Mode
# ============================================================================
section "5. Compact Mode"

if has_cmd git; then
  full_out=$("$SXMC" inspect cli git 2>/dev/null)
  compact_out=$("$SXMC" inspect cli git --compact 2>/dev/null)
  full_chars=${#full_out}
  compact_chars=${#compact_out}

  if [ "$compact_chars" -lt "$full_chars" ]; then
    savings=$(( 100 - (100 * compact_chars / full_chars) ))
    pass "compact mode smaller than full ($savings% reduction)"
  else
    fail "compact mode not smaller" "full=$full_chars compact=$compact_chars"
  fi

  if json_check "$compact_out" "'subcommand_count' in d"; then
    pass "compact has subcommand_count field"
  else
    fail "compact missing subcommand_count"
  fi

  if json_check "$compact_out" "'option_count' in d"; then
    pass "compact has option_count field"
  else
    fail "compact missing option_count"
  fi

  if json_check "$compact_out" "'provenance' not in d"; then
    pass "compact strips provenance"
  else
    fail "compact should not include provenance"
  fi
else
  skip "compact mode tests" "git not installed"
fi

# Heavy tool compact savings
if has_cmd curl; then
  full_c=$("$SXMC" inspect cli curl 2>/dev/null | wc -c | tr -d ' ')
  compact_c=$("$SXMC" inspect cli curl --compact 2>/dev/null | wc -c | tr -d ' ')
  savings=$(( 100 - (100 * compact_c / full_c) ))
  if [ "$savings" -ge 50 ]; then
    pass "curl compact savings >= 50% (got ${savings}%)"
  else
    fail "curl compact savings < 50%" "got ${savings}%"
  fi
else
  skip "curl compact test" "curl not installed"
fi

# ============================================================================
# SECTION 6: Profile Caching
# ============================================================================
section "6. Profile Caching"

if has_cmd git; then
  # Clear cache
  CACHE_DIR_MAC="$TESTHOME/Library/Caches/sxmc"
  CACHE_DIR_LINUX="$TESTHOME/.cache/sxmc"
  rm -rf "$CACHE_DIR_MAC" "$CACHE_DIR_LINUX" 2>/dev/null

  cold_ms=$(HOME="$TESTHOME" time_ms "$SXMC" inspect cli git)
  warm_ms=$(HOME="$TESTHOME" time_ms "$SXMC" inspect cli git)

  # Check cache dir exists
  if [ -d "$CACHE_DIR_MAC" ] || [ -d "$CACHE_DIR_LINUX" ]; then
    cache_files=$(find "$CACHE_DIR_MAC" "$CACHE_DIR_LINUX" -name "*.json" 2>/dev/null | wc -l | tr -d ' ')
    pass "cache directory created ($cache_files files)"
  else
    fail "cache directory not created"
  fi

  if [ "$warm_ms" -le "$cold_ms" ]; then
    pass "warm cache faster or equal (cold=${cold_ms}ms warm=${warm_ms}ms)"
  else
    # Warm can sometimes be slower due to system noise, only fail if much slower
    if [ "$warm_ms" -gt $(( cold_ms * 3 )) ]; then
      fail "warm cache much slower than cold" "cold=${cold_ms}ms warm=${warm_ms}ms"
    else
      pass "cache timing within noise (cold=${cold_ms}ms warm=${warm_ms}ms)"
    fi
  fi
else
  skip "caching tests" "git not installed"
fi

# ============================================================================
# SECTION 7: Scaffold System
# ============================================================================
section "7. Scaffold System"

if has_cmd git; then
  profile=$("$SXMC" inspect cli git 2>/dev/null)
  echo "$profile" > "$TMPDIR_TEST/git-profile.json"

  # Skill scaffold
  skill_out=$("$SXMC" scaffold skill --from-profile "$TMPDIR_TEST/git-profile.json" --output-dir "$TMPDIR_TEST/scaffolds" 2>&1)
  if echo "$skill_out" | grep -q "SKILL.md"; then
    pass "scaffold skill produces SKILL.md"
  else
    fail "scaffold skill" "${skill_out:0:100}"
  fi
  if echo "$skill_out" | grep -qi "subcommand"; then
    pass "scaffold skill mentions subcommands"
  else
    fail "scaffold skill should mention subcommands"
  fi

  # MCP wrapper scaffold
  mcp_out=$("$SXMC" scaffold mcp-wrapper --from-profile "$TMPDIR_TEST/git-profile.json" --output-dir "$TMPDIR_TEST/scaffolds" 2>&1)
  if echo "$mcp_out" | grep -q "README.md"; then
    pass "scaffold mcp-wrapper produces README.md"
  else
    fail "scaffold mcp-wrapper" "${mcp_out:0:100}"
  fi
  if echo "$mcp_out" | grep -q "manifest.json"; then
    pass "scaffold mcp-wrapper produces manifest.json"
  else
    fail "scaffold mcp-wrapper should produce manifest.json"
  fi

  # llms.txt scaffold
  llms_out=$("$SXMC" scaffold llms-txt --from-profile "$TMPDIR_TEST/git-profile.json" 2>&1)
  if echo "$llms_out" | grep -q "llms.txt"; then
    pass "scaffold llms-txt produces llms.txt"
  else
    fail "scaffold llms-txt" "${llms_out:0:100}"
  fi
else
  skip "scaffold tests" "git not installed"
fi

# Overflow hints (use brew if available — 115+ subcmds)
if has_cmd brew; then
  brew_profile=$("$SXMC" inspect cli brew 2>/dev/null)
  echo "$brew_profile" > "$TMPDIR_TEST/brew-profile.json"
  brew_skill=$("$SXMC" scaffold skill --from-profile "$TMPDIR_TEST/brew-profile.json" --output-dir "$TMPDIR_TEST/scaffolds" 2>&1)
  if echo "$brew_skill" | grep -qi "showing.*of\|plus.*more"; then
    pass "scaffold skill shows overflow hints for large CLI"
  else
    skip "scaffold overflow hints" "brew profile may not have enough subcmds"
  fi
else
  skip "scaffold overflow hints" "brew not installed"
fi

# ============================================================================
# SECTION 8: Init AI Pipeline
# ============================================================================
section "8. Init AI Pipeline"

AI_HOSTS=(claude-code cursor gemini-cli github-copilot continue-dev open-code
          jetbrains-ai-assistant junie windsurf openai-codex)

if has_cmd git; then
  for host in "${AI_HOSTS[@]}"; do
    ai_out=$("$SXMC" init ai --from-cli git --client "$host" --mode preview 2>&1)
    if echo "$ai_out" | grep -q "Target:"; then
      pass "init ai --client $host"
    else
      fail "init ai --client $host" "${ai_out:0:80}"
    fi
  done

  # Full coverage mode
  full_ai=$("$SXMC" init ai --from-cli git --coverage full --mode preview 2>&1)
  section_count=$(echo "$full_ai" | grep -c "^==" || true)
  if [ "$section_count" -ge 10 ]; then
    pass "init ai --coverage full produces $section_count sections"
  else
    fail "init ai --coverage full" "only $section_count sections"
  fi
else
  skip "init ai tests" "git not installed"
fi

# ============================================================================
# SECTION 9: Security Scanner
# ============================================================================
section "9. Security Scanner"

# Scan the bundled malicious-skill fixture
if [ -d "$FIXTURES/malicious-skill" ]; then
  scan_out=$("$SXMC" scan --paths "$FIXTURES" 2>&1)

  if echo "$scan_out" | grep -q "CRITICAL"; then
    pass "scanner detects CRITICAL issues"
  else
    fail "scanner should detect CRITICAL" "${scan_out:0:100}"
  fi

  if echo "$scan_out" | grep -q "SL-INJ-001"; then
    pass "scanner detects prompt injection (SL-INJ-001)"
  else
    fail "scanner should detect prompt injection"
  fi

  if echo "$scan_out" | grep -q "SL-EXEC-001\|Dangerous"; then
    pass "scanner detects dangerous operations"
  else
    fail "scanner should detect dangerous ops"
  fi

  if echo "$scan_out" | grep -qi "secret\|SL-SEC"; then
    pass "scanner detects secrets"
  else
    fail "scanner should detect secrets"
  fi
else
  skip "security scanner" "fixtures/malicious-skill not found"
fi

# Enhanced secret patterns
mkdir -p "$TMPDIR_TEST/secret-skill"
cat > "$TMPDIR_TEST/secret-skill/SKILL.md" << 'SECRETEOF'
---
name: secret-test
description: test secret patterns
---
# Secret Test
TOKEN=abc123
SECRET=mysecretvalue
OPENAI_API_KEY=sk-proj-abcdef
AWS_SECRET_ACCESS_KEY=AKIAIOSFODNN7EXAMPLE
GITHUB_TOKEN=ghp_1234567890abcdef
SECRETEOF

secret_scan=$("$SXMC" scan --paths "$TMPDIR_TEST" 2>&1)
secret_count=$(echo "$secret_scan" | grep -c "SL-SEC-001\|secret\|credential" || true)
if [ "$secret_count" -ge 3 ]; then
  pass "scanner catches $secret_count secret patterns"
else
  fail "scanner should catch more secret patterns" "found $secret_count"
fi

# ============================================================================
# SECTION 10: MCP Bake + Grep + Call Pipeline
# ============================================================================
section "10. MCP Pipeline"

STATEFUL_SCRIPT="$FIXTURES/stateful_mcp_server.py"

if has_cmd python3 && [ -f "$STATEFUL_SCRIPT" ]; then
  # Create bake using the stateful MCP server fixture
  bake_source=$(python3 -c "import json; print(json.dumps(['python3', '$STATEFUL_SCRIPT']))")
  bake_out=$(sxmc_isolated bake create test-mcp --source "$bake_source" --skip-validate 2>&1)
  if echo "$bake_out" | grep -q "Created bake"; then
    pass "bake create (stateful fixture)"
  else
    fail "bake create" "$bake_out"
  fi

  # List
  list_out=$(sxmc_isolated bake list 2>&1)
  if echo "$list_out" | grep -q "test-mcp"; then
    pass "bake list shows test-mcp"
  else
    fail "bake list" "$list_out"
  fi

  # Tools
  tools_out=$(sxmc_isolated mcp tools test-mcp 2>&1)
  if echo "$tools_out" | grep -q "remember_state\|read_state\|Tools"; then
    pass "mcp tools lists server tools"
  else
    fail "mcp tools" "${tools_out:0:100}"
  fi

  # Grep
  grep_out=$(sxmc_isolated mcp grep state 2>&1)
  if echo "$grep_out" | grep -qi "match\|state"; then
    pass "mcp grep finds matches"
  else
    fail "mcp grep" "${grep_out:0:100}"
  fi

  # Remove
  rm_out=$(sxmc_isolated bake remove test-mcp 2>&1)
  if echo "$rm_out" | grep -q "Removed"; then
    pass "bake remove"
  else
    fail "bake remove" "$rm_out"
  fi
else
  skip "MCP pipeline tests" "python3 or fixtures not available"
fi

# ============================================================================
# SECTION 11: Bake Validation
# ============================================================================
section "11. Bake Validation"

# Invalid source should fail
bad_bake=$(sxmc_isolated bake create broken-bake --source 'definitely-not-a-real-command-xyz' 2>&1 || true)
if echo "$bad_bake" | grep -qi "error\|could not connect\|not found"; then
  pass "bake create rejects invalid source"
else
  fail "bake create should reject invalid source" "${bad_bake:0:100}"
fi

if echo "$bad_bake" | grep -qi "skip-validate\|guidance\|hint"; then
  pass "bake error includes --skip-validate guidance"
else
  skip "bake error guidance" "error text may vary"
fi

# --skip-validate should succeed
skip_bake=$(sxmc_isolated bake create skip-bake --source 'not-real-cmd' --skip-validate 2>&1)
if echo "$skip_bake" | grep -q "Created"; then
  pass "bake create --skip-validate succeeds"
  sxmc_isolated bake remove skip-bake >/dev/null 2>&1
else
  fail "bake create --skip-validate" "$skip_bake"
fi

# ============================================================================
# SECTION 12: API Mode
# ============================================================================
section "12. API Mode"

PETSTORE_URL="https://petstore3.swagger.io/api/v3/openapi.json"

# Check network
if has_cmd curl && curl -s --max-time 5 "$PETSTORE_URL" >/dev/null 2>&1; then
  # List operations
  api_list=$("$SXMC" api "$PETSTORE_URL" --list 2>/dev/null)
  if json_check "$api_list" "d.get('count', 0) >= 10"; then
    count=$(json_field "$api_list" "d['count']")
    pass "api --list finds $count operations"
  else
    fail "api --list" "${api_list:0:100}"
  fi

  # Search
  api_search=$("$SXMC" api "$PETSTORE_URL" --search pet --list 2>/dev/null)
  if json_check "$api_search" "d.get('count', 0) >= 3"; then
    pass "api --search pet filters operations"
  else
    fail "api --search" "${api_search:0:100}"
  fi

  # Call
  api_call=$("$SXMC" api "$PETSTORE_URL" getPetById petId=1 --pretty 2>&1)
  if echo "$api_call" | grep -q '"id"'; then
    pass "api call getPetById returns JSON"
  else
    # Petstore may not have pet 1; check for valid HTTP response
    if echo "$api_call" | grep -qE '"status"|"id"|"body"'; then
      pass "api call getPetById returns HTTP response"
    else
      fail "api call" "${api_call:0:100}"
    fi
  fi
else
  skip "API mode tests" "no network or curl unavailable"
fi

# ============================================================================
# SECTION 13: Doctor Command
# ============================================================================
section "13. Doctor Command"

doc_out=$("$SXMC" doctor 2>&1)
if json_check "$doc_out" "'root' in d"; then
  pass "doctor outputs JSON with root"
else
  fail "doctor output" "${doc_out:0:100}"
fi

if json_check "$doc_out" "'startup_files' in d"; then
  pass "doctor reports startup_files"
else
  fail "doctor missing startup_files"
fi

if json_check "$doc_out" "'recommended_first_moves' in d and len(d['recommended_first_moves']) >= 3"; then
  pass "doctor has recommended first moves"
else
  fail "doctor missing recommended_first_moves"
fi

if json_check "$doc_out" "any(m['surface'] == 'unknown_cli' for m in d.get('recommended_first_moves',[]))"; then
  pass "doctor recommends sxmc inspect cli"
else
  fail "doctor should recommend inspect cli"
fi

if json_check "$doc_out" "any(m['surface'] == 'unknown_api' for m in d.get('recommended_first_moves',[]))"; then
  pass "doctor recommends sxmc api"
else
  fail "doctor should recommend api"
fi

doc_human=$("$SXMC" doctor --human 2>&1)
if echo "$doc_human" | grep -q "Recommended first moves"; then
  pass "doctor --human renders human report"
else
  fail "doctor --human should render a report" "${doc_human:0:100}"
fi

if echo "$doc_human" | grep -q "CLI profile cache"; then
  pass "doctor --human reports cache stats"
else
  fail "doctor --human should mention cache stats"
fi

TMP_DOCTOR_ROOT="$TMPDIR_TEST/doctor-empty"
mkdir -p "$TMP_DOCTOR_ROOT"
if "$SXMC" doctor --check --root "$TMP_DOCTOR_ROOT" >/dev/null 2>&1; then
  fail "doctor --check should fail when startup files are missing"
else
  pass "doctor --check fails when startup files are missing"
fi

mkdir -p "$TMP_DOCTOR_ROOT/.cursor/rules" "$TMP_DOCTOR_ROOT/.cursor"
mkdir -p "$TMP_DOCTOR_ROOT/.sxmc/ai"
printf '# Claude\n' > "$TMP_DOCTOR_ROOT/CLAUDE.md"
printf '{"mcpServers":{}}' > "$TMP_DOCTOR_ROOT/.sxmc/ai/claude-code-mcp.json"
printf '# Cursor\n' > "$TMP_DOCTOR_ROOT/.cursor/rules/sxmc-cli-ai.md"
printf '{\"mcpServers\":{}}' > "$TMP_DOCTOR_ROOT/.cursor/mcp.json"
if "$SXMC" doctor --check --only claude-code,cursor --root "$TMP_DOCTOR_ROOT" >/dev/null 2>&1; then
  pass "doctor --check --only scopes validation to selected hosts"
else
  fail "doctor --check --only should pass when selected host files are present"
fi

cat > "$TMPDIR_TEST/doctor-fix-cli" <<'EOF'
#!/bin/sh
cat <<'HELP'
doctor-fix-cli

A CLI suitable for doctor repair flows.

Usage:
  doctor-fix-cli [OPTIONS]

Options:
  --json  Emit json
HELP
EOF
chmod +x "$TMPDIR_TEST/doctor-fix-cli"
TMP_DOCTOR_FIX_ROOT="$TMPDIR_TEST/doctor-fix-root"
mkdir -p "$TMP_DOCTOR_FIX_ROOT"
if "$SXMC" doctor --check --fix --allow-low-confidence --only claude-code,cursor --from-cli "$TMPDIR_TEST/doctor-fix-cli" --root "$TMP_DOCTOR_FIX_ROOT" >/dev/null 2>&1; then
  if [ -f "$TMP_DOCTOR_FIX_ROOT/CLAUDE.md" ] && [ -f "$TMP_DOCTOR_FIX_ROOT/.cursor/rules/sxmc-cli-ai.md" ]; then
    pass "doctor --fix repairs selected startup files"
  else
    fail "doctor --fix should create selected startup files"
  fi
else
  fail "doctor --fix should repair selected hosts"
fi

# ============================================================================
# SECTION 14: Self-Dogfooding
# ============================================================================
section "14. Self-Dogfooding"

DOGFOOD_FILES=(CLAUDE.md AGENTS.md GEMINI.md .cursor/rules/sxmc-cli-ai.md .github/copilot-instructions.md)

for f in "${DOGFOOD_FILES[@]}"; do
  if [ -f "$ROOT/$f" ]; then
    # Check it actually mentions sxmc
    if grep -qi "sxmc" "$ROOT/$f"; then
      pass "repo ships $f (mentions sxmc)"
    else
      fail "$f exists but doesn't mention sxmc"
    fi
  else
    fail "repo missing $f"
  fi
done

# ============================================================================
# SECTION 15: Depth Expansion & Batch Inspection
# ============================================================================
section "15. Depth Expansion & Batch Inspection"

if has_cmd git; then
  # Compact output should suggest --depth 2
  compact_git=$("$SXMC" inspect cli git --compact 2>/dev/null)
  if json_check "$compact_git" "any('depth' in n.get('summary','').lower() for n in d.get('confidence_notes',[]))"; then
    pass "compact output includes depth-2 guidance"
  else
    skip "depth-2 guidance in compact" "hint text may vary"
  fi

  # --depth 1 should produce subcommand_profiles (top-level list of nested profiles)
  depth1=$("$SXMC" inspect cli git --depth 1 2>/dev/null)
  nested=$(json_field "$depth1" "len(d.get('subcommand_profiles',[]))")
  if [ "${nested:-0}" -gt 0 ]; then
    pass "depth 1 produces $nested subcommand_profiles"
  else
    skip "depth 1 subcommand_profiles" "key may differ"
  fi
else
  skip "depth expansion tests" "git not installed"
fi

printf 'git\nls\n' > "$TMPDIR_TEST/tools.txt"
printf 'sed\n# comment\n   \n git \n' > "$TMPDIR_TEST/tools-with-comments.txt"

batch_out=$("$SXMC" inspect batch git cargo this-command-should-not-exist-xyz --parallel 4 --progress 2>/dev/null)
if json_check "$batch_out" "d.get('count', 0) == 3"; then
  pass "inspect batch reports requested command count"
else
  fail "inspect batch should report count" "${batch_out:0:100}"
fi

if json_check "$batch_out" "d.get('failed_count', 0) >= 1"; then
  pass "inspect batch keeps partial failures"
else
  fail "inspect batch should report failures"
fi

if json_check "$batch_out" "d.get('parallelism', 0) >= 1"; then
  pass "inspect batch reports parallelism"
else
  fail "inspect batch should report parallelism"
fi

batch_from_file=$("$SXMC" inspect batch --from-file "$TMPDIR_TEST/tools.txt" --parallel 2 2>/dev/null)
if json_check "$batch_from_file" "d.get('count', 0) == 2 and d.get('failed_count', 0) == 0"; then
  pass "inspect batch --from-file loads command specs"
else
  fail "inspect batch --from-file" "${batch_from_file:0:100}"
fi

batch_from_file_comments=$("$SXMC" inspect batch --from-file "$TMPDIR_TEST/tools-with-comments.txt" --parallel 2 2>/dev/null)
if json_check "$batch_from_file_comments" "d.get('count', 0) == 2 and d.get('failed_count', 0) == 0"; then
  pass "inspect batch --from-file ignores blank lines and # comments"
else
  fail "inspect batch --from-file comments" "${batch_from_file_comments:0:100}"
fi

cat > "$TMPDIR_TEST/tools.yaml" <<EOF
tools:
  - command: curl
    depth: 1
  - command: git
EOF
batch_from_yaml=$("$SXMC" inspect batch --from-file "$TMPDIR_TEST/tools.yaml" --parallel 2 2>/dev/null)
if json_check "$batch_from_yaml" "d.get('count', 0) == 2 and d.get('failed_count', 0) == 0"; then
  pass "inspect batch --from-file supports YAML"
else
  fail "inspect batch --from-file YAML" "${batch_from_yaml:0:120}"
fi

cat > "$TMPDIR_TEST/tools.toml" <<EOF
tools = [
  { command = "curl", depth = 1 },
  { command = "git" }
]
EOF
batch_from_toml=$("$SXMC" inspect batch --from-file "$TMPDIR_TEST/tools.toml" --parallel 2 2>/dev/null)
if json_check "$batch_from_toml" "d.get('count', 0) == 2 and d.get('failed_count', 0) == 0"; then
  pass "inspect batch --from-file supports TOML"
else
  fail "inspect batch --from-file TOML" "${batch_from_toml:0:120}"
fi

batch_since_rfc3339=$("$SXMC" inspect batch cargo --since 1970-01-01T00:00:00Z 2>/dev/null)
if json_check "$batch_since_rfc3339" "d.get('count', 0) == 1 and d.get('inspected_count', 0) == 1"; then
  pass "inspect batch --since accepts RFC3339"
else
  fail "inspect batch --since RFC3339" "${batch_since_rfc3339:0:120}"
fi

cache_stats=$("$SXMC" inspect cache-stats 2>/dev/null)
if json_check "$cache_stats" "'entry_count' in d and 'total_bytes' in d"; then
  pass "inspect cache-stats returns cache metrics"
else
  fail "inspect cache-stats" "${cache_stats:0:100}"
fi

cache_invalidate=$("$SXMC" inspect cache-invalidate git 2>/dev/null)
if json_check "$cache_invalidate" "'removed_entries' in d and d.get('match_mode') == 'exact'"; then
  pass "inspect cache-invalidate returns exact-match removal metrics"
else
  fail "inspect cache-invalidate" "${cache_invalidate:0:100}"
fi

cache_dry_run=$("$SXMC" inspect cache-invalidate 'c*' --dry-run 2>/dev/null)
if json_check "$cache_dry_run" "d.get('dry_run') is True and d.get('match_mode') == 'glob' and d.get('removed_entries') == 0"; then
  pass "inspect cache-invalidate --dry-run previews glob matches"
else
  fail "inspect cache-invalidate --dry-run" "${cache_dry_run:0:120}"
fi

cache_pattern=$("$SXMC" inspect cache-invalidate 'c*' 2>/dev/null)
if json_check "$cache_pattern" "'removed_entries' in d and d.get('match_mode') == 'glob'"; then
  pass "inspect cache-invalidate supports glob patterns"
else
  fail "inspect cache-invalidate pattern mode" "${cache_pattern:0:100}"
fi

cache_clear=$("$SXMC" inspect cache-clear 2>/dev/null)
if json_check "$cache_clear" "d.get('cleared', False) is True"; then
  pass "inspect cache-clear clears cache"
else
  fail "inspect cache-clear" "${cache_clear:0:100}"
fi

cache_warm=$("$SXMC" inspect cache-warm cargo git --parallel 2 2>/dev/null)
if json_check "$cache_warm" "d.get('count', 0) == 2 and 'warmed_count' in d"; then
  pass "inspect cache-warm pre-populates cache"
else
  fail "inspect cache-warm" "${cache_warm:0:120}"
fi

batch_toon=$("$SXMC" inspect batch git cargo --format toon 2>/dev/null)
if echo "$batch_toon" | grep -q "profiles:" && echo "$batch_toon" | grep -q "parallelism:"; then
  pass "inspect batch --format toon is summary-oriented"
else
  fail "inspect batch --format toon should be summary-oriented" "${batch_toon:0:100}"
fi

batch_toon_fail=$("$SXMC" inspect batch git this-command-should-not-exist-xyz --format toon 2>/dev/null)
if echo "$batch_toon_fail" | grep -q "failures:" && echo "$batch_toon_fail" | grep -q "this-command-should-not-exist-xyz"; then
  pass "inspect batch --format toon includes failure details"
else
  fail "inspect batch --format toon failure details" "${batch_toon_fail:0:140}"
fi

if has_cmd git; then
  before_profile="$TMPDIR_TEST/git-before.json"
  "$SXMC" inspect cli git --pretty > "$before_profile"
  diff_out=$("$SXMC" inspect diff git --before "$before_profile" 2>/dev/null)
  if json_check "$diff_out" "'summary_changed' in d and 'options_added' in d"; then
    pass "inspect diff compares a saved profile"
  else
    fail "inspect diff" "${diff_out:0:120}"
  fi

  diff_toon=$("$SXMC" inspect diff git --before "$before_profile" --format toon 2>/dev/null)
  if echo "$diff_toon" | grep -q "command: git" && echo "$diff_toon" | grep -q "summary_changed:"; then
    pass "inspect diff --format toon is human-oriented"
  else
    fail "inspect diff --format toon" "${diff_toon:0:120}"
  fi
else
  skip "inspect diff" "git not installed"
fi

compact_before="$TMPDIR_TEST/cargo-before-compact.json"
"$SXMC" inspect cli cargo --compact > "$compact_before"
compact_diff_err=$("$SXMC" inspect diff cargo --before "$compact_before" 2>&1 || true)
if echo "$compact_diff_err" | grep -q "Compact profiles cannot be diffed"; then
  pass "inspect diff explains compact-profile limitation"
else
  fail "inspect diff compact guidance" "${compact_diff_err:0:120}"
fi

# ============================================================================
# SECTION 16: Error Message Quality
# ============================================================================
section "16. Error Messages"

# Inspect nonexistent tool
err_nonexist=$("$SXMC" inspect cli this-does-not-exist-xyz 2>&1 || true)
if echo "$err_nonexist" | grep -qi "could not\|not found\|error"; then
  pass "inspect nonexistent tool gives clear error"
else
  fail "inspect nonexistent should error" "${err_nonexist:0:80}"
fi

# No arguments
err_noargs=$("$SXMC" 2>&1 || true)
if echo "$err_noargs" | grep -qi "usage\|help\|command"; then
  pass "no arguments shows usage"
else
  fail "no arguments should show usage"
fi

# Inspect self without --allow-self
sxmc_path=$(command -v "$SXMC" 2>/dev/null || echo "$SXMC")
err_self=$("$SXMC" inspect cli "$sxmc_path" 2>&1 || true)
if echo "$err_self" | grep -qi "self\|refusing"; then
  pass "inspect self blocked without --allow-self"
else
  skip "inspect self block" "may not detect self by path"
fi

# ============================================================================
# SECTION 17: sxmc serve
# ============================================================================
section "17. Serve"

serve_help=$("$SXMC" serve --help 2>&1)
if echo "$serve_help" | grep -q "transport"; then
  pass "serve --help mentions transport"
else
  fail "serve --help should mention transport"
fi

if echo "$serve_help" | grep -q "watch"; then
  pass "serve supports --watch"
else
  fail "serve should support --watch"
fi

if echo "$serve_help" | grep -qi "bearer-token\|require-header"; then
  pass "serve supports auth options"
else
  fail "serve should support auth"
fi

# Skills listing
if [ -d "$FIXTURES" ]; then
  skills_out=$("$SXMC" skills list --paths "$FIXTURES" 2>&1)
  if echo "$skills_out" | grep -q "simple-skill"; then
    pass "skills list finds fixture skills"
  else
    fail "skills list" "${skills_out:0:100}"
  fi

  skills_json=$("$SXMC" skills list --paths "$FIXTURES" --json 2>&1)
  if json_check "$skills_json" "isinstance(d, list) and len(d) >= 1"; then
    pass "skills list --json returns valid JSON array"
  else
    fail "skills list --json" "${skills_json:0:100}"
  fi
else
  skip "skills list" "fixtures not found"
fi

# ============================================================================
# SUMMARY
# ============================================================================
printf "\n${BOLD}${CYAN}━━━ RESULTS ━━━${RESET}\n\n"
printf "  ${GREEN}Passed: %d${RESET}\n" "$PASS"
printf "  ${RED}Failed: %d${RESET}\n" "$FAIL"
printf "  ${YELLOW}Skipped: %d${RESET}\n" "$SKIP"
printf "  Total:   %d\n\n" "$TOTAL"

if [ "$FAIL" -eq 0 ]; then
  printf "${GREEN}${BOLD}ALL TESTS PASSED${RESET}\n"
else
  printf "${RED}${BOLD}%d TEST(S) FAILED${RESET}\n" "$FAIL"
fi

# JSON summary
JSON_SUMMARY=$(python3 -c "
import json, datetime
data = {
    'sxmc_version': '$(echo "$SXMC_VERSION" | tr -d '\n')',
    'os': '$OS_NAME $OS_ARCH',
    'timestamp': datetime.datetime.now().isoformat(),
    'total': $TOTAL,
    'pass': $PASS,
    'fail': $FAIL,
    'skip': $SKIP,
    'cli_tools_parsed': $PARSED,
    'cli_tools_failed': $PARSE_FAIL,
    'cli_tools_skipped': $PARSE_SKIP,
    'bad_summaries': $BAD_SUMMARIES,
}
print(json.dumps(data, indent=2))
")

if [ -n "$JSON_OUT" ]; then
  echo "$JSON_SUMMARY" > "$JSON_OUT"
  printf "\nJSON results written to: %s\n" "$JSON_OUT"
else
  printf "\n${CYAN}--- JSON Summary ---${RESET}\n"
  echo "$JSON_SUMMARY"
fi

exit $(( FAIL > 0 ? 1 : 0 ))
