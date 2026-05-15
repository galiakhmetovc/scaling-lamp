#!/bin/sh
set -eu

PROGRAM=$(basename "$0")
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)

DRY_RUN=0
RESTART_SERVICES=1
SSH_OPTS=${TEAMD_BINARY_DEPLOY_SSH_OPTS:-}
SCP_OPTS=${TEAMD_BINARY_DEPLOY_SCP_OPTS:-}
SSH_PASSWORD=${TEAMD_BINARY_DEPLOY_PASSWORD:-${SSHPASS:-}}
LOCAL_BINARY=${TEAMD_BINARY_DEPLOY_LOCAL_BINARY:-$REPO_ROOT/target/release/agentd}
REMOTE_BIN=${TEAMD_BINARY_DEPLOY_REMOTE_BIN:-/opt/teamd/bin/agentd}
REMOTE_PATH_LINK=${TEAMD_BINARY_DEPLOY_PATH_LINK:-/usr/local/bin/agentd}
DAEMON_SERVICE=${TEAMD_BINARY_DEPLOY_DAEMON_SERVICE:-teamd-daemon.service}
TELEGRAM_SERVICE=${TEAMD_BINARY_DEPLOY_TELEGRAM_SERVICE:-teamd-telegram.service}
REMOTE_TMP_DIR=${TEAMD_BINARY_DEPLOY_TMP_DIR:-/tmp}
REMOTE_BACKUP_DIR=${TEAMD_BINARY_DEPLOY_BACKUP_DIR:-/opt/teamd/backups}

usage() {
  cat <<EOF
Usage: $PROGRAM [options] <ssh-target> [local-binary]

Install an already built local agentd binary on a systemd host without building
on the production server.

Options:
  --dry-run       Print commands without changing the remote host.
  --no-restart    Install the binary but do not restart systemd services.
  -h, --help      Show this help.

Environment overrides:
  TEAMD_BINARY_DEPLOY_LOCAL_BINARY   Local binary, default: $LOCAL_BINARY.
  TEAMD_BINARY_DEPLOY_REMOTE_BIN     Remote install path, default: $REMOTE_BIN.
  TEAMD_BINARY_DEPLOY_PATH_LINK      Remote PATH symlink, default: $REMOTE_PATH_LINK.
  TEAMD_BINARY_DEPLOY_DAEMON_SERVICE Daemon unit, default: $DAEMON_SERVICE.
  TEAMD_BINARY_DEPLOY_TELEGRAM_SERVICE
                                     Telegram unit, default: $TELEGRAM_SERVICE.
  TEAMD_BINARY_DEPLOY_SSH_OPTS       Extra ssh options as a single string.
  TEAMD_BINARY_DEPLOY_SCP_OPTS       Extra scp options as a single string.
  TEAMD_BINARY_DEPLOY_PASSWORD       Optional SSH password. Prefer keys; if set,
                                     sshpass is used for both scp and ssh.

Examples:
  cargo build --release -p agentd
  $PROGRAM root@31.130.128.89
  $PROGRAM --no-restart root@31.130.128.89 ./target/release/agentd
EOF
}

fail() {
  printf 'error: %s\n' "$1" >&2
  exit 1
}

quote_arg() {
  printf "'%s'" "$(printf '%s' "$1" | sed "s/'/'\\\\''/g")"
}

print_cmd() {
  printf '+'
  for arg in "$@"; do
    printf ' %s' "$(quote_arg "$arg")"
  done
  printf '\n'
}

run_cmd() {
  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd "$@"
  else
    "$@"
  fi
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    --no-restart)
      RESTART_SERVICES=0
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    --)
      shift
      break
      ;;
    -*)
      fail "unknown option: $1"
      ;;
    *)
      break
      ;;
  esac
done

[ "$#" -ge 1 ] || {
  usage >&2
  exit 1
}

SSH_TARGET=$1
shift
if [ "$#" -gt 0 ]; then
  LOCAL_BINARY=$1
  shift
fi
[ "$#" -eq 0 ] || fail "unexpected extra arguments"

