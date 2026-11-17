#!/usr/bin/env bash
# Records what a pipeline does to start the change and check it answers, as the verify.json a revui bundle shows on
# its verification card. `revui ci verify` prints this file; a job's own script sources it:
#
#   source <(revui ci verify)
#   step install npm ci
#   step start   ./scripts/start-local.sh
#   check 'GET /health' "$(curl -s -o /dev/null -w '%{http_code}' localhost:3000/health)"
#   verified
#
# It writes into $VERIFY_OUT (default: sandbox): <step>.log for each step, and verify.json. Hand that directory to
# the reviewer as GUIDE_SANDBOX and it tests the change there too.
VERIFY_OUT=${VERIFY_OUT:-sandbox}
mkdir -p "$VERIFY_OUT"
: >"$VERIFY_OUT/steps.jsonl"
: >"$VERIFY_OUT/checks.jsonl"
verify_failed=0

# step <name> <command...>: runs it, keeping its output and how long it took. A step that fails is recorded and the
# later ones still run, so one run shows everything that is wrong.
step() {
  local name=$1 start=$SECONDS ok=true
  shift
  "$@" >"$VERIFY_OUT/$name.log" 2>&1 || ok=false
  jq -cn --arg step "$name" --argjson ok "$ok" --argjson seconds "$((SECONDS - start))" '{step: $step, ok: $ok, seconds: $seconds}' >>"$VERIFY_OUT/steps.jsonl"
  echo "$name: $([[ $ok == true ]] && echo ok || echo FAILED) in $((SECONDS - start)) s"
  if [[ $ok == false ]]; then
    verify_failed=1
    tail -n 25 "$VERIFY_OUT/$name.log" | sed 's/^/    /'
  fi
}

# check <name> <status> [detail]: something asked of the running change, and what came back. It passed when the
# status is 200; give the response or the output as the detail.
check() {
  jq -cn --arg name "$1" --arg status "$2" --arg detail "${3:-}" '{name: $name, status: $status, detail: $detail}' >>"$VERIFY_OUT/checks.jsonl"
  [[ $2 == 200 ]] || verify_failed=1
}

# verified [address]: writes verify.json, and returns 1 when a step or a check failed.
verified() {
  jq -n --slurpfile steps "$VERIFY_OUT/steps.jsonl" --slurpfile checks "$VERIFY_OUT/checks.jsonl" --arg api "${1:-}" \
    '{steps: $steps, checks: $checks} + (if $api == "" then {} else {api: $api} end)' >"$VERIFY_OUT/verify.json"
  rm -f "$VERIFY_OUT/steps.jsonl" "$VERIFY_OUT/checks.jsonl"
  return "$verify_failed"
}
