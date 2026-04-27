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
ENABLE_JAEGER=0
ENABLE_CADDY=1
RESTART_TEAMD_SERVICES=1

CONTAINERS_ROOT=${TEAMD_CONTAINERS_ROOT:-/opt/teamd/containers}
DATA_ROOT=${TEAMD_CONTAINERS_DATA_ROOT:-/var/lib/teamd/containers}
EDGE_NETWORK=${TEAMD_CONTAINERS_EDGE_NETWORK:-teamd-edge}
CONFIG_FILE=${TEAMD_CONFIG:-${TEAMD_DEPLOY_CONFIG_FILE:-/etc/teamd/config.toml}}
ENV_FILE=${TEAMD_DEPLOY_ENV_FILE:-/etc/teamd/teamd.env}
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
OBSIDIAN_CONTAINER_PORT=${TEAMD_OBSIDIAN_CONTAINER_PORT:-3000}
OBSIDIAN_IMAGE=${TEAMD_OBSIDIAN_IMAGE:-lscr.io/linuxserver/obsidian:latest}
OBSIDIAN_DIR=$CONTAINERS_ROOT/obsidian
OBSIDIAN_VAULTS_DIR=${TEAMD_OBSIDIAN_VAULTS_DIR:-/var/lib/teamd/vaults}
OBSIDIAN_VAULT_NAME=${TEAMD_OBSIDIAN_VAULT_NAME:-teamd}
OBSIDIAN_VAULT_DIR=${TEAMD_OBSIDIAN_VAULT_DIR:-$OBSIDIAN_VAULTS_DIR/$OBSIDIAN_VAULT_NAME}
OBSIDIAN_LEGACY_VAULT_LINK=${TEAMD_OBSIDIAN_LEGACY_VAULT_LINK:-/var/lib/teamd/vault}
OBSIDIAN_CONFIG_DIR=${TEAMD_OBSIDIAN_CONFIG_DIR:-$DATA_ROOT/obsidian/config}
OBSIDIAN_COMPOSE=$OBSIDIAN_DIR/docker-compose.yml
OBSIDIAN_MCP_EXAMPLE=$OBSIDIAN_DIR/obsidian-mcp.example.toml
OBSIDIAN_MCP_PACKAGE=${TEAMD_OBSIDIAN_MCP_PACKAGE:-@bitbonsai/mcpvault@latest}
OBSIDIAN_MCP_NODE_IMAGE=${TEAMD_OBSIDIAN_MCP_NODE_IMAGE:-docker.io/library/node:22-alpine}
OBSIDIAN_PUID=${TEAMD_OBSIDIAN_PUID:-}
OBSIDIAN_PGID=${TEAMD_OBSIDIAN_PGID:-}

JAEGER_UI_PORT=${TEAMD_JAEGER_UI_PORT:-16686}
JAEGER_OTLP_GRPC_PORT=${TEAMD_JAEGER_OTLP_GRPC_PORT:-4317}
JAEGER_OTLP_HTTP_PORT=${TEAMD_JAEGER_OTLP_HTTP_PORT:-4318}
JAEGER_IMAGE=${TEAMD_JAEGER_IMAGE:-docker.io/jaegertracing/all-in-one:1.76.0}
JAEGER_UID=${TEAMD_JAEGER_UID:-10001}
JAEGER_GID=${TEAMD_JAEGER_GID:-10001}
JAEGER_DIR=$CONTAINERS_ROOT/jaeger
JAEGER_DATA_DIR=$DATA_ROOT/jaeger/badger
JAEGER_COMPOSE=$JAEGER_DIR/docker-compose.yml
if [ "${TEAMD_JAEGER_BASE_PATH+x}" ]; then
  JAEGER_BASE_PATH=$TEAMD_JAEGER_BASE_PATH
elif [ -n "${TEAMD_CADDY_DOMAIN:-}" ]; then
  JAEGER_BASE_PATH=/
else
  JAEGER_BASE_PATH=/jaeger
