#!/bin/sh
set -eu

PROGRAM=$(basename "$0")
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)

DRY_RUN=0
NON_INTERACTIVE=0
SKIP_BUILD=0
SKIP_START=0
INSTALL_RUST=${TEAMD_DEPLOY_INSTALL_RUST:-1}
INSTALL_SYSTEM_DEPS=${TEAMD_DEPLOY_INSTALL_SYSTEM_DEPS:-1}
MIN_RUST_VERSION=${TEAMD_DEPLOY_MIN_RUST_VERSION:-1.85.0}
OVERWRITE_CONFIG=0
OVERWRITE_ENV=0
ASSUME_YES=${TEAMD_DEPLOY_ASSUME_YES:-0}

INSTALL_PREFIX=${TEAMD_DEPLOY_INSTALL_PREFIX:-/opt/teamd}
BIN_DIR=${TEAMD_DEPLOY_BIN_DIR:-$INSTALL_PREFIX/bin}
PATH_LINK=${TEAMD_DEPLOY_PATH_LINK:-/usr/local/bin/agentd}
CTL_BIN=${TEAMD_DEPLOY_CTL_BIN:-/usr/local/bin/teamdctl}
CONFIG_DIR=${TEAMD_DEPLOY_CONFIG_DIR:-/etc/teamd}
CONFIG_FILE=${TEAMD_DEPLOY_CONFIG_FILE:-$CONFIG_DIR/config.toml}
ENV_FILE=${TEAMD_DEPLOY_ENV_FILE:-$CONFIG_DIR/teamd.env}
WORK_DIR=${TEAMD_DEPLOY_WORK_DIR:-/var/lib/teamd}
DATA_DIR=${TEAMD_DEPLOY_DATA_DIR:-$WORK_DIR/state}
SERVICE_USER=${TEAMD_DEPLOY_USER:-teamd}
SERVICE_GROUP=${TEAMD_DEPLOY_GROUP:-$SERVICE_USER}
DAEMON_SERVICE=${TEAMD_DEPLOY_DAEMON_SERVICE:-teamd-daemon.service}
TELEGRAM_SERVICE=${TEAMD_DEPLOY_TELEGRAM_SERVICE:-teamd-telegram.service}
PROVIDER_KIND=${TEAMD_PROVIDER_KIND:-zai_chat_completions}
PROVIDER_API_BASE=${TEAMD_PROVIDER_API_BASE:-https://api.z.ai/api/coding/paas/v4}
PROVIDER_MODEL=${TEAMD_PROVIDER_MODEL:-glm-5-turbo}
TELEGRAM_TOKEN=${TEAMD_TELEGRAM_BOT_TOKEN:-}
PROVIDER_KEY=${TEAMD_PROVIDER_API_KEY:-}
CARGO_HOME=${CARGO_HOME:-$HOME/.cargo}
RUSTUP_HOME=${RUSTUP_HOME:-$HOME/.rustup}
RUSTUP_INIT_URL=${TEAMD_DEPLOY_RUSTUP_INIT_URL:-https://sh.rustup.rs}
CONFIG_PARENT=$(dirname "$CONFIG_FILE")
ENV_PARENT=$(dirname "$ENV_FILE")
PATH_LINK_PARENT=$(dirname "$PATH_LINK")
CTL_BIN_PARENT=$(dirname "$CTL_BIN")
CTL_SCRIPT="$REPO_ROOT/scripts/teamdctl.sh"
CARGO_BIN=

usage() {
  cat <<EOF
Usage: $PROGRAM [options]

Build and deploy teamD agentd with daemon + Telegram systemd services.

Options:
  --dry-run           Print actions without changing the system.
  --non-interactive   Do not prompt; require secrets from environment.
  --no-build          Do not run cargo build --release -p agentd.
  --no-install-rust   Fail if cargo/rustc are missing instead of installing Rust.
  --no-install-system-deps
                      Fail if native build dependencies are missing.
  --no-start          Install files but do not enable/start services.
  --overwrite-config  Replace existing $CONFIG_FILE.
  --overwrite-env     Replace existing $ENV_FILE.
  -y, --yes           Assume yes for overwrite prompts.
  -h, --help          Show this help.

Environment overrides:
  TEAMD_TELEGRAM_BOT_TOKEN       Telegram bot token.
  TEAMD_PROVIDER_API_KEY         Z.ai/API provider key.
  TEAMD_PROVIDER_KIND            Provider kind, default: $PROVIDER_KIND.
  TEAMD_PROVIDER_API_BASE        Provider API base, default: $PROVIDER_API_BASE.
  TEAMD_PROVIDER_MODEL           Provider model, default: $PROVIDER_MODEL.
  TEAMD_DEPLOY_INSTALL_RUST      Auto-install Rust when missing, default: $INSTALL_RUST.
  TEAMD_DEPLOY_INSTALL_SYSTEM_DEPS
                                 Auto-install pkg-config/OpenSSL/build deps, default: $INSTALL_SYSTEM_DEPS.
  TEAMD_DEPLOY_MIN_RUST_VERSION  Minimum cargo/rustc version, default: $MIN_RUST_VERSION.
  TEAMD_DEPLOY_RUSTUP_INIT_URL   rustup installer URL, default: $RUSTUP_INIT_URL.
  TEAMD_DEPLOY_INSTALL_PREFIX    Install prefix, default: $INSTALL_PREFIX.
  TEAMD_DEPLOY_PATH_LINK         agentd PATH symlink, default: $PATH_LINK.
  TEAMD_DEPLOY_CTL_BIN           teamdctl helper path, default: $CTL_BIN.
  TEAMD_DEPLOY_CONFIG_FILE       Config path, default: $CONFIG_FILE.
  TEAMD_DEPLOY_ENV_FILE          Environment file, default: $ENV_FILE.
  TEAMD_DEPLOY_DATA_DIR          Runtime state dir, default: $DATA_DIR.
  TEAMD_DEPLOY_USER              Service user, default: $SERVICE_USER.
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

run_root() {
  if [ "$(id -u)" -eq 0 ]; then
    run_cmd "$@"
  else
    run_cmd sudo "$@"
  fi
}

chown_work_dir() {
  attempt=1
  while [ "$attempt" -le 3 ]; do
    if run_root chown -R "$SERVICE_USER:$SERVICE_GROUP" "$WORK_DIR"; then
      return 0
    fi
    if [ "$attempt" -lt 3 ]; then
      printf 'Retrying ownership update for %s after transient filesystem change...\n' "$WORK_DIR" >&2
      sleep 1
    fi
    attempt=$((attempt + 1))
  done

  fail "failed to update ownership for $WORK_DIR"
}

confirm() {
  prompt=$1
  default=${2:-no}

  if [ "$ASSUME_YES" = "1" ]; then
    return 0
  fi

  if [ "$NON_INTERACTIVE" -eq 1 ]; then
    [ "$default" = "yes" ]
    return $?
  fi

  if [ "$default" = "yes" ]; then
    suffix='[Y/n]'
  else
    suffix='[y/N]'
  fi

  printf '%s %s ' "$prompt" "$suffix" >&2
  IFS= read -r answer
  case "$answer" in
    y|Y|yes|YES|Yes|д|Д|да|Да|ДА) return 0 ;;
    n|N|no|NO|No|н|Н|нет|Нет|НЕТ) return 1 ;;
    '')
      [ "$default" = "yes" ]
      return $?
      ;;
    *) return 1 ;;
  esac
}

