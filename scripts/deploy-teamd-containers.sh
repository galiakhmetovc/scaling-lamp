#!/bin/sh
set -eu

PROGRAM=$(basename "$0")

DRY_RUN=0
NON_INTERACTIVE=0
SKIP_START=0
INSTALL_DOCKER=${TEAMD_CONTAINERS_INSTALL_DOCKER:-1}
ENABLE_SEARXNG=1
ENABLE_OBSIDIAN=0
ENABLE_OBSIDIAN_MCP=0
WRITE_OBSIDIAN_MCP_EXAMPLE=0
ENABLE_CADDY=1
RESTART_TEAMD_SERVICES=1

CONTAINERS_ROOT=${TEAMD_CONTAINERS_ROOT:-/opt/teamd/containers}
DATA_ROOT=${TEAMD_CONTAINERS_DATA_ROOT:-/var/lib/teamd/containers}
EDGE_NETWORK=${TEAMD_CONTAINERS_EDGE_NETWORK:-teamd-edge}
CONFIG_FILE=${TEAMD_CONFIG:-${TEAMD_DEPLOY_CONFIG_FILE:-/etc/teamd/config.toml}}
SERVICE_USER=${TEAMD_DEPLOY_USER:-teamd}
SERVICE_GROUP=${TEAMD_DEPLOY_GROUP:-$SERVICE_USER}
DAEMON_SERVICE=${TEAMD_DEPLOY_DAEMON_SERVICE:-teamd-daemon.service}
TELEGRAM_SERVICE=${TEAMD_DEPLOY_TELEGRAM_SERVICE:-teamd-telegram.service}

SEARXNG_PORT=${TEAMD_SEARXNG_PORT:-8888}
SEARXNG_IMAGE=${TEAMD_SEARXNG_IMAGE:-docker.io/searxng/searxng:latest}
SEARXNG_DIR=$CONTAINERS_ROOT/searxng
SEARXNG_CONFIG_DIR=$DATA_ROOT/searxng/config
SEARXNG_DATA_DIR=$DATA_ROOT/searxng/data
SEARXNG_COMPOSE=$SEARXNG_DIR/docker-compose.yml
SEARXNG_SETTINGS=$SEARXNG_CONFIG_DIR/settings.yml
SEARXNG_MCP_EXAMPLE=$SEARXNG_DIR/mcp-searxng.example.json

