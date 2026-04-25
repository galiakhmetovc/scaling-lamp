#!/bin/sh
set -eu

PROGRAM=$(basename "$0")
HOST=${TEAMD_DIAG_HOST:-teamd-ams1}
OUT_ROOT=${TEAMD_DIAG_OUTPUT:-diagnostics}
SESSION_ID=
LOG_LINES=${TEAMD_DIAG_LOG_LINES:-800}
JOURNAL_LINES=${TEAMD_DIAG_JOURNAL_LINES:-500}
TOOL_LIMIT=${TEAMD_DIAG_TOOL_LIMIT:-1000}
STATE_DIR=${TEAMD_DIAG_STATE_DIR:-/var/lib/teamd/state}
LOCAL_MODE=false

usage() {
  cat <<EOF
Usage:
  $PROGRAM [--local] [--host ssh-host] [--session session-id] [--output dir]
           [--log-lines n] [--journal-lines n] [--tool-limit n]

Defaults:
  --host          $HOST
  --output        $OUT_ROOT
  --log-lines     $LOG_LINES
  --journal-lines $JOURNAL_LINES
  --tool-limit    $TOOL_LIMIT

The script collects production diagnostics through teamdctl/journalctl over SSH.
Use --local to run the same collection commands on the current machine.
If --session is omitted, the session with the largest updated_at is selected.
EOF
}

fail() {
  printf '%s: %s\n' "$PROGRAM" "$1" >&2
  exit 1
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    -h|--help)
      usage
      exit 0
      ;;
    --host)
      [ "$#" -ge 2 ] || fail "--host requires a value"
      HOST=$2
      shift 2
      ;;
    --local)
      LOCAL_MODE=true
      HOST=local
      shift
      ;;
    --session)
      [ "$#" -ge 2 ] || fail "--session requires a value"
      SESSION_ID=$2
      shift 2
      ;;
    --output)
      [ "$#" -ge 2 ] || fail "--output requires a value"
      OUT_ROOT=$2
      shift 2
      ;;
    --log-lines)
      [ "$#" -ge 2 ] || fail "--log-lines requires a value"
      LOG_LINES=$2
      shift 2
      ;;
    --journal-lines)
      [ "$#" -ge 2 ] || fail "--journal-lines requires a value"
      JOURNAL_LINES=$2
      shift 2
      ;;
    --tool-limit)
      [ "$#" -ge 2 ] || fail "--tool-limit requires a value"
      TOOL_LIMIT=$2
      shift 2
      ;;
    *)
      fail "unknown argument: $1"
      ;;
  esac
done

case "$LOG_LINES:$JOURNAL_LINES:$TOOL_LIMIT" in
  *[!0-9:]*|:*|*:|*::*)
    fail "line and limit values must be positive integers"
    ;;
esac

ssh_remote() {
  if [ "$LOCAL_MODE" = true ] || [ "$HOST" = local ] || [ "$HOST" = localhost ]; then
    sh -c "$1"
  else
    ssh -o BatchMode=yes -o ConnectTimeout=10 "$HOST" "$@"
  fi
}

timestamp=$(date -u +%Y%m%dT%H%M%SZ)
safe_host=$(printf '%s' "$HOST" | tr -c 'A-Za-z0-9_.-' '_')
OUT_DIR=$OUT_ROOT/teamd-diagnostics-$safe_host-$timestamp
mkdir -p "$OUT_DIR"

MANIFEST=$OUT_DIR/manifest.txt
{
  printf 'created_at=%s\n' "$timestamp"
  printf 'host=%s\n' "$HOST"
  printf 'state_dir=%s\n' "$STATE_DIR"
  printf 'log_lines=%s\n' "$LOG_LINES"
  printf 'journal_lines=%s\n' "$JOURNAL_LINES"
  printf 'tool_limit=%s\n' "$TOOL_LIMIT"
} >"$MANIFEST"

capture() {
  name=$1
  shift
  stdout=$OUT_DIR/$name
  stderr=$OUT_DIR/$name.stderr
  printf 'collect %s\n' "$name" >&2
  if ssh_remote "$@" >"$stdout" 2>"$stderr"; then
    printf 'ok %s\n' "$name" >>"$MANIFEST"
  else
    status=$?
    printf 'failed %s status=%s stderr=%s\n' "$name" "$status" "$stderr" >>"$MANIFEST"
    return 0
  fi
}

capture host.txt '
set -eu
echo "== host =="
hostname
date -Is
uname -a
uptime
echo "== binaries =="
command -v teamdctl || true
command -v agentd || true
/opt/teamd/bin/agentd version 2>/dev/null || true
teamdctl version 2>/dev/null || true
echo "== services =="
systemctl list-units --all --no-pager "*teamd*" "*agentd*" || true
echo "== processes =="
ps -eo pid,ppid,user,stat,etime,cmd | grep -E "[a]gentd|[t]eamd" || true
echo "== resources =="
df -h / /var/lib/teamd 2>/dev/null || true
free -h 2>/dev/null || true
'

capture services.txt '
systemctl status teamd-daemon.service teamd-telegram.service --no-pager -l 2>&1 || true
'