read_secret() {
  var_name=$1
  prompt=$2
  current=$3

  if [ -n "$current" ]; then
    printf '%s' "$current"
    return 0
  fi

  if [ "$NON_INTERACTIVE" -eq 1 ]; then
    fail "$var_name is required in --non-interactive mode"
  fi

  printf '%s: ' "$prompt" >&2
  if [ -t 0 ]; then
    saved_stty=$(stty -g 2>/dev/null || printf '')
    stty -echo 2>/dev/null || true
    IFS= read -r value
    if [ -n "$saved_stty" ]; then
      stty "$saved_stty" 2>/dev/null || true
    else
      stty echo 2>/dev/null || true
    fi
    printf '\n' >&2
  else
    IFS= read -r value
  fi

  [ -n "$value" ] || fail "$var_name cannot be empty"
  printf '%s' "$value"
}

need_command() {
  command -v "$1" >/dev/null 2>&1 || fail "required command not found: $1"
}

has_c_compiler() {
  command -v cc >/dev/null 2>&1 ||
    command -v gcc >/dev/null 2>&1 ||
    command -v clang >/dev/null 2>&1
}

openssl_configured_by_env() {
  [ -n "${OPENSSL_DIR:-}" ] ||
    { [ -n "${OPENSSL_LIB_DIR:-}" ] && [ -n "${OPENSSL_INCLUDE_DIR:-}" ]; }
}