OBSIDIAN_PORT=${TEAMD_OBSIDIAN_PORT:-8080}
OBSIDIAN_IMAGE=${TEAMD_OBSIDIAN_IMAGE:-ghcr.io/sytone/obsidian-remote:latest}
OBSIDIAN_DIR=$CONTAINERS_ROOT/obsidian
OBSIDIAN_VAULTS_DIR=${TEAMD_OBSIDIAN_VAULTS_DIR:-/var/lib/teamd/vaults}
OBSIDIAN_VAULT_NAME=${TEAMD_OBSIDIAN_VAULT_NAME:-teamd}
OBSIDIAN_VAULT_DIR=${TEAMD_OBSIDIAN_VAULT_DIR:-$OBSIDIAN_VAULTS_DIR/$OBSIDIAN_VAULT_NAME}
OBSIDIAN_CONFIG_DIR=${TEAMD_OBSIDIAN_CONFIG_DIR:-$DATA_ROOT/obsidian/config}
OBSIDIAN_COMPOSE=$OBSIDIAN_DIR/docker-compose.yml
OBSIDIAN_PLUGIN_ID=obsidian-local-rest-api
OBSIDIAN_PLUGIN_VERSION=${TEAMD_OBSIDIAN_LOCAL_REST_API_VERSION:-latest}
OBSIDIAN_PLUGIN_BASE_URL=${TEAMD_OBSIDIAN_LOCAL_REST_API_BASE_URL:-https://github.com/coddingtonbear/obsidian-local-rest-api/releases}
OBSIDIAN_MCP_EXAMPLE=$OBSIDIAN_DIR/obsidian-mcp.example.toml
OBSIDIAN_MCP_ENV_EXAMPLE=$OBSIDIAN_DIR/obsidian-mcp.env.example
OBSIDIAN_MCP_ENV_FILE=${TEAMD_OBSIDIAN_MCP_ENV_FILE:-/etc/teamd/obsidian-mcp.env}
OBSIDIAN_MCP_IMAGE=${TEAMD_OBSIDIAN_MCP_IMAGE:-ghcr.io/oleksandrkucherenko/obsidian-mcp:latest}
OBSIDIAN_API_KEY=${TEAMD_OBSIDIAN_API_KEY:-}
OBSIDIAN_REST_API_URLS=${TEAMD_OBSIDIAN_REST_API_URLS:-}
if [ -z "$OBSIDIAN_REST_API_URLS" ]; then
  OBSIDIAN_REST_API_URLS='["https://127.0.0.1:27124","http://127.0.0.1:27123"]'
fi
OBSIDIAN_PUID=${TEAMD_OBSIDIAN_PUID:-}
OBSIDIAN_PGID=${TEAMD_OBSIDIAN_PGID:-}

CADDY_DOMAIN=${TEAMD_CADDY_DOMAIN:-}
if [ "${TEAMD_OBSIDIAN_SUBFOLDER+x}" ]; then
  OBSIDIAN_SUBFOLDER=$TEAMD_OBSIDIAN_SUBFOLDER
elif [ -n "$CADDY_DOMAIN" ]; then
  OBSIDIAN_SUBFOLDER=
else
  OBSIDIAN_SUBFOLDER=/obsidian/
fi
if [ -n "${TEAMD_CADDY_HTTP_PORT:-}" ]; then
  CADDY_HTTP_PORT=$TEAMD_CADDY_HTTP_PORT
elif [ -n "$CADDY_DOMAIN" ]; then
  CADDY_HTTP_PORT=80
else
  CADDY_HTTP_PORT=8088
fi
if [ -n "${TEAMD_CADDY_HTTPS_PORT:-}" ]; then
  CADDY_HTTPS_PORT=$TEAMD_CADDY_HTTPS_PORT
elif [ -n "$CADDY_DOMAIN" ]; then
  CADDY_HTTPS_PORT=443
else
  CADDY_HTTPS_PORT=
fi
CADDY_IMAGE=${TEAMD_CADDY_IMAGE:-docker.io/library/caddy:2}
CADDY_DIR=$CONTAINERS_ROOT/caddy
CADDY_DATA_DIR=$DATA_ROOT/caddy/data
CADDY_CONFIG_DIR=$DATA_ROOT/caddy/config
CADDY_COMPOSE=$CADDY_DIR/docker-compose.yml
CADDYFILE=$CADDY_DIR/Caddyfile

usage() {
  cat <<EOF
Usage: $PROGRAM [options]

Deploy teamD container add-ons without changing the main agentd deploy path.

By default this installs/uses Docker Engine, deploys a local SearXNG instance
bound to 127.0.0.1:$SEARXNG_PORT, and starts Caddy as an edge reverse proxy.
Obsidian is opt-in.

Options:
  --dry-run             Print actions without changing the system.
  --non-interactive     Do not prompt.
  --no-install-docker   Fail if Docker Engine or Docker Compose plugin is missing.
  --no-start            Write files but do not start containers.
  --no-searxng          Do not deploy SearXNG.
  --no-caddy            Do not deploy Caddy reverse proxy.
  --with-obsidian       Also deploy browser-accessible Obsidian container.
  --with-obsidian-mcp   Fully install Obsidian Local REST API + agentd MCP connector.
  --with-obsidian-mcp-example
                         Write an agentd stdio MCP connector example for Obsidian Local REST API.
  --no-restart-teamd    Do not restart teamd systemd services after writing MCP config.
  --searxng-port PORT   Local SearXNG port, default: $SEARXNG_PORT.
  --obsidian-port PORT  Local Obsidian port, default: $OBSIDIAN_PORT.
  -h, --help            Show this help.

Environment overrides:
  TEAMD_CONTAINERS_ROOT          Compose files root, default: $CONTAINERS_ROOT.
  TEAMD_CONTAINERS_DATA_ROOT     Persistent container data root, default: $DATA_ROOT.
  TEAMD_CONTAINERS_EDGE_NETWORK  Shared Docker network, default: $EDGE_NETWORK.
  TEAMD_CONTAINERS_INSTALL_DOCKER
                                 Auto-install Docker when missing, default: $INSTALL_DOCKER.
  TEAMD_SEARXNG_IMAGE            SearXNG image, default: $SEARXNG_IMAGE.
  TEAMD_SEARXNG_PORT             SearXNG localhost port, default: $SEARXNG_PORT.
  TEAMD_OBSIDIAN_IMAGE           Obsidian image, default: $OBSIDIAN_IMAGE.
  TEAMD_OBSIDIAN_PORT            Obsidian localhost port, default: $OBSIDIAN_PORT.
  TEAMD_OBSIDIAN_VAULTS_DIR      Vaults directory, default: $OBSIDIAN_VAULTS_DIR.
  TEAMD_OBSIDIAN_VAULT_NAME      Default managed vault name, default: $OBSIDIAN_VAULT_NAME.
  TEAMD_OBSIDIAN_VAULT_DIR       Managed vault directory, default: $OBSIDIAN_VAULT_DIR.
  TEAMD_OBSIDIAN_CONFIG_DIR      Obsidian config directory, default: $OBSIDIAN_CONFIG_DIR.
  TEAMD_OBSIDIAN_SUBFOLDER       Obsidian reverse-proxy subfolder.
                                 Default: "$OBSIDIAN_SUBFOLDER".
                                 Use empty value with a dedicated domain.
  TEAMD_OBSIDIAN_API_KEY         Optional fixed Local REST API key; generated when absent.
  TEAMD_OBSIDIAN_LOCAL_REST_API_VERSION
                                 Local REST API plugin version, default: $OBSIDIAN_PLUGIN_VERSION.
  TEAMD_OBSIDIAN_MCP_IMAGE       Obsidian MCP image, default: $OBSIDIAN_MCP_IMAGE.
  TEAMD_OBSIDIAN_MCP_ENV_FILE    Runtime env file for Obsidian MCP connector, default: $OBSIDIAN_MCP_ENV_FILE.
  TEAMD_OBSIDIAN_REST_API_URLS   JSON array of Local REST API URLs seen from MCP container,
                                 default: $OBSIDIAN_REST_API_URLS.
  TEAMD_CONFIG / TEAMD_DEPLOY_CONFIG_FILE
                                 agentd config.toml path, default: $CONFIG_FILE.
  TEAMD_DEPLOY_USER              teamd system user, default: $SERVICE_USER.
  TEAMD_DEPLOY_GROUP             teamd system group, default: $SERVICE_GROUP.
  TEAMD_CADDY_DOMAIN             Optional base domain; creates search.<domain> and obsidian.<domain>.
  TEAMD_CADDY_HTTP_PORT          Caddy HTTP host port, default: $CADDY_HTTP_PORT.
  TEAMD_CADDY_HTTPS_PORT         Caddy HTTPS host port, default: ${CADDY_HTTPS_PORT:-disabled}.
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

need_command() {
  command -v "$1" >/dev/null 2>&1 || fail "required command not found: $1"
}

valid_port() {
  case "$1" in
    ''|*[!0-9]*) return 1 ;;
    *)
      [ "$1" -ge 1 ] && [ "$1" -le 65535 ]
      ;;
  esac
}

