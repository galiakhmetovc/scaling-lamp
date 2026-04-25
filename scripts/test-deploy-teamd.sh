#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
DEPLOY_SCRIPT="$SCRIPT_DIR/deploy-teamd.sh"
CONTAINERS_DEPLOY_SCRIPT="$SCRIPT_DIR/deploy-teamd-containers.sh"
TEAMDCTL_SCRIPT="$SCRIPT_DIR/teamdctl.sh"
DIAGNOSTICS_SCRIPT="$SCRIPT_DIR/collect-teamd-diagnostics.sh"

fail() {
  printf 'FAIL: %s\n' "$1" >&2
  exit 1
}

assert_contains() {
  haystack=$1
  needle=$2
  case "$haystack" in
    *"$needle"*) ;;
    *) fail "expected output to contain: $needle" ;;
  esac
}

help_output=$("$DEPLOY_SCRIPT" --help)
assert_contains "$help_output" "Usage:"
assert_contains "$help_output" "--dry-run"
assert_contains "$help_output" "--non-interactive"

containers_help_output=$("$CONTAINERS_DEPLOY_SCRIPT" --help)
assert_contains "$containers_help_output" "container add-ons"
assert_contains "$containers_help_output" "--with-obsidian"
assert_contains "$containers_help_output" "--no-searxng"
assert_contains "$containers_help_output" "--no-caddy"

diagnostics_help_output=$("$DIAGNOSTICS_SCRIPT" --help)
assert_contains "$diagnostics_help_output" "collects production diagnostics"
assert_contains "$diagnostics_help_output" "--session"

dry_run_output=$(
  TEAMD_TELEGRAM_BOT_TOKEN='123456789:test-token' \
    TEAMD_PROVIDER_API_KEY='zai-test-key' \
    "$DEPLOY_SCRIPT" --dry-run --non-interactive --no-build --no-start 2>&1
)

assert_contains "$dry_run_output" "DRY RUN"
assert_contains "$dry_run_output" "teamd-daemon.service"
assert_contains "$dry_run_output" "teamd-telegram.service"
assert_contains "$dry_run_output" "/usr/local/bin/agentd"
assert_contains "$dry_run_output" "/usr/local/bin/teamdctl"
assert_contains "$dry_run_output" "telegram pair"
assert_contains "$dry_run_output" "session list"

containers_dry_run_output=$(
  "$CONTAINERS_DEPLOY_SCRIPT" --dry-run --non-interactive --no-start --with-obsidian 2>&1
)

assert_contains "$containers_dry_run_output" "DRY RUN"
assert_contains "$containers_dry_run_output" "teamd-searxng"
assert_contains "$containers_dry_run_output" "127.0.0.1:8888"
assert_contains "$containers_dry_run_output" "mcp-searxng"
assert_contains "$containers_dry_run_output" "teamd-obsidian"
assert_contains "$containers_dry_run_output" "teamd-caddy"
assert_contains "$containers_dry_run_output" "docker compose"
assert_contains "$containers_dry_run_output" "TEAMD_WEB_SEARCH_BACKEND=searxng_json"
assert_contains "$containers_dry_run_output" "TEAMD_WEB_SEARCH_URL=http://127.0.0.1:8888/search"

existing_env_dir="$SCRIPT_DIR/../target/deploy-script-test"
existing_env="$existing_env_dir/existing.env"
mkdir -p "$existing_env_dir"
: > "$existing_env"

existing_env_output=$(
  TEAMD_DEPLOY_ENV_FILE="$existing_env" \
    "$DEPLOY_SCRIPT" --dry-run --non-interactive --no-build --no-start 2>&1
)

assert_contains "$existing_env_output" "Keeping existing environment file"
assert_contains "$existing_env_output" "teamd-telegram.service"

fake_bin="$existing_env_dir/fake-bin"
fake_cargo_home="$existing_env_dir/fake-cargo-home"
fake_rustup_home="$existing_env_dir/fake-rustup-home"
mkdir -p "$fake_bin" "$fake_cargo_home" "$fake_rustup_home"
cat > "$fake_bin/cargo" <<'EOF'
#!/bin/sh
echo 'cargo 1.75.0 (fake)'
EOF
cat > "$fake_bin/rustc" <<'EOF'
#!/bin/sh
echo 'rustc 1.75.0 (fake)'
EOF
chmod +x "$fake_bin/cargo" "$fake_bin/rustc"

old_rust_output=$(
  PATH="$fake_bin:/usr/bin:/bin" \
    CARGO_HOME="$fake_cargo_home" \
    RUSTUP_HOME="$fake_rustup_home" \
    TEAMD_TELEGRAM_BOT_TOKEN='123456789:test-token' \
    TEAMD_PROVIDER_API_KEY='zai-test-key' \
    "$DEPLOY_SCRIPT" --dry-run --non-interactive --no-start 2>&1
)

assert_contains "$old_rust_output" "cargo 1.75.0 is too old"
assert_contains "$old_rust_output" "sh.rustup.rs"
assert_contains "$old_rust_output" "build --release -p agentd"

set +e
no_install_output=$(
  PATH="$fake_bin:/usr/bin:/bin" \
    CARGO_HOME="$fake_cargo_home" \
    RUSTUP_HOME="$fake_rustup_home" \
    "$DEPLOY_SCRIPT" --dry-run --non-interactive --no-install-rust --no-start 2>&1
)
no_install_status=$?
set -e

[ "$no_install_status" -ne 0 ] || fail "expected --no-install-rust to fail with old cargo"
assert_contains "$no_install_output" "install Rust >= 1.85.0"

deps_bin="$existing_env_dir/deps-bin"
mkdir -p "$deps_bin"
cat > "$deps_bin/pkg-config" <<'EOF'
#!/bin/sh
exit 1
EOF
chmod +x "$deps_bin/pkg-config"

deps_output=$(
  PATH="$deps_bin:/usr/bin:/bin" \
    TEAMD_TELEGRAM_BOT_TOKEN='123456789:test-token' \
    TEAMD_PROVIDER_API_KEY='zai-test-key' \
    "$DEPLOY_SCRIPT" --dry-run --non-interactive --no-start 2>&1
)

assert_contains "$deps_output" "OpenSSL development package not found via pkg-config"
assert_contains "$deps_output" "Installing system build dependencies"
assert_contains "$deps_output" "pkg-config"

fake_ctl_dir="$existing_env_dir/fake-teamdctl"
fake_agentd="$fake_ctl_dir/agentd"
fake_env="$fake_ctl_dir/teamd.env"
mkdir -p "$fake_ctl_dir"
cat > "$fake_agentd" <<'EOF'
#!/bin/sh
printf 'fake-agentd'
for arg in "$@"; do
  printf ' %s' "$arg"
done
printf '\n'
EOF
chmod +x "$fake_agentd"
cat > "$fake_env" <<'EOF'
TEAMD_DATA_DIR='/tmp/fake-teamd-state'
EOF

teamdctl_pair_output=$(
  TEAMD_RUN_USER="$(id -un)" \
    TEAMD_ENV_FILE="$fake_env" \
    TEAMD_AGENTD_BIN="$fake_agentd" \
    "$TEAMDCTL_SCRIPT" telegram pair tg-test
)

assert_contains "$teamdctl_pair_output" "fake-agentd telegram pair tg-test"

printf 'ok deploy-teamd smoke\n'