system_build_deps_missing_reason() {
  if ! has_c_compiler; then
    printf 'C compiler not found'
    return 0
  fi

  if openssl_configured_by_env; then
    return 1
  fi

  if ! command -v pkg-config >/dev/null 2>&1; then
    printf 'pkg-config not found'
    return 0
  fi

  if ! pkg-config --exists openssl >/dev/null 2>&1; then
    printf 'OpenSSL development package not found via pkg-config'
    return 0
  fi

  return 1
}

detect_package_manager() {
  if command -v apt-get >/dev/null 2>&1; then
    printf 'apt'
  elif command -v dnf >/dev/null 2>&1; then
    printf 'dnf'
  elif command -v yum >/dev/null 2>&1; then
    printf 'yum'
  elif command -v apk >/dev/null 2>&1; then
    printf 'apk'
  elif command -v pacman >/dev/null 2>&1; then
    printf 'pacman'
  elif command -v zypper >/dev/null 2>&1; then
    printf 'zypper'
  else
    return 1
  fi
}

install_system_build_deps() {
  reason=$1

  if [ "$INSTALL_SYSTEM_DEPS" != "1" ]; then
    fail "$reason; install pkg-config, OpenSSL development headers and a C compiler, or remove --no-install-system-deps"
  fi

  manager=$(detect_package_manager || true)
  [ -n "$manager" ] || fail "$reason; supported package manager not found. Install pkg-config, OpenSSL development headers and a C compiler manually"

  printf '%s. Installing system build dependencies with %s.\n' "$reason" "$manager"

  case "$manager" in
    apt)
      run_root env DEBIAN_FRONTEND=noninteractive apt-get update
      run_root env DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        pkg-config libssl-dev build-essential ca-certificates curl
      ;;
    dnf)
      run_root dnf install -y pkgconf-pkg-config openssl-devel gcc gcc-c++ make ca-certificates curl
      ;;
    yum)
      run_root yum install -y pkgconfig openssl-devel gcc gcc-c++ make ca-certificates curl
      ;;
    apk)
      run_root apk add --no-cache pkgconfig openssl-dev build-base ca-certificates curl
      ;;
    pacman)
      run_root pacman -Sy --noconfirm --needed pkgconf openssl base-devel ca-certificates curl
      ;;
    zypper)
      run_root zypper --non-interactive install pkg-config libopenssl-devel gcc gcc-c++ make ca-certificates curl
      ;;
    *) fail "unsupported package manager: $manager" ;;
  esac
}

ensure_system_build_deps() {
  [ "$SKIP_BUILD" -eq 0 ] || return 0

  reason=$(system_build_deps_missing_reason || true)
  if [ -z "$reason" ]; then
    return 0
  fi

  install_system_build_deps "$reason"

  if [ "$DRY_RUN" -eq 0 ]; then
    reason=$(system_build_deps_missing_reason || true)
    [ -z "$reason" ] || fail "system build dependencies are still missing after install: $reason"
  fi
}

find_in_path_or_cargo_home() {
  name=$1
  candidate="$CARGO_HOME/bin/$name"
  if [ -x "$candidate" ]; then
    printf '%s\n' "$candidate"
    return 0
  fi

  if command -v "$name" >/dev/null 2>&1; then
    command -v "$name"
    return 0
  fi

  return 1
}

tool_version() {
  tool_path=$1
  "$tool_path" --version 2>/dev/null | sed 's/^[^ ]* //; s/ .*//; s/[^0-9.].*$//'
}

version_at_least() {
  current=$1
  minimum=$2

  old_ifs=$IFS
  IFS=.
  set -- $current
  current_major=${1:-0}
  current_minor=${2:-0}
  current_patch=${3:-0}
  set -- $minimum
  minimum_major=${1:-0}
  minimum_minor=${2:-0}
  minimum_patch=${3:-0}
  IFS=$old_ifs

  [ "$current_major" -gt "$minimum_major" ] && return 0
  [ "$current_major" -lt "$minimum_major" ] && return 1
  [ "$current_minor" -gt "$minimum_minor" ] && return 0
  [ "$current_minor" -lt "$minimum_minor" ] && return 1
  [ "$current_patch" -ge "$minimum_patch" ]
}