docker_compose_available() {
  command -v docker >/dev/null 2>&1 && docker compose version >/dev/null 2>&1
}

detect_os_for_docker_apt() {
  [ -r /etc/os-release ] || return 1
  # shellcheck disable=SC1091
  . /etc/os-release
  case "${ID:-}" in
    ubuntu|debian)
      printf '%s %s\n' "$ID" "${UBUNTU_CODENAME:-${VERSION_CODENAME:-}}"
      ;;
    *)
      return 1
      ;;
  esac
}

install_docker_with_apt() {
  os_info=$(detect_os_for_docker_apt || true)
  [ -n "$os_info" ] || fail "Docker auto-install currently supports Ubuntu/Debian apt only"
  set -- $os_info
  docker_os=$1
  docker_codename=${2:-}
  [ -n "$docker_codename" ] || fail "cannot detect OS codename for Docker apt repository"

  printf 'Installing Docker Engine and Compose plugin from Docker apt repository.\n'
  run_root env DEBIAN_FRONTEND=noninteractive apt-get update
  run_root env DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
    ca-certificates curl
  run_root install -m 0755 -d /etc/apt/keyrings
  run_root sh -c "curl -fsSL https://download.docker.com/linux/$docker_os/gpg -o /etc/apt/keyrings/docker.asc"
  run_root chmod a+r /etc/apt/keyrings/docker.asc
  arch=$(dpkg --print-architecture)
  run_root sh -c "cat > /etc/apt/sources.list.d/docker.sources <<EOF
Types: deb
URIs: https://download.docker.com/linux/$docker_os
Suites: $docker_codename
Components: stable
Architectures: $arch
Signed-By: /etc/apt/keyrings/docker.asc
EOF"
  run_root env DEBIAN_FRONTEND=noninteractive apt-get update
  run_root env DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
    docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
  if command -v systemctl >/dev/null 2>&1; then
    run_root systemctl enable --now docker
  fi
}

ensure_docker() {
  if [ "$DRY_RUN" -eq 1 ]; then
    printf 'DRY RUN: ensure Docker Engine and Docker Compose plugin are available.\n'
    return 0
  fi

  if docker_compose_available; then
    return 0
  fi

  if [ "$INSTALL_DOCKER" != "1" ]; then
    fail "Docker Engine with Compose plugin is required; install Docker or omit --no-install-docker"
  fi

  install_docker_with_apt
  docker_compose_available || fail "Docker Compose plugin is still unavailable after Docker install"
}