fi
OTLP_EXPORT_TIMEOUT_MS=${TEAMD_OTLP_TIMEOUT_MS:-2000}

CADDY_DOMAIN=${TEAMD_CADDY_DOMAIN:-}
CADDY_HOST=${TEAMD_CADDY_HOST:-}
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
  --with-obsidian-mcp   Deploy Obsidian and an agentd MCP connector for the vault.
  --with-obsidian-mcp-example
                         Write an agentd stdio MCP connector example for the vault.
  --with-jaeger         Also deploy Jaeger UI and enable agentd OTLP auto-export.
  --no-restart-teamd    Do not restart teamd systemd services after writing MCP config.
  --searxng-port PORT   Local SearXNG port, default: $SEARXNG_PORT.
  --obsidian-port PORT  Local Obsidian port, default: $OBSIDIAN_PORT.
  --jaeger-ui-port PORT Local Jaeger UI port, default: $JAEGER_UI_PORT.
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
  TEAMD_OBSIDIAN_CONTAINER_PORT  Obsidian web port inside container,
                                 default: $OBSIDIAN_CONTAINER_PORT.
  TEAMD_OBSIDIAN_VAULTS_DIR      Vaults directory, default: $OBSIDIAN_VAULTS_DIR.
  TEAMD_OBSIDIAN_VAULT_NAME      Default managed vault name, default: $OBSIDIAN_VAULT_NAME.
  TEAMD_OBSIDIAN_VAULT_DIR       Managed vault directory, default: $OBSIDIAN_VAULT_DIR.
  TEAMD_OBSIDIAN_LEGACY_VAULT_LINK
                                 Compatibility symlink for agents using ~/vault,
                                 default: $OBSIDIAN_LEGACY_VAULT_LINK.
                                 Set empty to disable.
  TEAMD_OBSIDIAN_CONFIG_DIR      Obsidian config directory, default: $OBSIDIAN_CONFIG_DIR.
  TEAMD_OBSIDIAN_SUBFOLDER       Obsidian reverse-proxy subfolder.
                                 Default: "$OBSIDIAN_SUBFOLDER".
                                 Use empty value with a dedicated domain.
  TEAMD_OBSIDIAN_MCP_PACKAGE     npm package for Obsidian vault MCP,
                                 default: $OBSIDIAN_MCP_PACKAGE.
  TEAMD_OBSIDIAN_MCP_NODE_IMAGE  Docker image used to run the MCP package,
                                 default: $OBSIDIAN_MCP_NODE_IMAGE.
  TEAMD_JAEGER_IMAGE             Jaeger all-in-one image, default: $JAEGER_IMAGE.
  TEAMD_JAEGER_UID               Jaeger container UID for Badger storage, default: $JAEGER_UID.
  TEAMD_JAEGER_GID               Jaeger container GID for Badger storage, default: $JAEGER_GID.
  TEAMD_JAEGER_UI_PORT           Jaeger UI localhost port, default: $JAEGER_UI_PORT.
  TEAMD_JAEGER_OTLP_GRPC_PORT    Jaeger OTLP/gRPC localhost port, default: $JAEGER_OTLP_GRPC_PORT.
  TEAMD_JAEGER_OTLP_HTTP_PORT    Jaeger OTLP/HTTP localhost port, default: $JAEGER_OTLP_HTTP_PORT.
  TEAMD_JAEGER_BASE_PATH         Jaeger UI base path. Default: "$JAEGER_BASE_PATH".
  TEAMD_OTLP_TIMEOUT_MS          agentd OTLP export timeout, default: $OTLP_EXPORT_TIMEOUT_MS.
  TEAMD_CONFIG / TEAMD_DEPLOY_CONFIG_FILE
                                 agentd config.toml path, default: $CONFIG_FILE.
  TEAMD_DEPLOY_ENV_FILE          agentd env file path, default: $ENV_FILE.
  TEAMD_DEPLOY_USER              teamd system user, default: $SERVICE_USER.
  TEAMD_DEPLOY_GROUP             teamd system group, default: $SERVICE_GROUP.
  TEAMD_CADDY_DOMAIN             Optional base domain; creates search.<domain> and obsidian.<domain>.
  TEAMD_CADDY_HOST               Hostname or IP for internal TLS without a dedicated domain.
                                 If unset, deploy script tries to detect the primary IPv4 address.
  TEAMD_CADDY_HTTP_PORT          Caddy HTTP host port, default: $CADDY_HTTP_PORT.
  TEAMD_CADDY_HTTPS_PORT         Caddy HTTPS host port. Default: 443 with TEAMD_CADDY_DOMAIN,
                                 8443 when Obsidian is enabled without a domain,
                                 otherwise disabled.
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