install_rust_toolchain() {
  reason=$1
  if [ "$INSTALL_RUST" != "1" ]; then
    fail "$reason; install Rust >= $MIN_RUST_VERSION or remove --no-install-rust"
  fi

  printf '%s Installing/updating stable Rust with rustup.\n' "$reason"
  export CARGO_HOME RUSTUP_HOME

  rustup_path=$(find_in_path_or_cargo_home rustup || true)
  if [ -n "$rustup_path" ]; then
    run_cmd "$rustup_path" toolchain install stable --profile minimal
    PATH="$CARGO_HOME/bin:$PATH"
    export PATH CARGO_HOME RUSTUP_HOME
    return 0
  fi

  if command -v curl >/dev/null 2>&1; then
    run_cmd sh -c "curl --proto '=https' --tlsv1.2 -sSf $(quote_arg "$RUSTUP_INIT_URL") | sh -s -- -y --profile minimal"
  elif command -v wget >/dev/null 2>&1; then
    run_cmd sh -c "wget -qO- $(quote_arg "$RUSTUP_INIT_URL") | sh -s -- -y --profile minimal"
  else
    fail "curl or wget is required to install Rust with rustup"
  fi

  PATH="$CARGO_HOME/bin:$PATH"
  export PATH CARGO_HOME RUSTUP_HOME
}

ensure_rust_toolchain() {
  [ "$SKIP_BUILD" -eq 0 ] || return 0

  cargo_path=$(find_in_path_or_cargo_home cargo || true)
  rustc_path=$(find_in_path_or_cargo_home rustc || true)
  installed_rust=0

  if [ -z "$cargo_path" ] || [ -z "$rustc_path" ]; then
    install_rust_toolchain "cargo/rustc not found."
    installed_rust=1
  else
    cargo_version=$(tool_version "$cargo_path")
    rustc_version=$(tool_version "$rustc_path")
    if [ -z "$cargo_version" ]; then
      install_rust_toolchain "cannot determine cargo version at $cargo_path."
      installed_rust=1
    elif [ -z "$rustc_version" ]; then
      install_rust_toolchain "cannot determine rustc version at $rustc_path."
      installed_rust=1
    elif ! version_at_least "$cargo_version" "$MIN_RUST_VERSION"; then
      install_rust_toolchain "cargo $cargo_version is too old; need >= $MIN_RUST_VERSION."
      installed_rust=1
    elif ! version_at_least "$rustc_version" "$MIN_RUST_VERSION"; then
      install_rust_toolchain "rustc $rustc_version is too old; need >= $MIN_RUST_VERSION."
      installed_rust=1
    fi
  fi

  if [ "$DRY_RUN" -eq 1 ] && [ "$installed_rust" -eq 1 ]; then
    cargo_path="$CARGO_HOME/bin/cargo"
    rustc_path="$CARGO_HOME/bin/rustc"
  else
    cargo_path=$(find_in_path_or_cargo_home cargo || true)
    rustc_path=$(find_in_path_or_cargo_home rustc || true)
  fi

  if [ "$DRY_RUN" -eq 1 ]; then
    [ -n "$cargo_path" ] || cargo_path="$CARGO_HOME/bin/cargo"
    [ -n "$rustc_path" ] || rustc_path="$CARGO_HOME/bin/rustc"
  fi

  [ -n "$cargo_path" ] || fail "cargo was not found after Rust installation"
  [ -n "$rustc_path" ] || fail "rustc was not found after Rust installation"

  CARGO_BIN=$cargo_path
  printf 'Using cargo: %s\n' "$CARGO_BIN"
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --dry-run) DRY_RUN=1 ;;
    --non-interactive) NON_INTERACTIVE=1 ;;
    --no-build) SKIP_BUILD=1 ;;
    --no-install-rust) INSTALL_RUST=0 ;;
    --no-install-system-deps) INSTALL_SYSTEM_DEPS=0 ;;
    --no-start) SKIP_START=1 ;;
    --overwrite-config) OVERWRITE_CONFIG=1 ;;
    --overwrite-env) OVERWRITE_ENV=1 ;;
    -y|--yes) ASSUME_YES=1 ;;
    -h|--help)
      usage
      exit 0
      ;;
    *) fail "unknown option: $1" ;;
  esac
  shift
