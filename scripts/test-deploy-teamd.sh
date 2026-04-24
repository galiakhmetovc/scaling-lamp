#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
DEPLOY_SCRIPT="$SCRIPT_DIR/deploy-teamd.sh"

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

dry_run_output=$(
  TEAMD_TELEGRAM_BOT_TOKEN='123456789:test-token' \
    TEAMD_PROVIDER_API_KEY='zai-test-key' \
    "$DEPLOY_SCRIPT" --dry-run --non-interactive --no-build --no-start 2>&1
)

assert_contains "$dry_run_output" "DRY RUN"
assert_contains "$dry_run_output" "teamd-daemon.service"
assert_contains "$dry_run_output" "teamd-telegram.service"
assert_contains "$dry_run_output" "telegram pair"

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

printf 'ok deploy-teamd smoke\n'
