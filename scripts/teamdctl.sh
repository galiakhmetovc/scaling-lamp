#!/bin/sh
set -eu

PROGRAM=$(basename "$0")
RUN_USER=${TEAMD_RUN_USER:-teamd}
ENV_FILE=${TEAMD_ENV_FILE:-/etc/teamd/teamd.env}
AGENTD_BIN=${TEAMD_AGENTD_BIN:-/opt/teamd/bin/agentd}
DAEMON_SERVICE=${TEAMD_DAEMON_SERVICE:-teamd-daemon.service}
TELEGRAM_SERVICE=${TEAMD_TELEGRAM_SERVICE:-teamd-telegram.service}

usage() {
  cat <<EOF
Usage:
  $PROGRAM <agentd-command> [args...]
  $PROGRAM daemon|telegram status|start|stop|restart|enable|disable|logs|follow
  $PROGRAM service daemon|telegram status|start|stop|restart|enable|disable|logs|follow

Examples:
  $PROGRAM version
  $PROGRAM logs 200
  $PROGRAM telegram pair tg...
  $PROGRAM session list
  $PROGRAM session list --raw
  $PROGRAM session tools <session_id> --limit 50 --offset 0
  $PROGRAM session tools <session_id> --raw --limit 50 --offset 0
  $PROGRAM daemon restart
  $PROGRAM telegram logs
EOF
}

fail() {
  printf '%s: %s\n' "$PROGRAM" "$1" >&2
  exit 1
}

service_unit() {
  case "$1" in
    daemon|teamd) printf '%s\n' "$DAEMON_SERVICE" ;;
    telegram|bot) printf '%s\n' "$TELEGRAM_SERVICE" ;;
    *) fail "unknown service: $1" ;;
  esac
}

is_service_action() {
  case "$1" in
    status|start|stop|restart|enable|disable|logs|follow) return 0 ;;
    *) return 1 ;;
  esac
}

run_privileged() {
  if [ "$(id -u)" -eq 0 ]; then
    exec "$@"
  fi
  if command -v sudo >/dev/null 2>&1; then
    exec sudo "$@"
  fi
  exec "$@"
}

service_command() {
  [ "$#" -ge 2 ] || fail "service command requires service and action"
  unit=$(service_unit "$1")
  action=$2
  shift 2

  case "$action" in
    status)
      run_privileged systemctl status "$unit" "$@"
      ;;
    start|stop|restart|enable|disable)
      run_privileged systemctl "$action" "$unit" "$@"
      ;;
    logs)
      run_privileged journalctl -u "$unit" -n "${1:-100}" --no-pager
      ;;
    follow)
      run_privileged journalctl -u "$unit" -f
      ;;
    *)
      fail "unknown service action: $action"
      ;;
  esac
}

run_agentd_here() {
  [ -r "$ENV_FILE" ] || fail "cannot read env file: $ENV_FILE"
  [ -x "$AGENTD_BIN" ] || fail "agentd binary is not executable: $AGENTD_BIN"
  set -a
  . "$ENV_FILE"
  set +a
  exec "$AGENTD_BIN" "$@"
}

run_agentd_as_service_user() {
  if [ "$(id -un 2>/dev/null || true)" = "$RUN_USER" ]; then
    run_agentd_here "$@"
  fi

  if command -v sudo >/dev/null 2>&1; then
    exec sudo -u "$RUN_USER" env \
      TEAMD_ENV_FILE="$ENV_FILE" \
      TEAMD_AGENTD_BIN="$AGENTD_BIN" \
      sh -c '
        set -eu
        [ -r "$TEAMD_ENV_FILE" ] || { echo "teamdctl: cannot read env file: $TEAMD_ENV_FILE" >&2; exit 1; }
        [ -x "$TEAMD_AGENTD_BIN" ] || { echo "teamdctl: agentd binary is not executable: $TEAMD_AGENTD_BIN" >&2; exit 1; }
        set -a
        . "$TEAMD_ENV_FILE"
        set +a
        exec "$TEAMD_AGENTD_BIN" "$@"
      ' sh "$@"
  fi

  if [ "$(id -u)" -eq 0 ] && command -v runuser >/dev/null 2>&1; then
    exec runuser -u "$RUN_USER" -- env \
      TEAMD_ENV_FILE="$ENV_FILE" \
      TEAMD_AGENTD_BIN="$AGENTD_BIN" \
      sh -c '
        set -eu
        [ -r "$TEAMD_ENV_FILE" ] || { echo "teamdctl: cannot read env file: $TEAMD_ENV_FILE" >&2; exit 1; }
        [ -x "$TEAMD_AGENTD_BIN" ] || { echo "teamdctl: agentd binary is not executable: $TEAMD_AGENTD_BIN" >&2; exit 1; }
        set -a
        . "$TEAMD_ENV_FILE"
        set +a
        exec "$TEAMD_AGENTD_BIN" "$@"
      ' sh "$@"
  fi

  fail "need sudo or root+runuser to execute agentd as $RUN_USER"
}

if [ "$#" -eq 0 ]; then
  usage
  exit 0
fi

case "$1" in
  -h|--help|help)
    usage
    ;;
  daemon)
    service_command "$@"
    ;;
  telegram)
    if [ "$#" -ge 2 ] && is_service_action "$2"; then
      service_command "$@"
    else
      run_agentd_as_service_user "$@"
    fi
    ;;
  service)
    shift
    service_command "$@"
    ;;
  *)
    run_agentd_as_service_user "$@"
    ;;
esac