done

if [ "$DRY_RUN" -eq 1 ]; then
  printf 'DRY RUN: no system files or services will be changed.\n'
fi

need_command install
need_command sed
need_command id
need_command getent

if [ "$(id -u)" -ne 0 ] && [ "$DRY_RUN" -eq 0 ]; then
  need_command sudo
fi

if [ "$DRY_RUN" -eq 0 ]; then
  need_command mktemp
fi

ensure_system_build_deps
ensure_rust_toolchain

if [ "$SKIP_BUILD" -eq 0 ]; then
  run_cmd sh -c "cd $(quote_arg "$REPO_ROOT") && $(quote_arg "$CARGO_BIN") build --release -p agentd"
fi

BINARY="$REPO_ROOT/target/release/agentd"
if [ "$DRY_RUN" -eq 0 ] && [ ! -x "$BINARY" ]; then
  fail "agentd binary not found or not executable: $BINARY"
fi
if [ "$DRY_RUN" -eq 0 ] && [ ! -f "$CTL_SCRIPT" ]; then
  fail "teamdctl helper script not found: $CTL_SCRIPT"
fi

TMP_DIR=
if [ "$DRY_RUN" -eq 0 ]; then
  TMP_DIR=$(mktemp -d)
  chmod 0700 "$TMP_DIR"
  trap 'rm -rf "$TMP_DIR"' EXIT INT TERM
else
  TMP_DIR="$REPO_ROOT/target/deploy-teamd-dry-run"
fi

CONFIG_TMP="$TMP_DIR/config.toml"
ENV_TMP="$TMP_DIR/teamd.env"
DAEMON_UNIT_TMP="$TMP_DIR/$DAEMON_SERVICE"
TELEGRAM_UNIT_TMP="$TMP_DIR/$TELEGRAM_SERVICE"

WRITE_CONFIG=1
if [ -f "$CONFIG_FILE" ] && [ "$OVERWRITE_CONFIG" -ne 1 ]; then
  if confirm "$CONFIG_FILE already exists. Overwrite it?" no; then
    WRITE_CONFIG=1
  else
    WRITE_CONFIG=0
    printf 'Keeping existing config: %s\n' "$CONFIG_FILE"
  fi
fi

WRITE_ENV=1
if [ -f "$ENV_FILE" ] && [ "$OVERWRITE_ENV" -ne 1 ]; then
  if confirm "$ENV_FILE already exists. Overwrite it?" no; then
    WRITE_ENV=1
  else
    WRITE_ENV=0
    printf 'Keeping existing environment file: %s\n' "$ENV_FILE"
  fi
fi

if [ "$WRITE_ENV" -eq 1 ]; then
  TELEGRAM_TOKEN=$(read_secret TEAMD_TELEGRAM_BOT_TOKEN "Telegram bot token" "$TELEGRAM_TOKEN")
  PROVIDER_KEY=$(read_secret TEAMD_PROVIDER_API_KEY "Provider API key" "$PROVIDER_KEY")
fi

if [ "$DRY_RUN" -eq 0 ]; then
  cat > "$CONFIG_TMP" <<EOF
data_dir = "$DATA_DIR"

[daemon]
bind_host = "127.0.0.1"
bind_port = 5140
skills_dir = "skills"

[telegram]
enabled = true
poll_interval_ms = 1000
poll_request_timeout_seconds = 50
progress_update_min_interval_ms = 1250
global_send_min_interval_ms = 42
private_chat_send_min_interval_ms = 1250
group_chat_send_min_interval_ms = 3750
pairing_token_ttl_seconds = 900
max_upload_bytes = 16777216
max_download_bytes = 41943040
private_chat_auto_create_session = true
group_require_mention = true
default_autoapprove = true

[provider]
kind = "$PROVIDER_KIND"
api_base = "$PROVIDER_API_BASE"
default_model = "$PROVIDER_MODEL"
connect_timeout_seconds = 15
stream_idle_timeout_seconds = 1200
max_tool_rounds = 24

[web]
search_backend = "duckduckgo_html"
search_url = "https://duckduckgo.com/html/"