[ -f "$LOCAL_BINARY" ] || fail "local binary not found: $LOCAL_BINARY"
[ -x "$LOCAL_BINARY" ] || fail "local binary is not executable: $LOCAL_BINARY"
command -v ssh >/dev/null 2>&1 || fail "required command not found: ssh"
command -v scp >/dev/null 2>&1 || fail "required command not found: scp"
if [ -n "$SSH_PASSWORD" ] && [ "$DRY_RUN" -eq 0 ]; then
  command -v sshpass >/dev/null 2>&1 || fail "TEAMD_BINARY_DEPLOY_PASSWORD requires sshpass"
fi

REMOTE_TMP="$REMOTE_TMP_DIR/agentd.$(date +%s).$$"
REMOTE_SCRIPT=$(cat <<EOF
set -eu
SUDO=
if [ "\$(id -u)" -ne 0 ]; then
  command -v sudo >/dev/null 2>&1 || { echo "sudo is required for non-root deploy user" >&2; exit 1; }
  SUDO=sudo
fi
remote_bin=$(quote_arg "$REMOTE_BIN")
remote_link=$(quote_arg "$REMOTE_PATH_LINK")
remote_tmp=$(quote_arg "$REMOTE_TMP")
backup_dir=$(quote_arg "$REMOTE_BACKUP_DIR")
daemon_service=$(quote_arg "$DAEMON_SERVICE")
telegram_service=$(quote_arg "$TELEGRAM_SERVICE")
\$SUDO mkdir -p "\$(dirname "\$remote_bin")" "\$(dirname "\$remote_link")" "\$backup_dir"
if [ -x "\$remote_bin" ]; then
  backup="\$backup_dir/agentd.\$(date +%Y%m%d%H%M%S)"
  \$SUDO cp -a "\$remote_bin" "\$backup"
  echo "backup=\$backup"
fi
\$SUDO install -m 0755 -o root -g root "\$remote_tmp" "\$remote_bin"
\$SUDO rm -f "\$remote_tmp"
\$SUDO ln -sf "\$remote_bin" "\$remote_link"
if [ -r /etc/teamd/teamd.env ]; then
  set -a
  . /etc/teamd/teamd.env
  set +a
fi
"\$remote_bin" version || true
if [ "$RESTART_SERVICES" -eq 1 ]; then
  \$SUDO systemctl daemon-reload
  \$SUDO systemctl restart "\$daemon_service"
  if \$SUDO systemctl is-enabled --quiet "\$telegram_service" 2>/dev/null || \$SUDO systemctl is-active --quiet "\$telegram_service" 2>/dev/null; then
    \$SUDO systemctl restart "\$telegram_service"
  else
    echo "telegram service restart skipped: \$telegram_service is disabled and inactive"
  fi
  \$SUDO systemctl is-active --quiet "\$daemon_service"
  \$SUDO systemctl --no-pager --lines=0 status "\$daemon_service" || true
  if \$SUDO systemctl is-enabled --quiet "\$telegram_service" 2>/dev/null || \$SUDO systemctl is-active --quiet "\$telegram_service" 2>/dev/null; then
    \$SUDO systemctl --no-pager --lines=0 status "\$telegram_service" || true
  fi
else
  echo "service restart skipped"
fi
EOF
)

printf 'Deploying %s to %s:%s\n' "$LOCAL_BINARY" "$SSH_TARGET" "$REMOTE_BIN"

if [ -n "$SSH_PASSWORD" ]; then
  export SSHPASS=$SSH_PASSWORD
  # shellcheck disable=SC2086
  run_cmd sshpass -e scp $SCP_OPTS "$LOCAL_BINARY" "$SSH_TARGET:$REMOTE_TMP"

  # shellcheck disable=SC2086
  run_cmd sshpass -e ssh $SSH_OPTS "$SSH_TARGET" "$REMOTE_SCRIPT"
else
  # shellcheck disable=SC2086
  run_cmd scp $SCP_OPTS "$LOCAL_BINARY" "$SSH_TARGET:$REMOTE_TMP"

  # shellcheck disable=SC2086
  run_cmd ssh $SSH_OPTS "$SSH_TARGET" "$REMOTE_SCRIPT"
fi