capture journal-daemon.txt "journalctl -u teamd-daemon.service -n '$JOURNAL_LINES' --no-pager -o short-iso 2>&1 || true"
capture journal-telegram.txt "journalctl -u teamd-telegram.service -n '$JOURNAL_LINES' --no-pager -o short-iso 2>&1 || true"
capture audit-runtime-tail.jsonl "tail -n '$LOG_LINES' '$STATE_DIR/audit/runtime.jsonl' 2>/dev/null || true"
capture audit-errors-tail.jsonl "grep -E '\"level\":\"(error|warn)\"' '$STATE_DIR/audit/runtime.jsonl' 2>/dev/null | tail -n '$LOG_LINES' || true"
capture sessions.txt 'teamdctl session list 2>&1 || true'
capture sessions.raw.txt 'teamdctl session list --raw 2>&1 || true'

if [ -z "$SESSION_ID" ]; then
  SESSION_ID=$(
    awk '
      /^session / {
        id = "";
        updated = "";
        for (i = 1; i <= NF; i++) {
          if ($i ~ /^id=/) {
            id = substr($i, 4);
          }
          if ($i ~ /^updated_at=/) {
            updated = substr($i, 12);
          }
        }
        if (id != "" && updated + 0 >= best + 0) {
          best = updated;
          best_id = id;
        }
      }
      END {
        if (best_id != "") {
          print best_id;
        }
      }
    ' "$OUT_DIR/sessions.raw.txt"
  )
fi

[ -n "$SESSION_ID" ] || fail "cannot determine latest session id"
printf 'session_id=%s\n' "$SESSION_ID" >>"$MANIFEST"
printf '%s\n' "$SESSION_ID" >"$OUT_DIR/session-id.txt"

capture session-show.txt "teamdctl session show '$SESSION_ID' 2>&1 || true"
capture session-transcript.txt "teamdctl session transcript '$SESSION_ID' 2>&1 || true"
capture session-tools.txt "teamdctl session tools '$SESSION_ID' --results --limit '$TOOL_LIMIT' --offset 0 2>&1 || true"
capture session-tools.raw.txt "teamdctl session tools '$SESSION_ID' --raw --results --limit '$TOOL_LIMIT' --offset 0 2>&1 || true"
capture session-debug.json "curl -fsS 'http://127.0.0.1:5140/v1/sessions/$SESSION_ID/debug' 2>&1 || true"
capture session-audit-grep.jsonl "grep -F '$SESSION_ID' '$STATE_DIR/audit/runtime.jsonl' 2>/dev/null | tail -n '$LOG_LINES' || true"

run_ids=$(
  grep -hEo "run-[A-Za-z0-9_.:-]*$SESSION_ID-[0-9]+" \
    "$OUT_DIR/session-transcript.txt" \
    "$OUT_DIR/session-tools.raw.txt" 2>/dev/null | sort -u || true
)
printf 'collect session-runs.txt\n' >&2
: >"$OUT_DIR/session-runs.txt"
: >"$OUT_DIR/session-runs.txt.stderr"
if [ -n "$run_ids" ]; then
  for run_id in $run_ids; do
    {
      printf '== %s ==\n' "$run_id"
      ssh_remote "teamdctl run show '$run_id' 2>&1 || true"
      printf '\n'
    } >>"$OUT_DIR/session-runs.txt" 2>>"$OUT_DIR/session-runs.txt.stderr"
  done
fi
printf 'ok session-runs.txt\n' >>"$MANIFEST"

printf 'collect session-payloads.tar.gz\n' >&2
if ssh_remote "set -eu
tmp=\$(mktemp)
trap 'rm -f \"\$tmp\"' EXIT
cd '$STATE_DIR'
if [ -d 'transcripts/$SESSION_ID' ]; then
  find 'transcripts/$SESSION_ID' -type f >>\"\$tmp\"
fi
find transcripts -maxdepth 1 -type f -name '*$SESSION_ID*' >>\"\$tmp\" 2>/dev/null || true
find artifacts -maxdepth 1 -type f -name '*$SESSION_ID*' >>\"\$tmp\" 2>/dev/null || true
if [ -s \"\$tmp\" ]; then
  tar -czf - -T \"\$tmp\"
else
  tar -czf - --files-from /dev/null
fi
" >"$OUT_DIR/session-payloads.tar.gz" 2>"$OUT_DIR/session-payloads.tar.gz.stderr"; then
  printf 'ok session-payloads.tar.gz\n' >>"$MANIFEST"
else
  status=$?
  printf 'failed session-payloads.tar.gz status=%s stderr=%s\n' "$status" "$OUT_DIR/session-payloads.tar.gz.stderr" >>"$MANIFEST"
fi

printf 'diagnostics_dir=%s\n' "$OUT_DIR" >>"$MANIFEST"
archive=$OUT_DIR.tar.gz
tar -czf "$archive" -C "$OUT_ROOT" "$(basename "$OUT_DIR")"
printf 'diagnostics_archive=%s\n' "$archive" >>"$MANIFEST"
printf '%s\n' "$OUT_DIR"
printf '%s\n' "$archive"