[permissions]
mode = "default"
EOF

  {
    printf 'TEAMD_CONFIG=%s\n' "$(quote_arg "$CONFIG_FILE")"
    printf 'TEAMD_DATA_DIR=%s\n' "$(quote_arg "$DATA_DIR")"
    printf 'TEAMD_TELEGRAM_BOT_TOKEN=%s\n' "$(quote_arg "$TELEGRAM_TOKEN")"
    printf 'TEAMD_PROVIDER_API_KEY=%s\n' "$(quote_arg "$PROVIDER_KEY")"
  } > "$ENV_TMP"
  chmod 0600 "$ENV_TMP"

  cat > "$DAEMON_UNIT_TMP" <<EOF
[Unit]
Description=teamD daemon
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=$SERVICE_USER
Group=$SERVICE_GROUP
EnvironmentFile=$ENV_FILE
WorkingDirectory=$WORK_DIR
ExecStart=$BIN_DIR/agentd daemon
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF

  cat > "$TELEGRAM_UNIT_TMP" <<EOF
[Unit]
Description=teamD Telegram worker
After=network-online.target $DAEMON_SERVICE
Wants=network-online.target
Requires=$DAEMON_SERVICE

[Service]
Type=simple
User=$SERVICE_USER
Group=$SERVICE_GROUP
EnvironmentFile=$ENV_FILE
WorkingDirectory=$WORK_DIR
ExecStart=$BIN_DIR/agentd telegram run
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF
else
  print_cmd sh -c "generate $CONFIG_FILE, $ENV_FILE and systemd unit files with redacted secrets"
fi

if ! getent group "$SERVICE_GROUP" >/dev/null 2>&1; then
  run_root groupadd --system "$SERVICE_GROUP"
fi

if ! id -u "$SERVICE_USER" >/dev/null 2>&1; then
  run_root useradd --system --gid "$SERVICE_GROUP" --create-home --home-dir "$WORK_DIR" --shell /usr/sbin/nologin "$SERVICE_USER"
fi

run_root mkdir -p "$BIN_DIR" "$CONFIG_PARENT" "$ENV_PARENT" "$PATH_LINK_PARENT" "$CTL_BIN_PARENT" "$WORK_DIR" "$DATA_DIR"
run_root install -m 0755 "$BINARY" "$BIN_DIR/agentd"
run_root ln -sf "$BIN_DIR/agentd" "$PATH_LINK"
run_root install -m 0755 -o root -g root "$CTL_SCRIPT" "$CTL_BIN"

if [ "$WRITE_CONFIG" -eq 1 ]; then
  run_root install -m 0644 -o root -g root "$CONFIG_TMP" "$CONFIG_FILE"
fi

if [ "$WRITE_ENV" -eq 1 ]; then
  run_root install -m 0640 -o root -g "$SERVICE_GROUP" "$ENV_TMP" "$ENV_FILE"
fi

chown_work_dir

run_root install -m 0644 -o root -g root "$DAEMON_UNIT_TMP" "/etc/systemd/system/$DAEMON_SERVICE"
run_root install -m 0644 -o root -g root "$TELEGRAM_UNIT_TMP" "/etc/systemd/system/$TELEGRAM_SERVICE"

run_root systemctl daemon-reload

if [ "$SKIP_START" -eq 0 ]; then
  run_root systemctl enable --now "$DAEMON_SERVICE"
  run_root systemctl enable --now "$TELEGRAM_SERVICE"
else
  printf 'Skipping service start because --no-start was set.\n'
fi

cat <<EOF

Deployment commands:
  Status:
    systemctl status $DAEMON_SERVICE
    systemctl status $TELEGRAM_SERVICE

  Logs:
    journalctl -u $TELEGRAM_SERVICE -f
    journalctl -u $DAEMON_SERVICE -n 100 --no-pager

  Restart:
    sudo systemctl restart $DAEMON_SERVICE
    sudo systemctl restart $TELEGRAM_SERVICE

  Pairing after Telegram /start:
    teamdctl telegram pair <key>

  List pairings:
    teamdctl telegram pairings

  Provider smoke:
    teamdctl provider smoke

  Session audit:
    teamdctl session list
    teamdctl session transcript <session_id>
    teamdctl session tools <session_id> --limit 50 --offset 0
    teamdctl session tools <session_id> --results --limit 50 --offset 0
    teamdctl session tool-result <tool_call_id>

  Service shortcuts:
    teamdctl daemon status
    teamdctl daemon restart
    teamdctl telegram logs
EOF