validate_obsidian_subfolder() {
  [ "$ENABLE_OBSIDIAN" -eq 1 ] || return 0
  [ -n "$OBSIDIAN_SUBFOLDER" ] || return 0

  case "$OBSIDIAN_SUBFOLDER" in
    /*/) ;;
    *)
      fail "TEAMD_OBSIDIAN_SUBFOLDER must be empty or use leading and trailing slashes, e.g. /obsidian/"
      ;;
  esac
}

ensure_edge_network() {
  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd docker network create "$EDGE_NETWORK"
    return 0
  fi

  if docker network inspect "$EDGE_NETWORK" >/dev/null 2>&1; then
    return 0
  fi
  run_root docker network create "$EDGE_NETWORK"
}

generate_secret_key() {
  if command -v openssl >/dev/null 2>&1; then
    openssl rand -hex 32
    return 0
  fi
  od -An -N32 -tx1 /dev/urandom | tr -d ' \n'
}

write_searxng_files() {
  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd mkdir -p "$SEARXNG_DIR" "$SEARXNG_CONFIG_DIR" "$SEARXNG_DATA_DIR"
    print_cmd sh -c "write $SEARXNG_COMPOSE, $SEARXNG_SETTINGS and $SEARXNG_MCP_EXAMPLE"
    return 0
  fi

  run_root mkdir -p "$SEARXNG_DIR" "$SEARXNG_CONFIG_DIR" "$SEARXNG_DATA_DIR"
  secret_key=$(generate_secret_key)

  tmp_settings=$(mktemp)
  tmp_compose=$(mktemp)
  tmp_mcp=$(mktemp)
  trap 'rm -f "$tmp_settings" "$tmp_compose" "$tmp_mcp"' EXIT INT TERM

  cat > "$tmp_settings" <<EOF
use_default_settings: true

server:
  secret_key: "$secret_key"
  limiter: false
  image_proxy: false

search:
  formats:
    - html
    - json
EOF

  cat > "$tmp_compose" <<EOF
services:
  searxng:
    image: $SEARXNG_IMAGE
    container_name: teamd-searxng
    restart: unless-stopped
    ports:
      - "127.0.0.1:$SEARXNG_PORT:8080"
    networks:
      - $EDGE_NETWORK
    volumes:
      - "$SEARXNG_CONFIG_DIR:/etc/searxng:rw"
      - "$SEARXNG_DATA_DIR:/var/cache/searxng:rw"
    environment:
      - SEARXNG_BASE_URL=http://127.0.0.1:$SEARXNG_PORT/
      - FORCE_OWNERSHIP=true

networks:
  $EDGE_NETWORK:
    external: true
EOF

  cat > "$tmp_mcp" <<EOF
{
  "mcpServers": {
    "searxng": {
      "command": "npx",
      "args": ["-y", "mcp-searxng"],
      "env": {
        "SEARXNG_URL": "http://127.0.0.1:$SEARXNG_PORT"
      }
    }
  }
}
EOF

  run_root install -m 0644 -o root -g root "$tmp_settings" "$SEARXNG_SETTINGS"
  run_root install -m 0644 -o root -g root "$tmp_compose" "$SEARXNG_COMPOSE"
  run_root install -m 0644 -o root -g root "$tmp_mcp" "$SEARXNG_MCP_EXAMPLE"
}

resolve_obsidian_ids() {
  if [ -n "$OBSIDIAN_PUID" ] && [ -n "$OBSIDIAN_PGID" ]; then
    return 0
  fi

  if id -u teamd >/dev/null 2>&1; then
    OBSIDIAN_PUID=$(id -u teamd)
    OBSIDIAN_PGID=$(id -g teamd)
  else
    OBSIDIAN_PUID=$(id -u)
    OBSIDIAN_PGID=$(id -g)
  fi
}

write_obsidian_files() {
  resolve_obsidian_ids

  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd mkdir -p "$OBSIDIAN_DIR" "$OBSIDIAN_VAULTS_DIR" "$OBSIDIAN_VAULT_DIR" "$OBSIDIAN_CONFIG_DIR"
    print_cmd sh -c "write $OBSIDIAN_COMPOSE for teamd-obsidian"
    return 0
  fi

  run_root mkdir -p "$OBSIDIAN_DIR" "$OBSIDIAN_VAULTS_DIR" "$OBSIDIAN_VAULT_DIR" "$OBSIDIAN_CONFIG_DIR"
  run_root chown -R "$OBSIDIAN_PUID:$OBSIDIAN_PGID" "$OBSIDIAN_VAULT_DIR" "$OBSIDIAN_CONFIG_DIR"
  tmp_compose=$(mktemp)
  trap 'rm -f "$tmp_compose"' EXIT INT TERM

  cat > "$tmp_compose" <<EOF
services:
  obsidian:
    image: $OBSIDIAN_IMAGE
    container_name: teamd-obsidian
    restart: unless-stopped
    ports:
      - "127.0.0.1:$OBSIDIAN_PORT:8080"
    networks:
      - $EDGE_NETWORK
    volumes:
      - "$OBSIDIAN_VAULTS_DIR:/vaults:rw"
      - "$OBSIDIAN_CONFIG_DIR:/config:rw"
    environment:
      - PUID=$OBSIDIAN_PUID
      - PGID=$OBSIDIAN_PGID
      - TZ=Etc/UTC
      - CUSTOM_PORT=8080
      - SUBFOLDER=$OBSIDIAN_SUBFOLDER
      - DOCKER_MODS=linuxserver/mods:universal-git

networks:
  $EDGE_NETWORK:
    external: true
EOF

  run_root install -m 0644 -o root -g root "$tmp_compose" "$OBSIDIAN_COMPOSE"
}

obsidian_plugin_asset_url() {
  asset=$1
  if [ "$OBSIDIAN_PLUGIN_VERSION" = "latest" ]; then
    printf '%s/latest/download/%s' "$OBSIDIAN_PLUGIN_BASE_URL" "$asset"
  else
    printf '%s/download/%s/%s' "$OBSIDIAN_PLUGIN_BASE_URL" "$OBSIDIAN_PLUGIN_VERSION" "$asset"
  fi
}

ensure_obsidian_community_plugin_enabled() {
  community_file=$OBSIDIAN_VAULT_DIR/.obsidian/community-plugins.json

  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd sh -c "ensure $OBSIDIAN_PLUGIN_ID is present in $community_file"
    return 0
  fi

  tmp_plugins=$(mktemp)
  trap 'rm -f "$tmp_plugins"' EXIT INT TERM

  if [ ! -s "$community_file" ]; then
    cat > "$tmp_plugins" <<EOF
[
  "$OBSIDIAN_PLUGIN_ID"
]
EOF
    run_root install -m 0644 -o "$OBSIDIAN_PUID" -g "$OBSIDIAN_PGID" "$tmp_plugins" "$community_file"
    return 0
  fi

  if grep -F "\"$OBSIDIAN_PLUGIN_ID\"" "$community_file" >/dev/null 2>&1; then
    return 0
  fi

  compact=$(tr -d '[:space:]' < "$community_file")
  if [ "$compact" = "[]" ]; then
    cat > "$tmp_plugins" <<EOF
[
  "$OBSIDIAN_PLUGIN_ID"
]
EOF
  else
    sed "0,/]/s//,\\
  \"$OBSIDIAN_PLUGIN_ID\"\\
]/" "$community_file" > "$tmp_plugins"
  fi
  run_root install -m 0644 -o "$OBSIDIAN_PUID" -g "$OBSIDIAN_PGID" "$tmp_plugins" "$community_file"
}

write_obsidian_vault_config() {
  config_dir=$OBSIDIAN_VAULT_DIR/.obsidian
  plugin_dir=$config_dir/plugins/$OBSIDIAN_PLUGIN_ID
  data_file=$plugin_dir/data.json
  app_vault_file=$OBSIDIAN_CONFIG_DIR/.config/obsidian/obsidian.json

  if [ -z "$OBSIDIAN_API_KEY" ]; then
    OBSIDIAN_API_KEY=$(generate_secret_key)
  fi

  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd mkdir -p "$plugin_dir" "$OBSIDIAN_CONFIG_DIR/.config/obsidian"
    print_cmd sh -c "download Obsidian Local REST API plugin files into $plugin_dir"
    print_cmd sh -c "write $data_file with generated API_KEY and enable $OBSIDIAN_PLUGIN_ID"
    print_cmd sh -c "seed Obsidian vault registry at $app_vault_file"
    return 0
  fi

  need_command curl

  tmp_dir=$(mktemp -d)
  trap 'rm -rf "$tmp_dir"' EXIT INT TERM

  for asset in main.js manifest.json styles.css; do
    curl -fsSL "$(obsidian_plugin_asset_url "$asset")" -o "$tmp_dir/$asset"
  done

  run_root mkdir -p "$plugin_dir" "$OBSIDIAN_CONFIG_DIR/.config/obsidian"
  run_root install -m 0644 -o "$OBSIDIAN_PUID" -g "$OBSIDIAN_PGID" "$tmp_dir/main.js" "$plugin_dir/main.js"
  run_root install -m 0644 -o "$OBSIDIAN_PUID" -g "$OBSIDIAN_PGID" "$tmp_dir/manifest.json" "$plugin_dir/manifest.json"
  run_root install -m 0644 -o "$OBSIDIAN_PUID" -g "$OBSIDIAN_PGID" "$tmp_dir/styles.css" "$plugin_dir/styles.css"

  tmp_data=$(mktemp)
  cat > "$tmp_data" <<EOF
{
  "apiKey": "$OBSIDIAN_API_KEY",
  "port": 27124,
  "insecurePort": 27123,
  "enableInsecureServer": false,
  "enableSecureServer": true,
  "bindingHost": "127.0.0.1"
}
EOF
  run_root install -m 0600 -o "$OBSIDIAN_PUID" -g "$OBSIDIAN_PGID" "$tmp_data" "$data_file"
  rm -f "$tmp_data"

  ensure_obsidian_community_plugin_enabled

  if [ ! -e "$app_vault_file" ]; then
    vault_ts=$(date +%s)
    tmp_app_vault=$(mktemp)
    cat > "$tmp_app_vault" <<EOF
{
  "vaults": {
    "$OBSIDIAN_VAULT_NAME": {
      "path": "/vaults/$OBSIDIAN_VAULT_NAME",
      "ts": $vault_ts,
      "open": true
    }
  }
}
EOF
    run_root install -m 0644 -o "$OBSIDIAN_PUID" -g "$OBSIDIAN_PGID" "$tmp_app_vault" "$app_vault_file"
    rm -f "$tmp_app_vault"
  fi

  welcome_file=$OBSIDIAN_VAULT_DIR/teamd.md
  if [ ! -e "$welcome_file" ]; then
    tmp_welcome=$(mktemp)
    cat > "$tmp_welcome" <<EOF
# teamD vault

Этот vault создан deploy script'ом teamD.

- Obsidian UI редактирует заметки здесь.
- agentd обращается к vault через Obsidian Local REST API и MCP connector.
EOF
    run_root install -m 0644 -o "$OBSIDIAN_PUID" -g "$OBSIDIAN_PGID" "$tmp_welcome" "$welcome_file"
    rm -f "$tmp_welcome"
  fi

  run_root chown -R "$OBSIDIAN_PUID:$OBSIDIAN_PGID" "$OBSIDIAN_VAULT_DIR/.obsidian"
  run_root chown -R "$OBSIDIAN_PUID:$OBSIDIAN_PGID" "$OBSIDIAN_CONFIG_DIR/.config"
}

write_obsidian_mcp_example() {
  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd mkdir -p "$OBSIDIAN_DIR"
    print_cmd sh -c "write $OBSIDIAN_MCP_EXAMPLE and $OBSIDIAN_MCP_ENV_EXAMPLE"
    return 0
  fi

  run_root mkdir -p "$OBSIDIAN_DIR"
  tmp_config=$(mktemp)
  tmp_env=$(mktemp)
  trap 'rm -f "$tmp_config" "$tmp_env"' EXIT INT TERM

  cat > "$tmp_config" <<EOF
# Copy this block into /etc/teamd/config.toml under [daemon.mcp_connectors]
# after enabling Obsidian Local REST API plugin and copying
# $OBSIDIAN_MCP_ENV_EXAMPLE to $OBSIDIAN_MCP_ENV_FILE.
#
# Current agentd MCP transport is stdio, so this connector starts the MCP
# server through docker run -i --rm. This is an operator opt-in because the
# teamd service user must be allowed to run this exact Docker command.
#
# The MCP container shares teamd-obsidian network namespace. That lets it reach
# the Local REST API plugin on 127.0.0.1 inside the Obsidian container without
# publishing REST API ports on the host.

[daemon.mcp_connectors.obsidian]
transport = "stdio"
command = "docker"
args = [
  "run",
  "-i",
  "--rm",
  "--network", "container:teamd-obsidian",
  "--env-file", "$OBSIDIAN_MCP_ENV_FILE",
  "$OBSIDIAN_MCP_IMAGE",
]
enabled = false
EOF

  cat > "$tmp_env" <<EOF
# Copy this file to $OBSIDIAN_MCP_ENV_FILE, set mode 0640,
# and put the API key from Obsidian Local REST API plugin into API_KEY.
# This file is consumed by docker run --env-file, so do not shell-quote values.
API_KEY=replace-with-local-rest-api-key
API_URLS=$OBSIDIAN_REST_API_URLS
VERIFY_SSL=false
EOF

  run_root install -m 0644 -o root -g root "$tmp_config" "$OBSIDIAN_MCP_EXAMPLE"
  run_root install -m 0640 -o root -g root "$tmp_env" "$OBSIDIAN_MCP_ENV_EXAMPLE"
}

write_obsidian_mcp_runtime() {
  if [ -z "$OBSIDIAN_API_KEY" ]; then
    OBSIDIAN_API_KEY=$(generate_secret_key)
  fi

  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd sh -c "write $OBSIDIAN_MCP_ENV_FILE with generated API_KEY"
    return 0
  fi

  env_parent=$(dirname "$OBSIDIAN_MCP_ENV_FILE")
  run_root mkdir -p "$env_parent"
  tmp_env=$(mktemp)
  trap 'rm -f "$tmp_env"' EXIT INT TERM
  cat > "$tmp_env" <<EOF
API_KEY=$OBSIDIAN_API_KEY
API_URLS=$OBSIDIAN_REST_API_URLS
VERIFY_SSL=false
EOF
  run_root install -m 0640 -o root -g "$SERVICE_GROUP" "$tmp_env" "$OBSIDIAN_MCP_ENV_FILE"
}

append_obsidian_mcp_connector_config() {
  config_parent=$(dirname "$CONFIG_FILE")

  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd mkdir -p "$config_parent"
    print_cmd sh -c "append enabled Obsidian MCP connector to $CONFIG_FILE if missing"
    return 0
  fi

  run_root mkdir -p "$config_parent"
  if [ -e "$CONFIG_FILE" ] && grep -F "[daemon.mcp_connectors.obsidian]" "$CONFIG_FILE" >/dev/null 2>&1; then
    printf 'Keeping existing Obsidian MCP connector in %s.\n' "$CONFIG_FILE"
    return 0
  fi

  tmp_config=$(mktemp)
  trap 'rm -f "$tmp_config"' EXIT INT TERM
  cat > "$tmp_config" <<EOF

[daemon.mcp_connectors.obsidian]
transport = "stdio"
command = "docker"
args = [
  "run",
  "-i",
  "--rm",
  "--network", "container:teamd-obsidian",
  "--env-file", "$OBSIDIAN_MCP_ENV_FILE",
  "$OBSIDIAN_MCP_IMAGE",
]
enabled = true
EOF
  run_root sh -c "cat '$tmp_config' >> '$CONFIG_FILE'"
}

ensure_teamd_docker_access() {
  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd usermod -aG docker "$SERVICE_USER"
    return 0
  fi

  if ! id "$SERVICE_USER" >/dev/null 2>&1; then
    printf 'Warning: service user %s does not exist; cannot grant Docker access for Obsidian MCP.\n' "$SERVICE_USER" >&2
    return 0
  fi
  if ! getent group docker >/dev/null 2>&1; then
    printf 'Warning: docker group does not exist; cannot grant Docker access for Obsidian MCP.\n' >&2
    return 0
  fi

  run_root usermod -aG docker "$SERVICE_USER"
}

restart_teamd_service_if_present() {
  service=$1
  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd systemctl restart "$service"
    return 0
  fi

  if ! command -v systemctl >/dev/null 2>&1; then
    return 0
  fi
  if systemctl status "$service" >/dev/null 2>&1; then
    run_root systemctl restart "$service"
  fi
}

restart_teamd_services() {
  [ "$RESTART_TEAMD_SERVICES" -eq 1 ] || return 0
  [ "$SKIP_START" -eq 0 ] || return 0

  restart_teamd_service_if_present "$DAEMON_SERVICE"
  restart_teamd_service_if_present "$TELEGRAM_SERVICE"
}

write_caddy_files() {
  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd mkdir -p "$CADDY_DIR" "$CADDY_DATA_DIR" "$CADDY_CONFIG_DIR"
    print_cmd sh -c "write $CADDY_COMPOSE and $CADDYFILE for teamd-caddy"
    return 0
  fi

  run_root mkdir -p "$CADDY_DIR" "$CADDY_DATA_DIR" "$CADDY_CONFIG_DIR"
  tmp_compose=$(mktemp)
  tmp_caddyfile=$(mktemp)
  trap 'rm -f "$tmp_compose" "$tmp_caddyfile"' EXIT INT TERM

  ports_block="      - \"$CADDY_HTTP_PORT:80\""
  if [ -n "$CADDY_HTTPS_PORT" ]; then
    ports_block="$ports_block
      - \"$CADDY_HTTPS_PORT:443\""
  fi

  cat > "$tmp_compose" <<EOF
services:
  caddy:
    image: $CADDY_IMAGE
    container_name: teamd-caddy
    restart: unless-stopped
    ports:
$ports_block
    volumes:
      - "$CADDYFILE:/etc/caddy/Caddyfile:ro"
      - "$CADDY_DATA_DIR:/data:rw"
      - "$CADDY_CONFIG_DIR:/config:rw"
    networks:
      - $EDGE_NETWORK

networks:
  $EDGE_NETWORK:
    external: true
EOF

  if [ -n "$CADDY_DOMAIN" ]; then
    cat > "$tmp_caddyfile" <<EOF
search.$CADDY_DOMAIN {
  reverse_proxy teamd-searxng:8080
}

obsidian.$CADDY_DOMAIN {
  reverse_proxy teamd-obsidian:8080
}
EOF
  else
    if [ -n "$OBSIDIAN_SUBFOLDER" ]; then
      obsidian_route='handle /obsidian/*'
    else
      obsidian_route='handle_path /obsidian/*'
    fi
    cat > "$tmp_caddyfile" <<EOF
:80 {
  handle_path /searxng/* {
    reverse_proxy teamd-searxng:8080
  }

  $obsidian_route {
    reverse_proxy teamd-obsidian:8080
  }

  respond / "teamD container edge: /searxng/ and /obsidian/"
}
EOF
  fi

  run_root install -m 0644 -o root -g root "$tmp_compose" "$CADDY_COMPOSE"
  run_root install -m 0644 -o root -g root "$tmp_caddyfile" "$CADDYFILE"
}

compose_up() {
  compose_file=$1
  if [ "$SKIP_START" -eq 1 ]; then
    printf 'Skipping container start for %s because --no-start was set.\n' "$compose_file"
    return 0
  fi
  run_root docker compose -f "$compose_file" up -d
}

compose_up_caddy() {
  if [ "$SKIP_START" -eq 1 ]; then
    printf 'Skipping container start for %s because --no-start was set.\n' "$CADDY_COMPOSE"
    return 0
  fi

  # Caddyfile is mounted as a single bind-mounted file. Recreate avoids stale
  # inode mounts after atomic config replacement on the host.
  run_root docker compose -f "$CADDY_COMPOSE" up -d --force-recreate
}

reload_caddy_if_running() {
  [ "$ENABLE_CADDY" -eq 1 ] || return 0
  [ "$SKIP_START" -eq 0 ] || return 0

  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd docker exec teamd-caddy caddy reload --config /etc/caddy/Caddyfile
    return 0
  fi

  if ! run_root docker exec teamd-caddy caddy reload --config /etc/caddy/Caddyfile >/dev/null 2>&1; then
    run_root docker restart teamd-caddy >/dev/null
  fi
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --dry-run) DRY_RUN=1 ;;
    --non-interactive) NON_INTERACTIVE=1 ;;
    --no-install-docker) INSTALL_DOCKER=0 ;;
    --no-start) SKIP_START=1 ;;
    --no-searxng) ENABLE_SEARXNG=0 ;;
    --no-caddy) ENABLE_CADDY=0 ;;
    --with-obsidian) ENABLE_OBSIDIAN=1 ;;
    --with-obsidian-mcp)
      ENABLE_OBSIDIAN=1
      ENABLE_OBSIDIAN_MCP=1
      WRITE_OBSIDIAN_MCP_EXAMPLE=1
      ;;
    --with-obsidian-mcp-example)
      ENABLE_OBSIDIAN=1
      WRITE_OBSIDIAN_MCP_EXAMPLE=1
      ;;
    --no-restart-teamd) RESTART_TEAMD_SERVICES=0 ;;
    --searxng-port)
      shift
      [ "$#" -gt 0 ] || fail "--searxng-port requires a value"
      valid_port "$1" || fail "invalid --searxng-port: $1"
      SEARXNG_PORT=$1
      ;;
    --obsidian-port)
      shift
      [ "$#" -gt 0 ] || fail "--obsidian-port requires a value"
      valid_port "$1" || fail "invalid --obsidian-port: $1"
      OBSIDIAN_PORT=$1
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *) fail "unknown option: $1" ;;
  esac
  shift
done

[ "$NON_INTERACTIVE" -eq 0 ] || true

if [ "$DRY_RUN" -eq 1 ]; then
  printf 'DRY RUN: no Docker packages, compose files, data directories or containers will be changed.\n'
fi

need_command id
need_command sed
validate_obsidian_subfolder

if [ "$(id -u)" -ne 0 ] && [ "$DRY_RUN" -eq 0 ]; then
  need_command sudo
fi

if [ "$ENABLE_SEARXNG" -eq 0 ] && [ "$ENABLE_OBSIDIAN" -eq 0 ] && [ "$ENABLE_CADDY" -eq 0 ]; then
  fail "nothing to deploy: SearXNG disabled and Obsidian not enabled"
fi

ensure_docker
ensure_edge_network

if [ "$ENABLE_SEARXNG" -eq 1 ]; then
  write_searxng_files
  compose_up "$SEARXNG_COMPOSE"
fi

if [ "$ENABLE_OBSIDIAN" -eq 1 ]; then
  write_obsidian_files
  if [ "$ENABLE_OBSIDIAN_MCP" -eq 1 ]; then
    write_obsidian_vault_config
  fi
  compose_up "$OBSIDIAN_COMPOSE"
fi

if [ "$WRITE_OBSIDIAN_MCP_EXAMPLE" -eq 1 ]; then
  write_obsidian_mcp_example
fi

if [ "$ENABLE_OBSIDIAN_MCP" -eq 1 ]; then
  write_obsidian_mcp_runtime
  append_obsidian_mcp_connector_config
  ensure_teamd_docker_access
fi

if [ "$ENABLE_CADDY" -eq 1 ]; then
  write_caddy_files
  compose_up_caddy
  reload_caddy_if_running
fi

if [ "$ENABLE_OBSIDIAN_MCP" -eq 1 ]; then
  restart_teamd_services
fi

cat <<EOF

Container add-ons:
EOF

if [ "$ENABLE_SEARXNG" -eq 1 ]; then
  cat <<EOF
  SearXNG:
    Container: teamd-searxng
    URL: http://127.0.0.1:$SEARXNG_PORT
    Compose: $SEARXNG_COMPOSE
    Start command: docker compose -f $SEARXNG_COMPOSE up -d
    Settings: $SEARXNG_SETTINGS
    agentd web_search:
      TEAMD_WEB_SEARCH_BACKEND=searxng_json
      TEAMD_WEB_SEARCH_URL=http://127.0.0.1:$SEARXNG_PORT/search
    MCP example: $SEARXNG_MCP_EXAMPLE
    MCP env: SEARXNG_URL=http://127.0.0.1:$SEARXNG_PORT
    JSON smoke: curl 'http://127.0.0.1:$SEARXNG_PORT/search?q=test&format=json'
EOF
fi

if [ "$ENABLE_OBSIDIAN" -eq 1 ]; then
  cat <<EOF
  Obsidian:
    Container: teamd-obsidian
    URL: http://127.0.0.1:$OBSIDIAN_PORT$OBSIDIAN_SUBFOLDER
    Compose: $OBSIDIAN_COMPOSE
    Start command: docker compose -f $OBSIDIAN_COMPOSE up -d
    Vaults: $OBSIDIAN_VAULTS_DIR
    Managed vault: $OBSIDIAN_VAULT_DIR
EOF
  if [ "$ENABLE_OBSIDIAN_MCP" -eq 1 ]; then
    cat <<EOF
    Local REST API plugin:
      installed into managed vault and configured with generated API_KEY
EOF
    cat <<EOF
    Automated MCP:
      Plugin: $OBSIDIAN_PLUGIN_ID ($OBSIDIAN_PLUGIN_VERSION)
      Runtime env: $OBSIDIAN_MCP_ENV_FILE
      agentd config: $CONFIG_FILE
      Service user Docker access: $SERVICE_USER -> docker group
      Restarted services unless absent/no-start: $DAEMON_SERVICE, $TELEGRAM_SERVICE
EOF
  else
    cat <<EOF
    Local REST API plugin:
      install inside Obsidian, then copy API key to $OBSIDIAN_MCP_ENV_FILE as API_KEY
EOF
  fi
  if [ "$WRITE_OBSIDIAN_MCP_EXAMPLE" -eq 1 ]; then
    cat <<EOF
    MCP example: $OBSIDIAN_MCP_EXAMPLE
    MCP env example: $OBSIDIAN_MCP_ENV_EXAMPLE
    MCP env target: $OBSIDIAN_MCP_ENV_FILE
    MCP image: $OBSIDIAN_MCP_IMAGE
    MCP API_URLS: $OBSIDIAN_REST_API_URLS
    MCP network: container:teamd-obsidian
EOF
  fi
fi

if [ "$ENABLE_CADDY" -eq 1 ]; then
  cat <<EOF
  Caddy:
    Container: teamd-caddy
    URL: http://127.0.0.1:$CADDY_HTTP_PORT
    Compose: $CADDY_COMPOSE
    Start command: docker compose -f $CADDY_COMPOSE up -d
    Caddyfile: $CADDYFILE
    Routes without TEAMD_CADDY_DOMAIN: /searxng/ and /obsidian/
    Routes with TEAMD_CADDY_DOMAIN: search.<domain> and obsidian.<domain>
EOF
fi