ensure_obsidian_https_port() {
  [ "$ENABLE_OBSIDIAN" -eq 1 ] || return 0
  [ "$ENABLE_CADDY" -eq 1 ] || return 0
  [ -n "$CADDY_DOMAIN" ] && return 0
  [ -n "$CADDY_HTTPS_PORT" ] && return 0

  CADDY_HTTPS_PORT=8443
}

detect_primary_ipv4() {
  if command -v ip >/dev/null 2>&1; then
    ip route get 1.1.1.1 2>/dev/null | awk '/src/ { for (i = 1; i <= NF; i++) if ($i == "src") { print $(i + 1); exit } }'
    return 0
  fi

  if command -v hostname >/dev/null 2>&1; then
    hostname -I 2>/dev/null | awk '{ print $1 }'
    return 0
  fi

  return 1
}

ensure_caddy_host() {
  [ "$ENABLE_OBSIDIAN" -eq 1 ] || return 0
  [ "$ENABLE_CADDY" -eq 1 ] || return 0
  [ -n "$CADDY_DOMAIN" ] && return 0
  [ -n "$CADDY_HTTPS_PORT" ] || return 0
  [ -n "$CADDY_HOST" ] && return 0

  if [ "$DRY_RUN" -eq 1 ]; then
    CADDY_HOST=127.0.0.1
    return 0
  fi

  CADDY_HOST=$(detect_primary_ipv4 || true)
  [ -n "$CADDY_HOST" ] || fail "cannot detect Caddy host/IP for Obsidian HTTPS; set TEAMD_CADDY_HOST explicitly"
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

configure_agentd_web_search_env() {
  env_parent=$(dirname "$ENV_FILE")
  search_url="http://127.0.0.1:$SEARXNG_PORT/search"

  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd mkdir -p "$env_parent"
    print_cmd sh -c "upsert SearXNG web_search defaults in $ENV_FILE"
    return 0
  fi

  run_root mkdir -p "$env_parent"

  tmp_env=$(mktemp)
  tmp_new=$(mktemp)
  if [ -e "$ENV_FILE" ]; then
    awk '
      !/^(export[[:space:]]+)?TEAMD_WEB_SEARCH_BACKEND=/ &&
      !/^(export[[:space:]]+)?TEAMD_WEB_SEARCH_URL=/
    ' "$ENV_FILE" > "$tmp_env"
  else
    : > "$tmp_env"
  fi

  {
    cat "$tmp_env"
    [ ! -s "$tmp_env" ] || printf '\n'
    printf 'TEAMD_WEB_SEARCH_BACKEND=%s\n' "$(quote_arg "searxng_json")"
    printf 'TEAMD_WEB_SEARCH_URL=%s\n' "$(quote_arg "$search_url")"
  } > "$tmp_new"

  env_group=root
  if getent group "$SERVICE_GROUP" >/dev/null 2>&1; then
    env_group=$SERVICE_GROUP
  fi
  run_root install -m 0640 -o root -g "$env_group" "$tmp_new" "$ENV_FILE"
  rm -f "$tmp_env" "$tmp_new"
}

write_jaeger_files() {
  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd mkdir -p "$JAEGER_DIR" "$JAEGER_DATA_DIR"
    print_cmd chown -R "$JAEGER_UID:$JAEGER_GID" "$JAEGER_DATA_DIR"
    print_cmd sh -c "write $JAEGER_COMPOSE for teamd-jaeger"
    return 0
  fi

  run_root mkdir -p "$JAEGER_DIR" "$JAEGER_DATA_DIR"
  run_root chown -R "$JAEGER_UID:$JAEGER_GID" "$JAEGER_DATA_DIR"
  tmp_compose=$(mktemp)
  trap 'rm -f "$tmp_compose"' EXIT INT TERM

  cat > "$tmp_compose" <<EOF
services:
  jaeger:
    image: $JAEGER_IMAGE
    container_name: teamd-jaeger
    restart: unless-stopped
    ports:
      - "127.0.0.1:$JAEGER_UI_PORT:16686"
      - "127.0.0.1:$JAEGER_OTLP_GRPC_PORT:4317"
      - "127.0.0.1:$JAEGER_OTLP_HTTP_PORT:4318"
    networks:
      - $EDGE_NETWORK
    volumes:
      - "$JAEGER_DATA_DIR:/badger:rw"
    environment:
      - COLLECTOR_OTLP_ENABLED=true
      - SPAN_STORAGE_TYPE=badger
      - BADGER_EPHEMERAL=false
      - BADGER_DIRECTORY_VALUE=/badger/data
      - BADGER_DIRECTORY_KEY=/badger/key
      - QUERY_BASE_PATH=$JAEGER_BASE_PATH

networks:
  $EDGE_NETWORK:
    external: true
EOF

  run_root install -m 0644 -o root -g root "$tmp_compose" "$JAEGER_COMPOSE"
}

configure_agentd_otlp_env() {
  env_parent=$(dirname "$ENV_FILE")
  otlp_endpoint="http://127.0.0.1:$JAEGER_OTLP_HTTP_PORT/v1/traces"

  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd mkdir -p "$env_parent"
    print_cmd sh -c "upsert OTLP trace export defaults in $ENV_FILE"
    return 0
  fi

  run_root mkdir -p "$env_parent"

  tmp_env=$(mktemp)
  tmp_new=$(mktemp)
  if [ -e "$ENV_FILE" ]; then
    awk '
      !/^(export[[:space:]]+)?TEAMD_OTLP_EXPORT_ENABLED=/ &&
      !/^(export[[:space:]]+)?TEAMD_OTLP_ENDPOINT=/ &&
      !/^(export[[:space:]]+)?TEAMD_OTLP_TIMEOUT_MS=/
    ' "$ENV_FILE" > "$tmp_env"
  else
    : > "$tmp_env"
  fi

  {
    cat "$tmp_env"
    [ ! -s "$tmp_env" ] || printf '\n'
    printf 'TEAMD_OTLP_EXPORT_ENABLED=%s\n' "$(quote_arg "true")"
    printf 'TEAMD_OTLP_ENDPOINT=%s\n' "$(quote_arg "$otlp_endpoint")"
    printf 'TEAMD_OTLP_TIMEOUT_MS=%s\n' "$(quote_arg "$OTLP_EXPORT_TIMEOUT_MS")"
  } > "$tmp_new"

  env_group=root
  if getent group "$SERVICE_GROUP" >/dev/null 2>&1; then
    env_group=$SERVICE_GROUP
  fi
  run_root install -m 0640 -o root -g "$env_group" "$tmp_new" "$ENV_FILE"
  rm -f "$tmp_env" "$tmp_new"
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
    shm_size: "1gb"
    ports:
      - "127.0.0.1:$OBSIDIAN_PORT:$OBSIDIAN_CONTAINER_PORT"
    networks:
      - $EDGE_NETWORK
    volumes:
      - "$OBSIDIAN_VAULTS_DIR:/vaults:rw"
      - "$OBSIDIAN_CONFIG_DIR:/config:rw"
    environment:
      - PUID=$OBSIDIAN_PUID
      - PGID=$OBSIDIAN_PGID
      - TZ=Etc/UTC
      - SUBFOLDER=$OBSIDIAN_SUBFOLDER
      - DOCKER_MODS=linuxserver/mods:universal-git

networks:
  $EDGE_NETWORK:
    external: true
EOF

  run_root install -m 0644 -o root -g root "$tmp_compose" "$OBSIDIAN_COMPOSE"
}

write_obsidian_vault_config() {
  config_dir=$OBSIDIAN_VAULT_DIR/.obsidian
  app_vault_file=$OBSIDIAN_CONFIG_DIR/.config/obsidian/obsidian.json

  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd mkdir -p "$config_dir" "$OBSIDIAN_CONFIG_DIR/.config/obsidian"
    print_cmd sh -c "seed Obsidian vault registry at $app_vault_file"
    print_cmd sh -c "write managed vault welcome note when missing"
    return 0
  fi

  run_root mkdir -p "$config_dir" "$OBSIDIAN_CONFIG_DIR/.config/obsidian"

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
- agentd обращается к vault через Obsidian MCP connector.
- Прямые filesystem writes нужны только для аварийной миграции или восстановления.
EOF
    run_root install -m 0644 -o "$OBSIDIAN_PUID" -g "$OBSIDIAN_PGID" "$tmp_welcome" "$welcome_file"
    rm -f "$tmp_welcome"
  fi

  run_root chown -R "$OBSIDIAN_PUID:$OBSIDIAN_PGID" "$OBSIDIAN_VAULT_DIR/.obsidian"
  run_root chown -R "$OBSIDIAN_PUID:$OBSIDIAN_PGID" "$OBSIDIAN_CONFIG_DIR/.config"
}

ensure_obsidian_legacy_vault_link() {
  [ -n "$OBSIDIAN_LEGACY_VAULT_LINK" ] || return 0
  [ "$OBSIDIAN_LEGACY_VAULT_LINK" != "$OBSIDIAN_VAULT_DIR" ] || return 0

  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd sh -c "create compatibility symlink $OBSIDIAN_LEGACY_VAULT_LINK -> $OBSIDIAN_VAULT_DIR when path is absent"
    return 0
  fi

  target_real=$(readlink -f "$OBSIDIAN_VAULT_DIR")

  if [ -L "$OBSIDIAN_LEGACY_VAULT_LINK" ]; then
    link_real=$(readlink -f "$OBSIDIAN_LEGACY_VAULT_LINK" || true)
    if [ "$link_real" = "$target_real" ]; then
      return 0
    fi
    printf 'Warning: %s is a symlink to %s, not %s; leaving it unchanged.\n' \
      "$OBSIDIAN_LEGACY_VAULT_LINK" "${link_real:-unknown}" "$OBSIDIAN_VAULT_DIR" >&2
    return 0
  fi

  if [ -e "$OBSIDIAN_LEGACY_VAULT_LINK" ]; then
    printf 'Warning: %s already exists and is not a symlink; not migrating automatically. Move it into %s manually, then replace it with a symlink.\n' \
      "$OBSIDIAN_LEGACY_VAULT_LINK" "$OBSIDIAN_VAULT_DIR" >&2
    return 0
  fi

  legacy_parent=$(dirname "$OBSIDIAN_LEGACY_VAULT_LINK")
  run_root mkdir -p "$legacy_parent"
  run_root ln -s "$OBSIDIAN_VAULT_DIR" "$OBSIDIAN_LEGACY_VAULT_LINK"
  run_root chown -h "$OBSIDIAN_PUID:$OBSIDIAN_PGID" "$OBSIDIAN_LEGACY_VAULT_LINK"
}

write_obsidian_mcp_example() {
  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd mkdir -p "$OBSIDIAN_DIR"
    print_cmd sh -c "write $OBSIDIAN_MCP_EXAMPLE for filesystem-backed Obsidian MCP"
    return 0
  fi

  run_root mkdir -p "$OBSIDIAN_DIR"
  tmp_config=$(mktemp)
  trap 'rm -f "$tmp_config"' EXIT INT TERM

  cat > "$tmp_config" <<EOF
# Copy this block into /etc/teamd/config.toml under [daemon.mcp_connectors]
#
# This is the primary supported path for agent access to Obsidian:
# agentd -> stdio MCP -> docker run node -> $OBSIDIAN_MCP_PACKAGE -> vault.
# It does not require the Obsidian desktop app, a community plugin, or REST API.

[daemon.mcp_connectors.obsidian]
transport = "stdio"
command = "docker"
args = [
  "run",
  "-i",
  "--rm",
  "-v", "$OBSIDIAN_VAULT_DIR:/vault:rw",
  "$OBSIDIAN_MCP_NODE_IMAGE",
  "npx",
  "-y",
  "$OBSIDIAN_MCP_PACKAGE",
  "/vault",
]
enabled = false
EOF

  run_root install -m 0644 -o root -g root "$tmp_config" "$OBSIDIAN_MCP_EXAMPLE"
}

append_obsidian_mcp_connector_config() {
  config_parent=$(dirname "$CONFIG_FILE")

  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd mkdir -p "$config_parent"
    print_cmd sh -c "upsert enabled filesystem-backed Obsidian MCP connector in $CONFIG_FILE"
    return 0
  fi

  run_root mkdir -p "$config_parent"

  tmp_block=$(mktemp)
  tmp_config=$(mktemp)
  trap 'rm -f "$tmp_block" "$tmp_config"' EXIT INT TERM
  cat > "$tmp_block" <<EOF
[daemon.mcp_connectors.obsidian]
transport = "stdio"
command = "docker"
args = [
  "run",
  "-i",
  "--rm",
  "-v", "$OBSIDIAN_VAULT_DIR:/vault:rw",
  "$OBSIDIAN_MCP_NODE_IMAGE",
  "npx",
  "-y",
  "$OBSIDIAN_MCP_PACKAGE",
  "/vault",
]
enabled = true
EOF

  if [ -e "$CONFIG_FILE" ]; then
    if grep -F "[daemon.mcp_connectors.obsidian]" "$CONFIG_FILE" >/dev/null 2>&1; then
      awk -v block="$tmp_block" '
        function print_block() {
          while ((getline line < block) > 0) print line
          close(block)
        }
        /^\[daemon\.mcp_connectors\.obsidian\][[:space:]]*$/ {
          print_block()
          in_obsidian = 1
          next
        }
        in_obsidian && /^\[/ {
          in_obsidian = 0
        }
        !in_obsidian {
          print
        }
      ' "$CONFIG_FILE" > "$tmp_config"
    else
      {
        cat "$CONFIG_FILE"
        printf '\n'
        cat "$tmp_block"
      } > "$tmp_config"
    fi
  else
    cat "$tmp_block" > "$tmp_config"
  fi

  run_root install -m 0644 -o root -g root "$tmp_config" "$CONFIG_FILE"
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

  jaeger_domain_block=
  jaeger_http_redirect=
  jaeger_handle=
  if [ "$ENABLE_JAEGER" -eq 1 ]; then
    if [ -n "$CADDY_DOMAIN" ]; then
      jaeger_domain_block="
jaeger.$CADDY_DOMAIN {
  reverse_proxy teamd-jaeger:16686
}
"
    else
      jaeger_http_redirect="  redir /jaeger /jaeger/ 308"
      jaeger_handle='  handle /jaeger/* {
    reverse_proxy teamd-jaeger:16686
  }'
    fi
  fi

  if [ -n "$CADDY_DOMAIN" ]; then
    cat > "$tmp_caddyfile" <<EOF
search.$CADDY_DOMAIN {
  reverse_proxy teamd-searxng:8080
}

obsidian.$CADDY_DOMAIN {
  reverse_proxy teamd-obsidian:$OBSIDIAN_CONTAINER_PORT
}
$jaeger_domain_block
EOF
  else
    if [ -n "$OBSIDIAN_SUBFOLDER" ]; then
      obsidian_route='handle /obsidian/*'
    else
      obsidian_route='handle_path /obsidian/*'
    fi

    if [ -n "$CADDY_HTTPS_PORT" ]; then
      cat > "$tmp_caddyfile" <<EOF
{
  auto_https disable_redirects
  default_sni $CADDY_HOST
}

:80 {
  redir /searxng /searxng/ 308
  redir /obsidian https://$CADDY_HOST:$CADDY_HTTPS_PORT/obsidian/ 308
  redir /obsidian/* https://$CADDY_HOST:$CADDY_HTTPS_PORT{uri} 308
$jaeger_http_redirect

  handle /searxng/* {
    reverse_proxy teamd-searxng:8080 {
      header_up X-Script-Name /searxng
    }
  }
$jaeger_handle

  respond / "teamD container edge: /searxng/ on HTTP, /obsidian/ on HTTPS"
}

https://$CADDY_HOST {
  tls internal

  handle /searxng/* {
    reverse_proxy teamd-searxng:8080 {
      header_up X-Script-Name /searxng
    }
  }
$jaeger_handle

  $obsidian_route {
    reverse_proxy teamd-obsidian:$OBSIDIAN_CONTAINER_PORT
  }

  respond / "teamD container edge (TLS): /searxng/ and /obsidian/"
}
EOF
    else
      cat > "$tmp_caddyfile" <<EOF
:80 {
  redir /searxng /searxng/ 308
$jaeger_http_redirect

  handle /searxng/* {
    reverse_proxy teamd-searxng:8080 {
      header_up X-Script-Name /searxng
    }
  }
$jaeger_handle

  $obsidian_route {
    reverse_proxy teamd-obsidian:$OBSIDIAN_CONTAINER_PORT
  }

  respond / "teamD container edge: /searxng/ and /obsidian/"
}
EOF
    fi
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
    --with-jaeger) ENABLE_JAEGER=1 ;;
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
    --jaeger-ui-port)
      shift
      [ "$#" -gt 0 ] || fail "--jaeger-ui-port requires a value"
      valid_port "$1" || fail "invalid --jaeger-ui-port: $1"
      JAEGER_UI_PORT=$1
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
ensure_obsidian_https_port
ensure_caddy_host

if [ "$(id -u)" -ne 0 ] && [ "$DRY_RUN" -eq 0 ]; then
  need_command sudo
fi

if [ "$ENABLE_SEARXNG" -eq 0 ] && [ "$ENABLE_OBSIDIAN" -eq 0 ] && [ "$ENABLE_JAEGER" -eq 0 ] && [ "$ENABLE_CADDY" -eq 0 ]; then
  fail "nothing to deploy: SearXNG disabled, Obsidian not enabled, Jaeger not enabled, and Caddy disabled"
fi

ensure_docker
ensure_edge_network

if [ "$ENABLE_SEARXNG" -eq 1 ]; then
  write_searxng_files
  configure_agentd_web_search_env
  compose_up "$SEARXNG_COMPOSE"
fi

if [ "$ENABLE_OBSIDIAN" -eq 1 ]; then
  write_obsidian_files
  ensure_obsidian_legacy_vault_link
  write_obsidian_vault_config
  compose_up "$OBSIDIAN_COMPOSE"
fi

if [ "$ENABLE_JAEGER" -eq 1 ]; then
  write_jaeger_files
  configure_agentd_otlp_env
  compose_up "$JAEGER_COMPOSE"
fi

if [ "$WRITE_OBSIDIAN_MCP_EXAMPLE" -eq 1 ]; then
  write_obsidian_mcp_example
fi

if [ "$ENABLE_OBSIDIAN_MCP" -eq 1 ]; then
  append_obsidian_mcp_connector_config
  ensure_teamd_docker_access
fi

if [ "$ENABLE_CADDY" -eq 1 ]; then
  write_caddy_files
  compose_up_caddy
  reload_caddy_if_running
fi

if [ "$ENABLE_SEARXNG" -eq 1 ] || [ "$ENABLE_OBSIDIAN_MCP" -eq 1 ] || [ "$ENABLE_JAEGER" -eq 1 ]; then
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
      Env file: $ENV_FILE
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
    Local URL: http://127.0.0.1:$OBSIDIAN_PORT$OBSIDIAN_SUBFOLDER
    Compose: $OBSIDIAN_COMPOSE
    Start command: docker compose -f $OBSIDIAN_COMPOSE up -d
    Vaults: $OBSIDIAN_VAULTS_DIR
    Managed vault: $OBSIDIAN_VAULT_DIR
EOF
  if [ -n "$CADDY_DOMAIN" ]; then
    cat <<EOF
    Caddy URL: https://obsidian.$CADDY_DOMAIN/
EOF
  elif [ -n "$CADDY_HTTPS_PORT" ]; then
    cat <<EOF
    Caddy URL: https://$CADDY_HOST:$CADDY_HTTPS_PORT/obsidian/
EOF
  fi
  if [ "$ENABLE_OBSIDIAN_MCP" -eq 1 ]; then
    cat <<EOF
    Automated MCP:
      Package: $OBSIDIAN_MCP_PACKAGE
      Runtime image: $OBSIDIAN_MCP_NODE_IMAGE
      Vault mount: $OBSIDIAN_VAULT_DIR:/vault:rw
      agentd config: $CONFIG_FILE
      Service user Docker access: $SERVICE_USER -> docker group
      Restarted services unless absent/no-start: $DAEMON_SERVICE, $TELEGRAM_SERVICE
EOF
  else
    cat <<EOF
    Agent access:
      run again with --with-obsidian-mcp to add filesystem-backed Obsidian MCP
EOF
  fi
  if [ "$WRITE_OBSIDIAN_MCP_EXAMPLE" -eq 1 ]; then
    cat <<EOF
    MCP example: $OBSIDIAN_MCP_EXAMPLE
    MCP package: $OBSIDIAN_MCP_PACKAGE
    MCP runtime image: $OBSIDIAN_MCP_NODE_IMAGE
EOF
  fi
fi

if [ "$ENABLE_JAEGER" -eq 1 ]; then
  cat <<EOF
  Jaeger:
    Container: teamd-jaeger
    UI URL: http://127.0.0.1:$JAEGER_UI_PORT$JAEGER_BASE_PATH
    OTLP gRPC: 127.0.0.1:$JAEGER_OTLP_GRPC_PORT
    OTLP HTTP: http://127.0.0.1:$JAEGER_OTLP_HTTP_PORT/v1/traces
    Compose: $JAEGER_COMPOSE
    Start command: docker compose -f $JAEGER_COMPOSE up -d
    Storage: $JAEGER_DATA_DIR
    agentd OTLP auto-export:
      Env file: $ENV_FILE
      TEAMD_OTLP_EXPORT_ENABLED=true
      TEAMD_OTLP_ENDPOINT=http://127.0.0.1:$JAEGER_OTLP_HTTP_PORT/v1/traces
      TEAMD_OTLP_TIMEOUT_MS=$OTLP_EXPORT_TIMEOUT_MS
EOF
  if [ -n "$CADDY_DOMAIN" ]; then
    cat <<EOF
    Caddy URL: https://jaeger.$CADDY_DOMAIN/
EOF
  else
    cat <<EOF
    Caddy URL: http://127.0.0.1:$CADDY_HTTP_PORT/jaeger/
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
EOF
  if [ -n "$CADDY_DOMAIN" ]; then
    cat <<EOF
    Routes with TEAMD_CADDY_DOMAIN: search.<domain>, obsidian.<domain> and jaeger.<domain> when enabled
EOF
  elif [ -n "$CADDY_HTTPS_PORT" ]; then
    cat <<EOF
    Routes without TEAMD_CADDY_DOMAIN:
      HTTP: /searxng/
      HTTP: /jaeger/ when enabled
      HTTPS: https://$CADDY_HOST:$CADDY_HTTPS_PORT/obsidian/
EOF
  else
    cat <<EOF
    Routes without TEAMD_CADDY_DOMAIN: /searxng/, /obsidian/ and /jaeger/ when enabled
EOF
  fi
fi
