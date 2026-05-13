#!/bin/sh
set -eu

PROGRAM=$(basename "$0")

DRY_RUN=0
NON_INTERACTIVE=0
SKIP_START=0
INSTALL_DOCKER=${TEAMD_CONTAINERS_INSTALL_DOCKER:-1}
ENABLE_NATS=1
ENABLE_SEARXNG=1
ENABLE_JAEGER=0
ENABLE_MEM0=0
ENABLE_SILVERBULLET=0
ENABLE_SILVERBULLET_MCP=0
WRITE_SILVERBULLET_MCP_EXAMPLE=0
ENABLE_BROWSERLESS=0
INSTALL_AGENT_BROWSER=0
ENABLE_FILEBROWSER=0
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

NATS_PORT=${TEAMD_NATS_PORT:-4222}
NATS_MONITOR_PORT=${TEAMD_NATS_MONITOR_PORT:-8222}
NATS_IMAGE=${TEAMD_NATS_IMAGE:-docker.io/library/nats:2-alpine}
NATS_DIR=$CONTAINERS_ROOT/nats
NATS_DATA_DIR=$DATA_ROOT/nats
NATS_COMPOSE=$NATS_DIR/docker-compose.yml

SEARXNG_PORT=${TEAMD_SEARXNG_PORT:-8888}
SEARXNG_IMAGE=${TEAMD_SEARXNG_IMAGE:-docker.io/searxng/searxng:latest}
SEARXNG_DIR=$CONTAINERS_ROOT/searxng
SEARXNG_CONFIG_DIR=$DATA_ROOT/searxng/config
SEARXNG_DATA_DIR=$DATA_ROOT/searxng/data
SEARXNG_COMPOSE=$SEARXNG_DIR/docker-compose.yml
SEARXNG_SETTINGS=$SEARXNG_CONFIG_DIR/settings.yml
SEARXNG_MCP_EXAMPLE=$SEARXNG_DIR/mcp-searxng.example.json

SILVERBULLET_IMAGE=${TEAMD_SILVERBULLET_IMAGE:-ghcr.io/silverbulletmd/silverbullet:latest}
SILVERBULLET_PORT=${TEAMD_SILVERBULLET_PORT:-8091}
SILVERBULLET_CONTAINER_PORT=${TEAMD_SILVERBULLET_CONTAINER_PORT:-3000}
SILVERBULLET_HTTPS_PORT=${TEAMD_SILVERBULLET_HTTPS_PORT:-}
SILVERBULLET_URL_PREFIX=${TEAMD_SILVERBULLET_URL_PREFIX:-}
SILVERBULLET_DIR=$CONTAINERS_ROOT/silverbullet
SILVERBULLET_COMPOSE=$SILVERBULLET_DIR/docker-compose.yml
SILVERBULLET_ENV_FILE=${TEAMD_SILVERBULLET_ENV_FILE:-$SILVERBULLET_DIR/silverbullet.env}
SILVERBULLET_USER=${TEAMD_SILVERBULLET_USER:-}
SILVERBULLET_SPACES_DIR=${TEAMD_SILVERBULLET_SPACES_DIR:-/var/lib/teamd/knowledge/silverbullet}
SILVERBULLET_SPACE_NAME=${TEAMD_SILVERBULLET_SPACE_NAME:-teamd}
SILVERBULLET_SPACE_DIR=${TEAMD_SILVERBULLET_SPACE_DIR:-$SILVERBULLET_SPACES_DIR/$SILVERBULLET_SPACE_NAME}
SILVERBULLET_MCP_REPOSITORY=${TEAMD_SILVERBULLET_MCP_REPOSITORY:-https://github.com/Ahmad-A0/silverbullet-mcp.git}
SILVERBULLET_MCP_REF=${TEAMD_SILVERBULLET_MCP_REF:-v1.1.0}
SILVERBULLET_MCP_PORT=${TEAMD_SILVERBULLET_MCP_PORT:-4000}
SILVERBULLET_MCP_CONTAINER_PORT=${TEAMD_SILVERBULLET_MCP_CONTAINER_PORT:-4000}
SILVERBULLET_MCP_NODE_IMAGE=${TEAMD_SILVERBULLET_MCP_NODE_IMAGE:-docker.io/library/node:22-alpine}
SILVERBULLET_MCP_STDIO_WRAPPER=$SILVERBULLET_DIR/silverbullet-mcp-stdio.sh
SILVERBULLET_MCP_EXAMPLE=$SILVERBULLET_DIR/silverbullet-mcp.example.toml

BROWSERLESS_IMAGE=${TEAMD_BROWSERLESS_IMAGE:-ghcr.io/browserless/chromium:latest}
BROWSERLESS_PORT=${TEAMD_BROWSERLESS_PORT:-3000}
BROWSERLESS_CONTAINER_PORT=${TEAMD_BROWSERLESS_CONTAINER_PORT:-3000}
BROWSERLESS_CONCURRENT=${TEAMD_BROWSERLESS_CONCURRENT:-3}
BROWSERLESS_DIR=$CONTAINERS_ROOT/browserless
BROWSERLESS_COMPOSE=$BROWSERLESS_DIR/docker-compose.yml
BROWSERLESS_ENV_FILE=${TEAMD_BROWSERLESS_ENV_FILE:-$BROWSERLESS_DIR/browserless.env}
BROWSERLESS_API_URL=${TEAMD_BROWSERLESS_API_URL:-http://127.0.0.1:$BROWSERLESS_PORT}
BROWSERLESS_CDP_URL=${TEAMD_BROWSERLESS_CDP_URL:-ws://127.0.0.1:$BROWSERLESS_PORT/chromium}
BROWSERLESS_BROWSER_TYPE=${TEAMD_BROWSERLESS_BROWSER_TYPE:-chromium}
BROWSERLESS_TTL_MS=${TEAMD_BROWSERLESS_TTL_MS:-300000}
BROWSERLESS_STEALTH=${TEAMD_BROWSERLESS_STEALTH:-true}
BROWSERLESS_TOKEN=${TEAMD_BROWSERLESS_TOKEN:-}

AGENT_BROWSER_NPM_PACKAGE=${TEAMD_AGENT_BROWSER_NPM_PACKAGE:-agent-browser@latest}
AGENT_BROWSER_INSTALL_DIR=${TEAMD_AGENT_BROWSER_INSTALL_DIR:-/opt/teamd/agent-browser}
AGENT_BROWSER_BIN=${TEAMD_AGENT_BROWSER_BIN:-/opt/teamd/bin/agent-browser}
AGENT_BROWSER_PATH_LINK=${TEAMD_AGENT_BROWSER_PATH_LINK:-/usr/local/bin/agent-browser}
AGENT_BROWSER_SESSION_PREFIX=${TEAMD_AGENT_BROWSER_SESSION_PREFIX:-teamd}
AGENT_BROWSER_DEFAULT_TIMEOUT_MS=${TEAMD_AGENT_BROWSER_DEFAULT_TIMEOUT_MS:-30000}
AGENT_BROWSER_MAX_OUTPUT_CHARS=${TEAMD_AGENT_BROWSER_MAX_OUTPUT_CHARS:-20000}

FILEBROWSER_IMAGE=${TEAMD_FILEBROWSER_IMAGE:-docker.io/filebrowser/filebrowser:s6}
FILEBROWSER_PORT=${TEAMD_FILEBROWSER_PORT:-8092}
FILEBROWSER_CONTAINER_PORT=${TEAMD_FILEBROWSER_CONTAINER_PORT:-80}
FILEBROWSER_DIR=$CONTAINERS_ROOT/filebrowser
FILEBROWSER_COMPOSE=$FILEBROWSER_DIR/docker-compose.yml
FILEBROWSER_ENV_FILE=${TEAMD_FILEBROWSER_ENV_FILE:-$FILEBROWSER_DIR/filebrowser.env}
FILEBROWSER_DB_DIR=${TEAMD_FILEBROWSER_DB_DIR:-$DATA_ROOT/filebrowser/database}
FILEBROWSER_CONFIG_DIR=${TEAMD_FILEBROWSER_CONFIG_DIR:-$DATA_ROOT/filebrowser/config}
FILEBROWSER_AGENT_HOMES_DIR=${TEAMD_FILEBROWSER_AGENT_HOMES_DIR:-/var/lib/teamd/state/agents}
FILEBROWSER_WORKSPACES_DIR=${TEAMD_FILEBROWSER_WORKSPACES_DIR:-/var/lib/teamd/workspaces}
FILEBROWSER_ARTIFACTS_DIR=${TEAMD_FILEBROWSER_ARTIFACTS_DIR:-/var/lib/teamd/state/artifacts}
FILEBROWSER_KNOWLEDGE_DIR=${TEAMD_FILEBROWSER_KNOWLEDGE_DIR:-/var/lib/teamd/knowledge}
FILEBROWSER_DOCS_DIR=${TEAMD_FILEBROWSER_DOCS_DIR:-}
FILEBROWSER_ADMIN_USER=${TEAMD_FILEBROWSER_ADMIN_USER:-admin}
FILEBROWSER_ADMIN_PASSWORD=${TEAMD_FILEBROWSER_ADMIN_PASSWORD:-}
FILEBROWSER_PUID=${TEAMD_FILEBROWSER_PUID:-}
FILEBROWSER_PGID=${TEAMD_FILEBROWSER_PGID:-}

JAEGER_UI_PORT=${TEAMD_JAEGER_UI_PORT:-16686}
JAEGER_OTLP_GRPC_PORT=${TEAMD_JAEGER_OTLP_GRPC_PORT:-4317}
JAEGER_OTLP_HTTP_PORT=${TEAMD_JAEGER_OTLP_HTTP_PORT:-4318}
JAEGER_IMAGE=${TEAMD_JAEGER_IMAGE:-docker.io/jaegertracing/all-in-one:1.76.0}
JAEGER_UID=${TEAMD_JAEGER_UID:-10001}
JAEGER_GID=${TEAMD_JAEGER_GID:-10001}
JAEGER_DIR=$CONTAINERS_ROOT/jaeger
JAEGER_DATA_DIR=$DATA_ROOT/jaeger/badger
JAEGER_COMPOSE=$JAEGER_DIR/docker-compose.yml
JAEGER_BASE_PATH_EXPLICIT=0
if [ "${TEAMD_JAEGER_BASE_PATH+x}" ]; then
  JAEGER_BASE_PATH_EXPLICIT=1
  JAEGER_BASE_PATH=$TEAMD_JAEGER_BASE_PATH
elif [ -n "${TEAMD_CADDY_DOMAIN:-}" ] && [ "${TEAMD_CADDY_SINGLE_DOMAIN:-0}" != "1" ]; then
  JAEGER_BASE_PATH=/
else
  JAEGER_BASE_PATH=/jaeger
fi

MEM0_PORT=${TEAMD_MEM0_PORT:-18888}
MEM0_API_BASE=${TEAMD_MEM0_API_BASE:-http://127.0.0.1:$MEM0_PORT}
MEM0_API_KEY=${TEAMD_MEM0_API_KEY:-}
MEM0_DEFAULT_USER_ID=${TEAMD_MEM0_DEFAULT_USER_ID:-local-operator}
MEM0_REQUEST_TIMEOUT_MS=${TEAMD_MEM0_REQUEST_TIMEOUT_MS:-120000}
MEM0_DEFAULT_LIMIT=${TEAMD_MEM0_DEFAULT_LIMIT:-10}
MEM0_MAX_LIMIT=${TEAMD_MEM0_MAX_LIMIT:-50}
MEMORY_CURATOR_ENABLED=${TEAMD_MEMORY_CURATOR_ENABLED:-true}
MEMORY_CURATOR_MODE=${TEAMD_MEMORY_CURATOR_MODE:-auto}
MEMORY_CURATOR_MIN_CONFIDENCE=${TEAMD_MEMORY_CURATOR_MIN_CONFIDENCE:-0.8}
MEMORY_CURATOR_MAX_CANDIDATES=${TEAMD_MEMORY_CURATOR_MAX_CANDIDATES:-5}
MEMORY_CURATOR_MAX_OUTPUT_TOKENS=${TEAMD_MEMORY_CURATOR_MAX_OUTPUT_TOKENS:-512}
MEMORY_RECALL_ENABLED=${TEAMD_MEMORY_RECALL_ENABLED:-true}
MEMORY_RECALL_SCOPES=${TEAMD_MEMORY_RECALL_SCOPES:-operator,workspace,agent_shared}
MEMORY_RECALL_MAX_RESULTS=${TEAMD_MEMORY_RECALL_MAX_RESULTS:-6}
MEMORY_RECALL_MAX_QUERY_CHARS=${TEAMD_MEMORY_RECALL_MAX_QUERY_CHARS:-512}
MEMORY_RECALL_MAX_MEMORY_CHARS=${TEAMD_MEMORY_RECALL_MAX_MEMORY_CHARS:-800}
MEM0_DIR=$CONTAINERS_ROOT/mem0
MEM0_SRC_DIR=$MEM0_DIR/src
MEM0_COMPOSE=$MEM0_DIR/docker-compose.yml
MEM0_ENV_FILE=${TEAMD_MEM0_ENV_FILE:-$MEM0_DIR/mem0.env}
MEM0_INIT_DB=$MEM0_DIR/init-db.sh
MEM0_DATA_DIR=$DATA_ROOT/mem0
MEM0_HISTORY_DIR=$MEM0_DATA_DIR/history
MEM0_POSTGRES_DATA_DIR=$MEM0_DATA_DIR/postgres
MEM0_CACHE_DIR=$MEM0_DATA_DIR/cache
MEM0_REPOSITORY=${TEAMD_MEM0_REPOSITORY:-https://github.com/mem0ai/mem0.git}
MEM0_REF=${TEAMD_MEM0_REF:-main}
MEM0_IMAGE=${TEAMD_MEM0_IMAGE:-teamd-mem0-api:latest}
MEM0_POSTGRES_IMAGE=${TEAMD_MEM0_POSTGRES_IMAGE:-ankane/pgvector:v0.5.1}
MEM0_POSTGRES_UID=${TEAMD_MEM0_POSTGRES_UID:-999}
MEM0_POSTGRES_GID=${TEAMD_MEM0_POSTGRES_GID:-999}
MEM0_POSTGRES_DB=${TEAMD_MEM0_POSTGRES_DB:-postgres}
MEM0_POSTGRES_USER=${TEAMD_MEM0_POSTGRES_USER:-postgres}
MEM0_POSTGRES_PASSWORD=${TEAMD_MEM0_POSTGRES_PASSWORD:-}
MEM0_APP_DB_NAME=${TEAMD_MEM0_APP_DB_NAME:-mem0_app}
MEM0_ADMIN_API_KEY=${TEAMD_MEM0_ADMIN_API_KEY:-$MEM0_API_KEY}
MEM0_JWT_SECRET=${TEAMD_MEM0_JWT_SECRET:-}
MEM0_COLLECTION_NAME=${TEAMD_MEM0_COLLECTION_NAME:-teamd_memories_fastembed_384}
MEM0_EMBEDDING_DIMS=${TEAMD_MEM0_EMBEDDING_DIMS:-384}
MEM0_FASTEMBED_MODEL=${TEAMD_MEM0_FASTEMBED_MODEL:-sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2}
MEM0_LLM_API_BASE=${TEAMD_MEM0_LLM_API_BASE:-https://api.z.ai/api/coding/paas/v4}
MEM0_LLM_API_KEY=${TEAMD_MEM0_LLM_API_KEY:-}
MEM0_LLM_MODEL=${TEAMD_MEM0_LLM_MODEL:-glm-4.5-air}
MEM0_LLM_TEMPERATURE=${TEAMD_MEM0_LLM_TEMPERATURE:-0.2}
MEM0_LLM_MAX_TOKENS=${TEAMD_MEM0_LLM_MAX_TOKENS:-2000}
OTLP_EXPORT_TIMEOUT_MS=${TEAMD_OTLP_TIMEOUT_MS:-2000}

CADDY_DOMAIN=${TEAMD_CADDY_DOMAIN:-}
CADDY_SINGLE_DOMAIN=${TEAMD_CADDY_SINGLE_DOMAIN:-0}
CADDY_HOST=${TEAMD_CADDY_HOST:-}
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
CADDY_DAEMON_UPSTREAM=${TEAMD_CADDY_DAEMON_UPSTREAM:-host.docker.internal:5140}
CADDY_WEB_UPSTREAM=${TEAMD_CADDY_WEB_UPSTREAM:-host.docker.internal:5173}
CADDY_DIR=$CONTAINERS_ROOT/caddy
CADDY_DATA_DIR=$DATA_ROOT/caddy/data
CADDY_CONFIG_DIR=$DATA_ROOT/caddy/config
CADDY_COMPOSE=$CADDY_DIR/docker-compose.yml
CADDYFILE=$CADDY_DIR/Caddyfile
FILEBROWSER_BASE_URL=

resolve_filebrowser_base_url() {
  if [ "${TEAMD_FILEBROWSER_BASE_URL+x}" ]; then
    FILEBROWSER_BASE_URL=$TEAMD_FILEBROWSER_BASE_URL
  elif [ -n "$CADDY_DOMAIN" ] && [ "$CADDY_SINGLE_DOMAIN" -eq 0 ]; then
    FILEBROWSER_BASE_URL=
  else
    FILEBROWSER_BASE_URL=/files
  fi
}

usage() {
  cat <<EOF
Usage: $PROGRAM [options]

Deploy teamD container add-ons without changing the main agentd deploy path.

By default this installs/uses Docker Engine, deploys a local SearXNG instance
bound to 127.0.0.1:$SEARXNG_PORT, and starts Caddy as an edge reverse proxy.
SilverBullet, SilverBullet MCP, Browserless, Mem0, File Browser, and Jaeger are opt-in.

Options:
  --dry-run             Print actions without changing the system.
  --non-interactive     Do not prompt.
  --no-install-docker   Fail if Docker Engine or Docker Compose plugin is missing.
  --no-start            Write files but do not start containers.
  --no-nats             Do not deploy local NATS JetStream.
  --no-searxng          Do not deploy SearXNG.
  --no-caddy            Do not deploy Caddy reverse proxy.
  --with-jaeger         Also deploy Jaeger UI and enable agentd OTLP auto-export.
  --with-mem0           Deploy Mem0/OpenMemory REST API and configure agentd memory_* tools.
  --with-silverbullet   Deploy SilverBullet editor over the canonical Markdown space.
  --with-silverbullet-mcp
                         Deploy SilverBullet plus MCP bridge and agentd MCP connector.
  --with-silverbullet-mcp-example
                         Write an agentd stdio MCP connector example for SilverBullet.
  --with-browserless     Deploy Browserless and install/configure agent-browser.
  --with-agent-browser   Install/configure agent-browser without deploying Browserless.
  --with-filebrowser     Deploy File Browser for operator editing of agent homes,
                         skills, approved workspaces, artifacts, and knowledge files.
  --single-domain       With TEAMD_CADDY_DOMAIN, publish all add-ons on that
                         exact host: /sb/, /searxng/, /jaeger/, /files/.
  --no-restart-teamd    Do not restart teamd systemd services after writing MCP config.
  --searxng-port PORT   Local SearXNG port, default: $SEARXNG_PORT.
  --jaeger-ui-port PORT Local Jaeger UI port, default: $JAEGER_UI_PORT.
  --silverbullet-port PORT
                         Local SilverBullet port, default: $SILVERBULLET_PORT.
  --silverbullet-https-port PORT
                         Caddy HTTPS port for SilverBullet without a domain,
                         default: 8444 when SilverBullet is enabled without TEAMD_CADDY_DOMAIN.
  --filebrowser-port PORT
                         Local File Browser port, default: $FILEBROWSER_PORT.
  -h, --help            Show this help.

Environment overrides:
  TEAMD_CONTAINERS_ROOT          Compose files root, default: $CONTAINERS_ROOT.
  TEAMD_CONTAINERS_DATA_ROOT     Persistent container data root, default: $DATA_ROOT.
  TEAMD_CONTAINERS_EDGE_NETWORK  Shared Docker network, default: $EDGE_NETWORK.
  TEAMD_CONTAINERS_INSTALL_DOCKER
                                 Auto-install Docker when missing, default: $INSTALL_DOCKER.
  TEAMD_NATS_IMAGE               NATS image, default: $NATS_IMAGE.
  TEAMD_NATS_PORT                Local NATS client port, default: $NATS_PORT.
  TEAMD_NATS_MONITOR_PORT        Local NATS monitor port, default: $NATS_MONITOR_PORT.
  TEAMD_SEARXNG_IMAGE            SearXNG image, default: $SEARXNG_IMAGE.
  TEAMD_SEARXNG_PORT             SearXNG localhost port, default: $SEARXNG_PORT.
  TEAMD_SILVERBULLET_IMAGE       SilverBullet image, default: $SILVERBULLET_IMAGE.
  TEAMD_SILVERBULLET_PORT        SilverBullet localhost port, default: $SILVERBULLET_PORT.
  TEAMD_SILVERBULLET_CONTAINER_PORT
                                 SilverBullet web port inside container,
                                 default: $SILVERBULLET_CONTAINER_PORT.
  TEAMD_SILVERBULLET_HTTPS_PORT  HTTPS Caddy port without a domain, default: "$SILVERBULLET_HTTPS_PORT".
  TEAMD_SILVERBULLET_URL_PREFIX  Optional URL prefix without trailing slash.
                                 In --single-domain mode default is /sb.
  TEAMD_SILVERBULLET_ENV_FILE    SB_USER credentials file, default: $SILVERBULLET_ENV_FILE.
  TEAMD_SILVERBULLET_USER        SilverBullet auth as username:password.
                                 If unset, deploy script generates and stores one.
  TEAMD_SILVERBULLET_SPACES_DIR  SilverBullet spaces root, default: $SILVERBULLET_SPACES_DIR.
  TEAMD_SILVERBULLET_SPACE_NAME  Managed space name, default: $SILVERBULLET_SPACE_NAME.
  TEAMD_SILVERBULLET_SPACE_DIR   Managed space directory, default: $SILVERBULLET_SPACE_DIR.
  TEAMD_SILVERBULLET_MCP_REPOSITORY
                                 SilverBullet MCP git repository, default: $SILVERBULLET_MCP_REPOSITORY.
  TEAMD_SILVERBULLET_MCP_REF     SilverBullet MCP git ref, default: $SILVERBULLET_MCP_REF.
  TEAMD_SILVERBULLET_MCP_PORT    SilverBullet MCP localhost port, default: $SILVERBULLET_MCP_PORT.
  TEAMD_SILVERBULLET_MCP_NODE_IMAGE
                                 Node image used for mcp-remote stdio bridge,
                                 default: $SILVERBULLET_MCP_NODE_IMAGE.
  TEAMD_BROWSERLESS_IMAGE        Browserless image, default: $BROWSERLESS_IMAGE.
  TEAMD_BROWSERLESS_PORT         Browserless localhost port, default: $BROWSERLESS_PORT.
  TEAMD_BROWSERLESS_CONTAINER_PORT
                                 Browserless container port, default: $BROWSERLESS_CONTAINER_PORT.
  TEAMD_BROWSERLESS_CONCURRENT   Browserless max concurrency, default: $BROWSERLESS_CONCURRENT.
  TEAMD_BROWSERLESS_ENV_FILE     Browserless TOKEN env file, default: $BROWSERLESS_ENV_FILE.
  TEAMD_BROWSERLESS_TOKEN        Browserless token. If unset, generated and stored.
  TEAMD_BROWSERLESS_API_URL      agent-browser Browserless URL,
                                 default: $BROWSERLESS_API_URL.
  TEAMD_BROWSERLESS_CDP_URL      agent-browser CDP endpoint for self-hosted Browserless,
                                 default: $BROWSERLESS_CDP_URL?token=<token>.
  TEAMD_BROWSERLESS_BROWSER_TYPE Browser type for agent-browser, default: $BROWSERLESS_BROWSER_TYPE.
  TEAMD_BROWSERLESS_TTL_MS       Browserless session TTL hint, default: $BROWSERLESS_TTL_MS.
  TEAMD_BROWSERLESS_STEALTH      agent-browser stealth flag, default: $BROWSERLESS_STEALTH.
  TEAMD_AGENT_BROWSER_NPM_PACKAGE
                                 npm package to install, default: $AGENT_BROWSER_NPM_PACKAGE.
  TEAMD_AGENT_BROWSER_INSTALL_DIR
                                 npm prefix for agent-browser, default: $AGENT_BROWSER_INSTALL_DIR.
  TEAMD_AGENT_BROWSER_BIN        Stable wrapper path used by agentd, default: $AGENT_BROWSER_BIN.
  TEAMD_AGENT_BROWSER_PATH_LINK  PATH symlink, default: $AGENT_BROWSER_PATH_LINK.
  TEAMD_AGENT_BROWSER_SESSION_PREFIX
                                 Browser session prefix, default: $AGENT_BROWSER_SESSION_PREFIX.
  TEAMD_AGENT_BROWSER_DEFAULT_TIMEOUT_MS
                                 Browser tool timeout, default: $AGENT_BROWSER_DEFAULT_TIMEOUT_MS.
  TEAMD_AGENT_BROWSER_MAX_OUTPUT_CHARS
                                 Browser tool output cap, default: $AGENT_BROWSER_MAX_OUTPUT_CHARS.
  TEAMD_MEM0_API_BASE            Mem0/OpenMemory REST base URL for agentd,
                                 default: $MEM0_API_BASE.
  TEAMD_MEM0_PORT                Local Mem0 API port when deploying the bundled
                                 backend, default: $MEM0_PORT.
  TEAMD_MEM0_API_KEY             Existing Mem0 X-API-Key for protected endpoints.
                                 If unset, deploy script generates and stores one.
  TEAMD_MEM0_REPOSITORY          Mem0 git repository, default: $MEM0_REPOSITORY.
  TEAMD_MEM0_REF                 Mem0 git ref, default: $MEM0_REF.
  TEAMD_MEM0_LLM_API_BASE        OpenAI-compatible LLM base URL for Mem0,
                                 default: $MEM0_LLM_API_BASE.
  TEAMD_MEM0_LLM_API_KEY         LLM API key for Mem0. If unset, falls back to
                                 TEAMD_PROVIDER_API_KEY from $ENV_FILE.
  TEAMD_MEM0_LLM_MODEL           LLM model for Mem0 extraction, default: $MEM0_LLM_MODEL.
  TEAMD_MEM0_FASTEMBED_MODEL     Local fastembed model, default: $MEM0_FASTEMBED_MODEL.
  TEAMD_MEM0_EMBEDDING_DIMS      fastembed vector dimensions, default: $MEM0_EMBEDDING_DIMS.
  TEAMD_MEM0_COLLECTION_NAME     pgvector collection, default: $MEM0_COLLECTION_NAME.
  TEAMD_MEM0_DEFAULT_USER_ID     Default Mem0 user_id scope, default: $MEM0_DEFAULT_USER_ID.
  TEAMD_MEM0_REQUEST_TIMEOUT_MS  Mem0 HTTP request timeout, default: $MEM0_REQUEST_TIMEOUT_MS.
  TEAMD_MEM0_DEFAULT_LIMIT       Default memory list/search limit, default: $MEM0_DEFAULT_LIMIT.
  TEAMD_MEM0_MAX_LIMIT           Maximum memory list/search limit, default: $MEM0_MAX_LIMIT.
  TEAMD_MEMORY_CURATOR_ENABLED   Enable post-turn memory curator when Mem0 is deployed,
                                 default: $MEMORY_CURATOR_ENABLED.
  TEAMD_MEMORY_CURATOR_MODE      Curator mode: auto, review, or off. Default: $MEMORY_CURATOR_MODE.
  TEAMD_MEMORY_CURATOR_MIN_CONFIDENCE
                                 Minimum confidence for auto-save, default: $MEMORY_CURATOR_MIN_CONFIDENCE.
  TEAMD_MEMORY_CURATOR_MAX_CANDIDATES
                                 Max candidates per turn, default: $MEMORY_CURATOR_MAX_CANDIDATES.
  TEAMD_MEMORY_CURATOR_MAX_OUTPUT_TOKENS
                                 Curator provider cap, default: $MEMORY_CURATOR_MAX_OUTPUT_TOKENS.
  TEAMD_MEMORY_RECALL_ENABLED   Enable pre-turn prompt Memory Recall when Mem0 is deployed,
                                 default: $MEMORY_RECALL_ENABLED.
  TEAMD_MEMORY_RECALL_SCOPES    Comma-separated recall scopes, default: $MEMORY_RECALL_SCOPES.
  TEAMD_MEMORY_RECALL_MAX_RESULTS
                                 Max memories inserted into the prompt, default: $MEMORY_RECALL_MAX_RESULTS.
  TEAMD_FILEBROWSER_IMAGE       File Browser image, default: $FILEBROWSER_IMAGE.
  TEAMD_FILEBROWSER_PORT        Local File Browser port, default: $FILEBROWSER_PORT.
  TEAMD_FILEBROWSER_ADMIN_USER  Admin username, default: $FILEBROWSER_ADMIN_USER.
  TEAMD_FILEBROWSER_ADMIN_PASSWORD
                                 Admin password; generated into $FILEBROWSER_ENV_FILE when unset.
  TEAMD_FILEBROWSER_AGENT_HOMES_DIR
                                 Mounted as /srv/agent-homes, default: $FILEBROWSER_AGENT_HOMES_DIR.
  TEAMD_FILEBROWSER_WORKSPACES_DIR
                                 Mounted as /srv/workspaces, default: $FILEBROWSER_WORKSPACES_DIR.
  TEAMD_FILEBROWSER_ARTIFACTS_DIR
                                 Mounted as /srv/artifacts, default: $FILEBROWSER_ARTIFACTS_DIR.
  TEAMD_FILEBROWSER_KNOWLEDGE_DIR
                                 Mounted as /srv/knowledge, default: $FILEBROWSER_KNOWLEDGE_DIR.
  TEAMD_FILEBROWSER_DOCS_DIR    Optional docs mount as /srv/docs.
  TEAMD_FILEBROWSER_BASE_URL    File Browser reverse-proxy base path.
                                 Default: /files except on dedicated files.<domain>.
  TEAMD_MEMORY_RECALL_MAX_QUERY_CHARS
                                Max latest-user query chars, default: $MEMORY_RECALL_MAX_QUERY_CHARS.
  TEAMD_MEMORY_RECALL_MAX_MEMORY_CHARS
                                Max chars per recalled memory, default: $MEMORY_RECALL_MAX_MEMORY_CHARS.
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
  TEAMD_CADDY_DOMAIN             Optional domain. By default creates search.<domain>,
                                 notes.<domain>, files.<domain>, and jaeger.<domain> when enabled.
  TEAMD_CADDY_SINGLE_DOMAIN      Set to 1 to use TEAMD_CADDY_DOMAIN as one exact host:
                                 / or /sb/ for SilverBullet, plus /searxng/,
                                 /jaeger/, and /files/ when enabled.
  TEAMD_CADDY_HOST               Hostname or IP for internal TLS without a dedicated domain.
                                 If unset, deploy script tries to detect the primary IPv4 address.
  TEAMD_CADDY_HTTP_PORT          Caddy HTTP host port, default: $CADDY_HTTP_PORT.
  TEAMD_CADDY_HTTPS_PORT         Caddy HTTPS host port. Default: 443 with TEAMD_CADDY_DOMAIN,
                                 otherwise disabled.
  TEAMD_CADDY_DAEMON_UPSTREAM    Upstream for daemon routes from Caddy,
                                 default: $CADDY_DAEMON_UPSTREAM.
  TEAMD_CADDY_WEB_UPSTREAM       Upstream for the native web console routes
                                 /web/ and /api/agentd/*, default: $CADDY_WEB_UPSTREAM.
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

install_node_npm_with_apt() {
  os_info=$(detect_os_for_docker_apt || true)
  [ -n "$os_info" ] || fail "Node/npm auto-install currently supports Ubuntu/Debian apt only"

  printf 'Installing Node.js/npm from apt for agent-browser CLI.\n'
  run_root env DEBIAN_FRONTEND=noninteractive apt-get update
  run_root env DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
    nodejs npm
}

ensure_npm() {
  if [ "$DRY_RUN" -eq 1 ]; then
    printf 'DRY RUN: ensure Node.js and npm are available for agent-browser.\n'
    return 0
  fi

  if command -v npm >/dev/null 2>&1 && command -v node >/dev/null 2>&1; then
    return 0
  fi

  if [ "$INSTALL_DOCKER" != "1" ]; then
    fail "Node.js/npm is required for agent-browser; install npm or omit --no-install-docker"
  fi

  install_node_npm_with_apt
  command -v npm >/dev/null 2>&1 || fail "npm is still unavailable after apt install"
  command -v node >/dev/null 2>&1 || fail "node is still unavailable after apt install"
}

validate_caddy_domain_mode() {
  case "$CADDY_SINGLE_DOMAIN" in
    0|1) ;;
    *) fail "TEAMD_CADDY_SINGLE_DOMAIN must be 0 or 1" ;;
  esac
  [ "$CADDY_SINGLE_DOMAIN" -eq 0 ] || [ -n "$CADDY_DOMAIN" ] || fail "--single-domain requires TEAMD_CADDY_DOMAIN"
  if [ "$CADDY_SINGLE_DOMAIN" -eq 1 ] && [ "$JAEGER_BASE_PATH_EXPLICIT" -eq 0 ]; then
    JAEGER_BASE_PATH=/jaeger
  fi
}

ensure_silverbullet_https_port() {
  [ "$ENABLE_SILVERBULLET" -eq 1 ] || return 0
  [ "$ENABLE_CADDY" -eq 1 ] || return 0
  [ -n "$CADDY_DOMAIN" ] && return 0
  [ -n "$SILVERBULLET_HTTPS_PORT" ] && return 0

  SILVERBULLET_HTTPS_PORT=8444
}

silverbullet_effective_url_prefix() {
  if [ -n "$SILVERBULLET_URL_PREFIX" ]; then
    printf '%s' "$SILVERBULLET_URL_PREFIX"
  elif [ "$ENABLE_SILVERBULLET" -eq 1 ] && [ -n "$CADDY_DOMAIN" ] && [ "$CADDY_SINGLE_DOMAIN" -eq 1 ]; then
    printf '/sb'
  fi
}

validate_silverbullet_url_prefix() {
  local prefix
  prefix=$(silverbullet_effective_url_prefix)
  [ -n "$prefix" ] || return 0
  case "$prefix" in
    /*) ;;
    *) fail "TEAMD_SILVERBULLET_URL_PREFIX must start with /: $prefix" ;;
  esac
  case "$prefix" in
    */) fail "TEAMD_SILVERBULLET_URL_PREFIX must not end with /: $prefix" ;;
  esac
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
  [ "$ENABLE_CADDY" -eq 1 ] || return 0
  [ -n "$CADDY_DOMAIN" ] && return 0
  need_host=0
  if [ "$ENABLE_SILVERBULLET" -eq 1 ] && [ -n "$SILVERBULLET_HTTPS_PORT" ]; then
    need_host=1
  fi
  [ "$need_host" -eq 1 ] || return 0
  [ -n "$CADDY_HOST" ] && return 0

  if [ "$DRY_RUN" -eq 1 ]; then
    CADDY_HOST=127.0.0.1
    return 0
  fi

  CADDY_HOST=$(detect_primary_ipv4 || true)
  [ -n "$CADDY_HOST" ] || fail "cannot detect Caddy host/IP for HTTPS add-ons; set TEAMD_CADDY_HOST explicitly"
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

docker_components_enabled() {
  [ "$ENABLE_NATS" -eq 1 ] ||
  [ "$ENABLE_SEARXNG" -eq 1 ] ||
    [ "$ENABLE_JAEGER" -eq 1 ] ||
    [ "$ENABLE_MEM0" -eq 1 ] ||
    [ "$ENABLE_BROWSERLESS" -eq 1 ] ||
    [ "$ENABLE_SILVERBULLET" -eq 1 ] ||
    [ "$ENABLE_FILEBROWSER" -eq 1 ] ||
    [ "$ENABLE_CADDY" -eq 1 ]
}

write_nats_files() {
  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd mkdir -p "$NATS_DIR" "$NATS_DATA_DIR"
    print_cmd sh -c "write $NATS_COMPOSE for teamd-nats JetStream"
    return 0
  fi

  run_root mkdir -p "$NATS_DIR" "$NATS_DATA_DIR"
  tmp_compose=$(mktemp)
  trap 'rm -f "$tmp_compose"' EXIT INT TERM

  cat > "$tmp_compose" <<EOF
services:
  nats:
    image: $NATS_IMAGE
    container_name: teamd-nats
    restart: unless-stopped
    command: ["-js", "-sd", "/data", "-m", "8222"]
    ports:
      - "127.0.0.1:$NATS_PORT:4222"
      - "127.0.0.1:$NATS_MONITOR_PORT:8222"
    volumes:
      - "$NATS_DATA_DIR:/data:rw"
    networks:
      - $EDGE_NETWORK

networks:
  $EDGE_NETWORK:
    external: true
EOF

  run_root install -m 0644 -o root -g root "$tmp_compose" "$NATS_COMPOSE"
}

configure_agentd_nats_env() {
  env_parent=$(dirname "$ENV_FILE")
  nats_url="nats://127.0.0.1:$NATS_PORT"

  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd mkdir -p "$env_parent"
    print_cmd sh -c "upsert NATS event bus defaults in $ENV_FILE"
    return 0
  fi

  run_root mkdir -p "$env_parent"

  tmp_env=$(mktemp)
  tmp_new=$(mktemp)
  if [ -e "$ENV_FILE" ]; then
    awk '
      !/^(export[[:space:]]+)?TEAMD_EVENT_BUS_BACKEND=/ &&
      !/^(export[[:space:]]+)?TEAMD_NATS_URL=/ &&
      !/^(export[[:space:]]+)?TEAMD_EVENT_BUS_NATS_URL=/
    ' "$ENV_FILE" > "$tmp_env"
  else
    : > "$tmp_env"
  fi

  {
    cat "$tmp_env"
    [ ! -s "$tmp_env" ] || printf '\n'
    printf 'TEAMD_EVENT_BUS_BACKEND=%s\n' "$(quote_arg "nats_jetstream")"
    printf 'TEAMD_NATS_URL=%s\n' "$(quote_arg "$nats_url")"
  } > "$tmp_new"

  env_group=root
  if getent group "$SERVICE_GROUP" >/dev/null 2>&1; then
    env_group=$SERVICE_GROUP
  fi
  run_root install -m 0640 -o root -g "$env_group" "$tmp_new" "$ENV_FILE"
  rm -f "$tmp_env" "$tmp_new"
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

read_env_file_value() {
  env_key=$1
  env_path=$2

  [ -r "$env_path" ] || return 1
  value=$(sed -n -E "s/^(export[[:space:]]+)?${env_key}=//p" "$env_path" | tail -n 1)
  [ -n "$value" ] || return 1
  value=${value#\"}
  value=${value%\"}
  value=${value#\'}
  value=${value%\'}
  printf '%s\n' "$value"
}

load_mem0_runtime_config() {
  if [ -z "$MEM0_LLM_API_KEY" ]; then
    MEM0_LLM_API_KEY=$(read_env_file_value TEAMD_PROVIDER_API_KEY "$ENV_FILE" 2>/dev/null || true)
  fi

  if [ -z "$MEM0_ADMIN_API_KEY" ]; then
    MEM0_ADMIN_API_KEY=$(read_env_file_value ADMIN_API_KEY "$MEM0_ENV_FILE" 2>/dev/null || true)
  fi
  if [ -z "$MEM0_ADMIN_API_KEY" ]; then
    MEM0_ADMIN_API_KEY=$(generate_secret_key)
  fi
  MEM0_API_KEY=$MEM0_ADMIN_API_KEY

  if [ -z "$MEM0_JWT_SECRET" ]; then
    MEM0_JWT_SECRET=$(read_env_file_value JWT_SECRET "$MEM0_ENV_FILE" 2>/dev/null || true)
  fi
  if [ -z "$MEM0_JWT_SECRET" ]; then
    MEM0_JWT_SECRET=$(generate_secret_key)
  fi

  if [ -z "$MEM0_POSTGRES_PASSWORD" ]; then
    MEM0_POSTGRES_PASSWORD=$(read_env_file_value POSTGRES_PASSWORD "$MEM0_ENV_FILE" 2>/dev/null || true)
  fi
  if [ -z "$MEM0_POSTGRES_PASSWORD" ]; then
    MEM0_POSTGRES_PASSWORD=$(generate_secret_key)
  fi

  if [ -z "$MEM0_LLM_API_KEY" ]; then
    fail "TEAMD_MEM0_LLM_API_KEY is unset and TEAMD_PROVIDER_API_KEY was not found in $ENV_FILE"
  fi
}

patch_mem0_server_source() {
  run_root python3 - "$MEM0_SRC_DIR" <<'PY'
from pathlib import Path
import sys

src = Path(sys.argv[1])
server = src / "server"

requirements = server / "requirements.txt"
s = requirements.read_text()
s = s.replace("psycopg>=3.2.8", "psycopg[binary]>=3.2.8")
if "fastembed" not in s:
    s += "\n# Local embeddings for TeamD Mem0 deployment\nfastembed>=0.3.1\n"
requirements.write_text(s)

main = server / "main.py"
s = main.read_text()
s = s.replace(
    'BUNDLED_EMBEDDER_PROVIDERS = ("openai", "gemini")',
    'BUNDLED_EMBEDDER_PROVIDERS = ("openai", "gemini", "fastembed")',
)
old_search = '''@app.post("/search", summary="Search memories")
def search_memories(search_req: SearchRequest, _auth=Depends(verify_auth)):
    """Search for memories based on a query."""
    try:
        params = {k: v for k, v in search_req.model_dump().items() if v is not None and k != "query"}
        return get_memory_instance().search(query=search_req.query, **params)
    except Exception:
        raise upstream_error()
'''
new_search = '''@app.post("/search", summary="Search memories")
def search_memories(search_req: SearchRequest, _auth=Depends(verify_auth)):
    """Search for memories based on a query."""
    try:
        payload = search_req.model_dump()
        filters = dict(payload.get("filters") or {})
        for key in ("user_id", "agent_id", "app_id", "run_id"):
            if payload.get(key) is not None:
                filters[key] = payload[key]
        params = {k: payload[k] for k in ("top_k", "threshold") if payload.get(k) is not None}
        if filters:
            params["filters"] = filters
        return get_memory_instance().search(query=search_req.query, **params)
    except Exception:
        raise upstream_error()
'''
if old_search in s:
    s = s.replace(old_search, new_search)
main.write_text(s)

pgvector = src / "mem0" / "vector_stores" / "pgvector.py"
s = pgvector.read_text()
old_pgvector_score = "        return [OutputData(id=str(r[0]), score=float(r[1]), payload=r[2]) for r in results]\n"
new_pgvector_score = """        # pgvector's <=> operator returns cosine distance, where lower is better.
        # Mem0's ranking layer treats OutputData.score as similarity, where higher
        # is better, so expose 1 - distance for semantic vector search.
        return [
            OutputData(id=str(r[0]), score=max(0.0, min(1.0, 1.0 - float(r[1]))), payload=r[2])
            for r in results
        ]
"""
if old_pgvector_score in s:
    s = s.replace(old_pgvector_score, new_pgvector_score, 1)
elif "1.0 - float(r[1])" not in s:
    raise SystemExit("Mem0 pgvector score patch target was not found")
pgvector.write_text(s)

dockerfile = server / "Dockerfile"
s = dockerfile.read_text()
patch = """RUN python - <<'PATCHPY'\nfrom pathlib import Path\n\nfastembed = Path('/usr/local/lib/python3.12/site-packages/mem0/embeddings/fastembed.py')\ns = fastembed.read_text()\ns = s.replace('        return embeddings[0]\\\\n', '        return embeddings[0].tolist() if hasattr(embeddings[0], \\\\\\"tolist\\\\\\") else embeddings[0]\\\\n')\nfastembed.write_text(s)\n\npgvector = Path('/usr/local/lib/python3.12/site-packages/mem0/vector_stores/pgvector.py')\ns = pgvector.read_text()\nold = '        return [OutputData(id=str(r[0]), score=float(r[1]), payload=r[2]) for r in results]\\\\n'\nnew = '''        # pgvector <=> returns cosine distance, where lower is better.\n        # Mem0 ranking treats OutputData.score as similarity, where higher is better.\n        return [\n            OutputData(id=str(r[0]), score=max(0.0, min(1.0, 1.0 - float(r[1]))), payload=r[2])\n            for r in results\n        ]\n'''\nif old in s:\n    s = s.replace(old, new, 1)\nelif '1.0 - float(r[1])' not in s:\n    raise SystemExit('Mem0 site-package pgvector score patch target was not found')\npgvector.write_text(s)\nPATCHPY\n"""
if "embeddings[0].tolist()" not in s or "1.0 - float(r[1])" not in s:
    s = s.replace("RUN pip install --no-cache-dir -r requirements.txt\n", "RUN pip install --no-cache-dir -r requirements.txt\n\n" + patch)
dockerfile.write_text(s)
PY
}

checkout_mem0_source() {
  need_command git
  need_command python3

  if [ -d "$MEM0_SRC_DIR/.git" ]; then
    run_root git -C "$MEM0_SRC_DIR" fetch --all --tags
    run_root git -C "$MEM0_SRC_DIR" checkout "$MEM0_REF"
    run_root git -C "$MEM0_SRC_DIR" pull --ff-only || true
  else
    run_root rm -rf "$MEM0_SRC_DIR"
    run_root git clone "$MEM0_REPOSITORY" "$MEM0_SRC_DIR"
    run_root git -C "$MEM0_SRC_DIR" checkout "$MEM0_REF"
  fi

  patch_mem0_server_source
}

write_mem0_files() {
  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd mkdir -p "$MEM0_DIR" "$MEM0_HISTORY_DIR" "$MEM0_POSTGRES_DATA_DIR" "$MEM0_CACHE_DIR"
    print_cmd chown -R "$MEM0_POSTGRES_UID:$MEM0_POSTGRES_GID" "$MEM0_POSTGRES_DATA_DIR"
    print_cmd git clone "$MEM0_REPOSITORY" "$MEM0_SRC_DIR"
    print_cmd sh -c "patch Mem0 server for bundled fastembed, psycopg[binary], pgvector score normalization, ndarray adaptation, and search filters"
    print_cmd sh -c "seed Mem0 ADMIN_API_KEY, JWT_SECRET and POSTGRES_PASSWORD in $MEM0_ENV_FILE"
    print_cmd sh -c "write $MEM0_COMPOSE for teamd-mem0 and teamd-mem0-postgres"
    print_cmd sh -c "configure Mem0 server at $MEM0_API_BASE/configure with fastembed model $MEM0_FASTEMBED_MODEL and LLM $MEM0_LLM_MODEL"
    return 0
  fi

  load_mem0_runtime_config
  checkout_mem0_source

  run_root mkdir -p "$MEM0_DIR" "$MEM0_HISTORY_DIR" "$MEM0_POSTGRES_DATA_DIR" "$MEM0_CACHE_DIR"
  run_root chown -R "$MEM0_POSTGRES_UID:$MEM0_POSTGRES_GID" "$MEM0_POSTGRES_DATA_DIR"

  tmp_env=$(mktemp)
  tmp_compose=$(mktemp)
  tmp_init=$(mktemp)
  trap 'rm -f "$tmp_env" "$tmp_compose" "$tmp_init"' EXIT INT TERM

  cat > "$tmp_env" <<EOF
OPENAI_API_KEY=$MEM0_LLM_API_KEY
OPENAI_BASE_URL=$MEM0_LLM_API_BASE
POSTGRES_HOST=teamd-mem0-postgres
POSTGRES_PORT=5432
POSTGRES_DB=$MEM0_POSTGRES_DB
POSTGRES_USER=$MEM0_POSTGRES_USER
POSTGRES_PASSWORD=$MEM0_POSTGRES_PASSWORD
POSTGRES_COLLECTION_NAME=$MEM0_COLLECTION_NAME
ADMIN_API_KEY=$MEM0_ADMIN_API_KEY
JWT_SECRET=$MEM0_JWT_SECRET
AUTH_DISABLED=false
DASHBOARD_URL=http://localhost:3000
APP_DB_NAME=$MEM0_APP_DB_NAME
MEM0_DEFAULT_LLM_MODEL=$MEM0_LLM_MODEL
MEM0_DEFAULT_EMBEDDER_MODEL=$MEM0_FASTEMBED_MODEL
MEM0_TELEMETRY=false
REQUEST_LOG_RETENTION_DAYS=30
PYTHONUNBUFFERED=1
EOF

  cat > "$tmp_init" <<EOF
#!/bin/sh
set -e
if ! psql -U "\$POSTGRES_USER" -d "\$POSTGRES_DB" -tAc "SELECT 1 FROM pg_database WHERE datname = '$MEM0_APP_DB_NAME';" | grep -q 1; then
  createdb -U "\$POSTGRES_USER" "$MEM0_APP_DB_NAME"
fi
EOF

  cat > "$tmp_compose" <<EOF
services:
  mem0:
    build:
      context: $MEM0_SRC_DIR/server
      dockerfile: Dockerfile
    image: $MEM0_IMAGE
    container_name: teamd-mem0
    restart: unless-stopped
    env_file:
      - $MEM0_ENV_FILE
    ports:
      - "127.0.0.1:$MEM0_PORT:8000"
    networks:
      - $EDGE_NETWORK
    volumes:
      - $MEM0_HISTORY_DIR:/app/history
      - $MEM0_CACHE_DIR:/root/.cache
    depends_on:
      postgres:
        condition: service_healthy
    command: >
      sh -lc "alembic upgrade head && uvicorn main:app --host 0.0.0.0 --port 8000"

  postgres:
    image: $MEM0_POSTGRES_IMAGE
    container_name: teamd-mem0-postgres
    restart: unless-stopped
    env_file:
      - $MEM0_ENV_FILE
    volumes:
      - $MEM0_POSTGRES_DATA_DIR:/var/lib/postgresql/data
      - $MEM0_INIT_DB:/docker-entrypoint-initdb.d/init-db.sh:ro
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U $MEM0_POSTGRES_USER -d $MEM0_POSTGRES_DB"]
      interval: 10s
      timeout: 5s
      retries: 5
    networks:
      - $EDGE_NETWORK

networks:
  $EDGE_NETWORK:
    external: true
EOF

  run_root install -m 0640 -o root -g root "$tmp_env" "$MEM0_ENV_FILE"
  run_root install -m 0644 -o root -g root "$tmp_compose" "$MEM0_COMPOSE"
  run_root install -m 0644 -o root -g root "$tmp_init" "$MEM0_INIT_DB"
  rm -f "$tmp_env" "$tmp_compose" "$tmp_init"
}

configure_mem0_server() {
  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd sh -c "POST $MEM0_API_BASE/configure for Mem0 pgvector + fastembed + OpenAI-compatible LLM"
    return 0
  fi
  if [ "$SKIP_START" -eq 1 ]; then
    printf 'Skipping Mem0 server configure because --no-start was set.\n'
    return 0
  fi

  run_root python3 - "$MEM0_API_BASE" "$MEM0_ADMIN_API_KEY" "$MEM0_LLM_API_KEY" "$MEM0_POSTGRES_PASSWORD" "$MEM0_LLM_API_BASE" "$MEM0_LLM_MODEL" "$MEM0_LLM_TEMPERATURE" "$MEM0_LLM_MAX_TOKENS" "$MEM0_COLLECTION_NAME" "$MEM0_EMBEDDING_DIMS" "$MEM0_FASTEMBED_MODEL" <<'PY'
import json
import sys
import time
import urllib.request

(
    api_base,
    admin_key,
    llm_api_key,
    pg_password,
    llm_api_base,
    llm_model,
    llm_temperature,
    llm_max_tokens,
    collection_name,
    embedding_dims,
    fastembed_model,
) = sys.argv[1:12]


def request(path, method="GET", body=None, timeout=10):
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(
        api_base.rstrip("/") + path,
        data=data,
        method=method,
        headers={"Content-Type": "application/json", "X-API-Key": admin_key},
    )
    with urllib.request.urlopen(req, timeout=timeout) as response:
        return response.read().decode()


deadline = time.time() + 120
last_error = None
while time.time() < deadline:
    try:
        request("/configure/providers")
        break
    except Exception as error:
        last_error = error
        time.sleep(2)
else:
    raise SystemExit(f"Mem0 API did not become ready: {last_error}")

config = {
    "version": "v1.1",
    "vector_store": {
        "provider": "pgvector",
        "config": {
            "host": "teamd-mem0-postgres",
            "port": 5432,
            "dbname": "postgres",
            "user": "postgres",
            "password": pg_password,
            "collection_name": collection_name,
            "embedding_model_dims": int(embedding_dims),
            "hnsw": True,
        },
    },
    "llm": {
        "provider": "openai",
        "config": {
            "api_key": llm_api_key,
            "openai_base_url": llm_api_base,
            "model": llm_model,
            "temperature": float(llm_temperature),
            "max_tokens": int(llm_max_tokens),
        },
    },
    "embedder": {
        "provider": "fastembed",
        "config": {
            "model": fastembed_model,
        },
    },
    "history_db_path": "/app/history/history.db",
}
request("/configure", method="POST", body=config, timeout=180)
print("Mem0 server configured.")
PY
}

configure_agentd_mem0_env() {
  env_parent=$(dirname "$ENV_FILE")

  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd mkdir -p "$env_parent"
    print_cmd sh -c "upsert Mem0 semantic memory, memory curator, and memory recall defaults in $ENV_FILE"
    return 0
  fi

  run_root mkdir -p "$env_parent"

  tmp_env=$(mktemp)
  tmp_new=$(mktemp)
  if [ -e "$ENV_FILE" ]; then
    awk '
      !/^(export[[:space:]]+)?TEAMD_MEM0_ENABLED=/ &&
      !/^(export[[:space:]]+)?TEAMD_MEM0_API_BASE=/ &&
      !/^(export[[:space:]]+)?TEAMD_MEM0_API_KEY=/ &&
      !/^(export[[:space:]]+)?TEAMD_MEM0_DEFAULT_USER_ID=/ &&
      !/^(export[[:space:]]+)?TEAMD_MEM0_REQUEST_TIMEOUT_MS=/ &&
      !/^(export[[:space:]]+)?TEAMD_MEM0_DEFAULT_LIMIT=/ &&
      !/^(export[[:space:]]+)?TEAMD_MEM0_MAX_LIMIT=/ &&
      !/^(export[[:space:]]+)?TEAMD_MEMORY_CURATOR_ENABLED=/ &&
      !/^(export[[:space:]]+)?TEAMD_MEMORY_CURATOR_MODE=/ &&
      !/^(export[[:space:]]+)?TEAMD_MEMORY_CURATOR_MIN_CONFIDENCE=/ &&
      !/^(export[[:space:]]+)?TEAMD_MEMORY_CURATOR_MAX_CANDIDATES=/ &&
      !/^(export[[:space:]]+)?TEAMD_MEMORY_CURATOR_MAX_OUTPUT_TOKENS=/ &&
      !/^(export[[:space:]]+)?TEAMD_MEMORY_RECALL_ENABLED=/ &&
      !/^(export[[:space:]]+)?TEAMD_MEMORY_RECALL_SCOPES=/ &&
      !/^(export[[:space:]]+)?TEAMD_MEMORY_RECALL_MAX_RESULTS=/ &&
      !/^(export[[:space:]]+)?TEAMD_MEMORY_RECALL_MAX_QUERY_CHARS=/ &&
      !/^(export[[:space:]]+)?TEAMD_MEMORY_RECALL_MAX_MEMORY_CHARS=/
    ' "$ENV_FILE" > "$tmp_env"
  else
    : > "$tmp_env"
  fi

  {
    cat "$tmp_env"
    [ ! -s "$tmp_env" ] || printf '\n'
    printf 'TEAMD_MEM0_ENABLED=%s\n' "$(quote_arg "true")"
    printf 'TEAMD_MEM0_API_BASE=%s\n' "$(quote_arg "$MEM0_API_BASE")"
    if [ -n "$MEM0_API_KEY" ]; then
      printf 'TEAMD_MEM0_API_KEY=%s\n' "$(quote_arg "$MEM0_API_KEY")"
    fi
    printf 'TEAMD_MEM0_DEFAULT_USER_ID=%s\n' "$(quote_arg "$MEM0_DEFAULT_USER_ID")"
    printf 'TEAMD_MEM0_REQUEST_TIMEOUT_MS=%s\n' "$(quote_arg "$MEM0_REQUEST_TIMEOUT_MS")"
    printf 'TEAMD_MEM0_DEFAULT_LIMIT=%s\n' "$(quote_arg "$MEM0_DEFAULT_LIMIT")"
    printf 'TEAMD_MEM0_MAX_LIMIT=%s\n' "$(quote_arg "$MEM0_MAX_LIMIT")"
    printf 'TEAMD_MEMORY_CURATOR_ENABLED=%s\n' "$(quote_arg "$MEMORY_CURATOR_ENABLED")"
    printf 'TEAMD_MEMORY_CURATOR_MODE=%s\n' "$(quote_arg "$MEMORY_CURATOR_MODE")"
    printf 'TEAMD_MEMORY_CURATOR_MIN_CONFIDENCE=%s\n' "$(quote_arg "$MEMORY_CURATOR_MIN_CONFIDENCE")"
    printf 'TEAMD_MEMORY_CURATOR_MAX_CANDIDATES=%s\n' "$(quote_arg "$MEMORY_CURATOR_MAX_CANDIDATES")"
    printf 'TEAMD_MEMORY_CURATOR_MAX_OUTPUT_TOKENS=%s\n' "$(quote_arg "$MEMORY_CURATOR_MAX_OUTPUT_TOKENS")"
    printf 'TEAMD_MEMORY_RECALL_ENABLED=%s\n' "$(quote_arg "$MEMORY_RECALL_ENABLED")"
    printf 'TEAMD_MEMORY_RECALL_SCOPES=%s\n' "$(quote_arg "$MEMORY_RECALL_SCOPES")"
    printf 'TEAMD_MEMORY_RECALL_MAX_RESULTS=%s\n' "$(quote_arg "$MEMORY_RECALL_MAX_RESULTS")"
    printf 'TEAMD_MEMORY_RECALL_MAX_QUERY_CHARS=%s\n' "$(quote_arg "$MEMORY_RECALL_MAX_QUERY_CHARS")"
    printf 'TEAMD_MEMORY_RECALL_MAX_MEMORY_CHARS=%s\n' "$(quote_arg "$MEMORY_RECALL_MAX_MEMORY_CHARS")"
  } > "$tmp_new"

  env_group=root
  if getent group "$SERVICE_GROUP" >/dev/null 2>&1; then
    env_group=$SERVICE_GROUP
  fi
  run_root install -m 0640 -o root -g "$env_group" "$tmp_new" "$ENV_FILE"
  rm -f "$tmp_env" "$tmp_new"
}

load_browserless_token() {
  if [ -n "$BROWSERLESS_TOKEN" ]; then
    return 0
  fi

  if [ -r "$BROWSERLESS_ENV_FILE" ]; then
    BROWSERLESS_TOKEN=$(
      awk -F= '/^TOKEN=/ { print $2; exit }' "$BROWSERLESS_ENV_FILE" | sed "s/^'//; s/'$//; s/^\"//; s/\"$//"
    )
  fi

  if [ -z "$BROWSERLESS_TOKEN" ]; then
    BROWSERLESS_TOKEN=$(generate_secret_key)
  fi
}

write_browserless_files() {
  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd mkdir -p "$BROWSERLESS_DIR"
    print_cmd sh -c "seed Browserless TOKEN in $BROWSERLESS_ENV_FILE"
    print_cmd sh -c "write $BROWSERLESS_COMPOSE for teamd-browserless"
    return 0
  fi

  load_browserless_token
  run_root mkdir -p "$BROWSERLESS_DIR"
  tmp_env=$(mktemp)
  tmp_compose=$(mktemp)
  trap 'rm -f "$tmp_env" "$tmp_compose"' EXIT INT TERM

  cat > "$tmp_env" <<EOF
TOKEN=$BROWSERLESS_TOKEN
EOF

  cat > "$tmp_compose" <<EOF
services:
  browserless:
    image: $BROWSERLESS_IMAGE
    container_name: teamd-browserless
    restart: unless-stopped
    shm_size: "1gb"
    ports:
      - "127.0.0.1:$BROWSERLESS_PORT:$BROWSERLESS_CONTAINER_PORT"
    networks:
      - $EDGE_NETWORK
    env_file:
      - "$BROWSERLESS_ENV_FILE"
    environment:
      - CONCURRENT=$BROWSERLESS_CONCURRENT

networks:
  $EDGE_NETWORK:
    external: true
EOF

  run_root install -m 0640 -o root -g root "$tmp_env" "$BROWSERLESS_ENV_FILE"
  run_root install -m 0644 -o root -g root "$tmp_compose" "$BROWSERLESS_COMPOSE"
  rm -f "$tmp_env" "$tmp_compose"
}

install_agent_browser_cli() {
  bin_parent=$(dirname "$AGENT_BROWSER_BIN")
  link_parent=$(dirname "$AGENT_BROWSER_PATH_LINK")

  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd mkdir -p "$AGENT_BROWSER_INSTALL_DIR" "$bin_parent" "$link_parent"
    print_cmd npm install -g --prefix "$AGENT_BROWSER_INSTALL_DIR" "$AGENT_BROWSER_NPM_PACKAGE"
    print_cmd sh -c "write $AGENT_BROWSER_BIN wrapper for agent-browser"
    print_cmd ln -sf "$AGENT_BROWSER_BIN" "$AGENT_BROWSER_PATH_LINK"
    return 0
  fi

  ensure_npm
  run_root mkdir -p "$AGENT_BROWSER_INSTALL_DIR" "$bin_parent" "$link_parent"
  run_root npm install -g --prefix "$AGENT_BROWSER_INSTALL_DIR" "$AGENT_BROWSER_NPM_PACKAGE"

  tmp_wrapper=$(mktemp)
  trap 'rm -f "$tmp_wrapper"' EXIT INT TERM
  cat > "$tmp_wrapper" <<EOF
#!/bin/sh
set -eu
exec "$AGENT_BROWSER_INSTALL_DIR/bin/agent-browser" "\$@"
EOF

  run_root install -m 0755 -o root -g root "$tmp_wrapper" "$AGENT_BROWSER_BIN"
  run_root ln -sf "$AGENT_BROWSER_BIN" "$AGENT_BROWSER_PATH_LINK"
  "$AGENT_BROWSER_BIN" --help >/dev/null
}

configure_agentd_browser_env() {
  env_parent=$(dirname "$ENV_FILE")
  [ "$DRY_RUN" -eq 1 ] || load_browserless_token

  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd mkdir -p "$env_parent"
    print_cmd sh -c "upsert agent-browser Browserless defaults in $ENV_FILE"
    return 0
  fi

  run_root mkdir -p "$env_parent"

  tmp_env=$(mktemp)
  tmp_new=$(mktemp)
  if [ -e "$ENV_FILE" ]; then
    awk '
      !/^(export[[:space:]]+)?TEAMD_BROWSER_ENABLED=/ &&
      !/^(export[[:space:]]+)?TEAMD_BROWSER_COMMAND=/ &&
      !/^(export[[:space:]]+)?TEAMD_BROWSER_PROVIDER=/ &&
      !/^(export[[:space:]]+)?TEAMD_BROWSER_SESSION_PREFIX=/ &&
      !/^(export[[:space:]]+)?TEAMD_BROWSER_DEFAULT_TIMEOUT_MS=/ &&
      !/^(export[[:space:]]+)?TEAMD_BROWSER_MAX_OUTPUT_CHARS=/ &&
      !/^(export[[:space:]]+)?TEAMD_BROWSERLESS_API_URL=/ &&
      !/^(export[[:space:]]+)?TEAMD_BROWSERLESS_CDP_URL=/ &&
      !/^(export[[:space:]]+)?TEAMD_BROWSERLESS_API_KEY=/ &&
      !/^(export[[:space:]]+)?TEAMD_BROWSERLESS_BROWSER_TYPE=/ &&
      !/^(export[[:space:]]+)?TEAMD_BROWSERLESS_TTL_MS=/ &&
      !/^(export[[:space:]]+)?TEAMD_BROWSERLESS_STEALTH=/
    ' "$ENV_FILE" > "$tmp_env"
  else
    : > "$tmp_env"
  fi

  {
    cat "$tmp_env"
    [ ! -s "$tmp_env" ] || printf '\n'
    printf 'TEAMD_BROWSER_ENABLED=%s\n' "$(quote_arg "true")"
    printf 'TEAMD_BROWSER_COMMAND=%s\n' "$(quote_arg "$AGENT_BROWSER_BIN")"
    printf 'TEAMD_BROWSER_PROVIDER=%s\n' "$(quote_arg "cdp")"
    printf 'TEAMD_BROWSER_SESSION_PREFIX=%s\n' "$(quote_arg "$AGENT_BROWSER_SESSION_PREFIX")"
    printf 'TEAMD_BROWSER_DEFAULT_TIMEOUT_MS=%s\n' "$(quote_arg "$AGENT_BROWSER_DEFAULT_TIMEOUT_MS")"
    printf 'TEAMD_BROWSER_MAX_OUTPUT_CHARS=%s\n' "$(quote_arg "$AGENT_BROWSER_MAX_OUTPUT_CHARS")"
    printf 'TEAMD_BROWSERLESS_API_URL=%s\n' "$(quote_arg "$BROWSERLESS_API_URL")"
    printf 'TEAMD_BROWSERLESS_CDP_URL=%s\n' "$(quote_arg "$BROWSERLESS_CDP_URL?token=$BROWSERLESS_TOKEN")"
    printf 'TEAMD_BROWSERLESS_API_KEY=%s\n' "$(quote_arg "$BROWSERLESS_TOKEN")"
    printf 'TEAMD_BROWSERLESS_BROWSER_TYPE=%s\n' "$(quote_arg "$BROWSERLESS_BROWSER_TYPE")"
    printf 'TEAMD_BROWSERLESS_TTL_MS=%s\n' "$(quote_arg "$BROWSERLESS_TTL_MS")"
    printf 'TEAMD_BROWSERLESS_STEALTH=%s\n' "$(quote_arg "$BROWSERLESS_STEALTH")"
  } > "$tmp_new"

  env_group=root
  if getent group "$SERVICE_GROUP" >/dev/null 2>&1; then
    env_group=$SERVICE_GROUP
  fi
  run_root install -m 0640 -o root -g "$env_group" "$tmp_new" "$ENV_FILE"
  rm -f "$tmp_env" "$tmp_new"
}

write_silverbullet_mcp_example() {
  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd mkdir -p "$SILVERBULLET_DIR"
    print_cmd sh -c "write $SILVERBULLET_MCP_EXAMPLE for SilverBullet MCP stdio bridge"
    return 0
  fi

  run_root mkdir -p "$SILVERBULLET_DIR"
  tmp_config=$(mktemp)
  trap 'rm -f "$tmp_config"' EXIT INT TERM

  cat > "$tmp_config" <<EOF
# Copy this block into /etc/teamd/config.toml under [daemon.mcp_connectors].
#
# agentd -> stdio wrapper -> docker run node -> mcp-remote -> local SilverBullet MCP HTTP service.
# Secrets stay in $SILVERBULLET_ENV_FILE, not in config.toml.

[daemon.mcp_connectors.silverbullet]
transport = "stdio"
command = "$SILVERBULLET_MCP_STDIO_WRAPPER"
args = []
enabled = false
EOF

  run_root install -m 0644 -o root -g root "$tmp_config" "$SILVERBULLET_MCP_EXAMPLE"
}

append_silverbullet_mcp_connector_config() {
  config_parent=$(dirname "$CONFIG_FILE")

  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd mkdir -p "$config_parent"
    print_cmd sh -c "upsert enabled SilverBullet MCP connector in $CONFIG_FILE"
    return 0
  fi

  run_root mkdir -p "$config_parent"

  tmp_block=$(mktemp)
  tmp_config=$(mktemp)
  trap 'rm -f "$tmp_block" "$tmp_config"' EXIT INT TERM
  cat > "$tmp_block" <<EOF
[daemon.mcp_connectors.silverbullet]
transport = "stdio"
command = "$SILVERBULLET_MCP_STDIO_WRAPPER"
args = []
enabled = true
EOF

  if [ -e "$CONFIG_FILE" ]; then
    if grep -F "[daemon.mcp_connectors.silverbullet]" "$CONFIG_FILE" >/dev/null 2>&1; then
      awk -v block="$tmp_block" '
        function print_block() {
          while ((getline line < block) > 0) print line
          close(block)
        }
        /^\[daemon\.mcp_connectors\.silverbullet\][[:space:]]*$/ {
          print_block()
          in_silverbullet = 1
          next
        }
        in_silverbullet && /^\[/ {
          in_silverbullet = 0
        }
        !in_silverbullet {
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

resolve_service_ids() {
  if id -u "$SERVICE_USER" >/dev/null 2>&1; then
    SERVICE_UID=$(id -u "$SERVICE_USER")
    SERVICE_GID=$(id -g "$SERVICE_USER")
  else
    SERVICE_UID=$(id -u)
    SERVICE_GID=$(id -g)
  fi
}

seed_silverbullet_credentials() {
  env_parent=$(dirname "$SILVERBULLET_ENV_FILE")

  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd mkdir -p "$env_parent"
    print_cmd sh -c "seed SilverBullet credentials and API/MCP tokens at $SILVERBULLET_ENV_FILE"
    return 0
  fi

  run_root mkdir -p "$env_parent"

  env_group=root
  if getent group "$SERVICE_GROUP" >/dev/null 2>&1; then
    env_group=$SERVICE_GROUP
  fi

  tmp_env=$(mktemp)
  if [ -e "$SILVERBULLET_ENV_FILE" ]; then
    cat "$SILVERBULLET_ENV_FILE" > "$tmp_env"
  else
    : > "$tmp_env"
  fi

  if [ -n "$SILVERBULLET_USER" ]; then
    case "$SILVERBULLET_USER" in
      *'
'*) fail "TEAMD_SILVERBULLET_USER must be a single line username:password value" ;;
    esac
    awk '!/^SB_USER=/' "$tmp_env" > "$tmp_env.next"
    mv "$tmp_env.next" "$tmp_env"
    printf 'SB_USER=%s\n' "$SILVERBULLET_USER" >> "$tmp_env"
  fi

  if ! grep -q '^SB_USER=' "$tmp_env"; then
    generated_password=$(generate_secret_key | cut -c 1-24)
    printf 'SB_USER=admin:%s\n' "$generated_password" >> "$tmp_env"
  fi

  if ! grep -q '^SB_AUTH_TOKEN=' "$tmp_env"; then
    printf 'SB_AUTH_TOKEN=%s\n' "$(generate_secret_key)" >> "$tmp_env"
  fi

  if ! grep -q '^MCP_TOKEN=' "$tmp_env"; then
    printf 'MCP_TOKEN=%s\n' "$(generate_secret_key)" >> "$tmp_env"
  fi

  run_root install -m 0640 -o root -g "$env_group" "$tmp_env" "$SILVERBULLET_ENV_FILE"
  rm -f "$tmp_env"
}

seed_silverbullet_space() {
  welcome_file=$SILVERBULLET_SPACE_DIR/teamD.md

  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd sh -c "write managed SilverBullet Space welcome note when missing"
    return 0
  fi

  if [ ! -e "$welcome_file" ]; then
    tmp_welcome=$(mktemp)
    cat > "$tmp_welcome" <<EOF
---
type: resource
status: active
tags: [teamd, silverbullet]
---

# teamD SilverBullet Space

Этот Space создан deploy script'ом teamD.

- SilverBullet является основным browser UI для заметок.
- agentd может работать с этим Space через SilverBullet MCP connector и штатные filesystem tools.
EOF
    run_root install -m 0644 -o "$SERVICE_UID" -g "$SERVICE_GID" "$tmp_welcome" "$welcome_file"
    rm -f "$tmp_welcome"
  fi
}

write_silverbullet_files() {
  resolve_service_ids
  local silverbullet_url_prefix
  local silverbullet_api_base_url
  local silverbullet_url_prefix_env=
  silverbullet_url_prefix=$(silverbullet_effective_url_prefix)
  silverbullet_api_base_url="http://silverbullet:$SILVERBULLET_CONTAINER_PORT$silverbullet_url_prefix"
  if [ -n "$silverbullet_url_prefix" ]; then
    silverbullet_url_prefix_env="
      - SB_URL_PREFIX=$silverbullet_url_prefix"
  fi

  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd mkdir -p "$SILVERBULLET_DIR" "$SILVERBULLET_SPACE_DIR"
    print_cmd chown -R "$SERVICE_UID:$SERVICE_GID" "$SILVERBULLET_SPACE_DIR"
    print_cmd sh -c "write $SILVERBULLET_COMPOSE for teamd-silverbullet"
    if [ -n "$silverbullet_url_prefix" ]; then
      print_cmd sh -c "set SilverBullet SB_URL_PREFIX=$silverbullet_url_prefix"
    fi
    if [ "$ENABLE_SILVERBULLET_MCP" -eq 1 ]; then
      print_cmd sh -c "include teamd-silverbullet-mcp service built from $SILVERBULLET_MCP_REPOSITORY#$SILVERBULLET_MCP_REF"
      print_cmd sh -c "write $SILVERBULLET_MCP_STDIO_WRAPPER for agentd stdio MCP bridge"
    fi
    seed_silverbullet_space
    seed_silverbullet_credentials
    return 0
  fi

  run_root mkdir -p "$SILVERBULLET_DIR" "$SILVERBULLET_SPACE_DIR"
  run_root chown -R "$SERVICE_UID:$SERVICE_GID" "$SILVERBULLET_SPACE_DIR"
  seed_silverbullet_space
  seed_silverbullet_credentials

  tmp_compose=$(mktemp)
  tmp_wrapper=$(mktemp)
  trap 'rm -f "$tmp_compose" "$tmp_wrapper"' EXIT INT TERM

  mcp_service_block=
  if [ "$ENABLE_SILVERBULLET_MCP" -eq 1 ]; then
    mcp_service_block="
  silverbullet-mcp:
    build:
      context: $SILVERBULLET_MCP_REPOSITORY#$SILVERBULLET_MCP_REF
    container_name: teamd-silverbullet-mcp
    restart: unless-stopped
    env_file:
      - \"$SILVERBULLET_ENV_FILE\"
    ports:
      - \"127.0.0.1:$SILVERBULLET_MCP_PORT:$SILVERBULLET_MCP_CONTAINER_PORT\"
    networks:
      - $EDGE_NETWORK
    environment:
      - SB_API_BASE_URL=$silverbullet_api_base_url
      - PORT=$SILVERBULLET_MCP_CONTAINER_PORT
      - DEBUG_REQUESTS=false
    depends_on:
      - silverbullet"
  fi

  cat > "$tmp_compose" <<EOF
services:
  silverbullet:
    image: $SILVERBULLET_IMAGE
    container_name: teamd-silverbullet
    restart: unless-stopped
    env_file:
      - "$SILVERBULLET_ENV_FILE"
    ports:
      - "127.0.0.1:$SILVERBULLET_PORT:$SILVERBULLET_CONTAINER_PORT"
    networks:
      - $EDGE_NETWORK
    volumes:
      - "$SILVERBULLET_SPACE_DIR:/space:rw"
    environment:
      - SB_FOLDER=/space
$silverbullet_url_prefix_env
$mcp_service_block

networks:
  $EDGE_NETWORK:
    external: true
EOF

  run_root install -m 0644 -o root -g root "$tmp_compose" "$SILVERBULLET_COMPOSE"

  if [ "$ENABLE_SILVERBULLET_MCP" -eq 1 ]; then
    wrapper_group=root
    if getent group "$SERVICE_GROUP" >/dev/null 2>&1; then
      wrapper_group=$SERVICE_GROUP
    fi
    cat > "$tmp_wrapper" <<EOF
#!/bin/sh
set -eu
. "$SILVERBULLET_ENV_FILE"
: "\${MCP_TOKEN:?MCP_TOKEN is required in $SILVERBULLET_ENV_FILE}"
exec docker run -i --rm --network host "$SILVERBULLET_MCP_NODE_IMAGE" \
  npx -y mcp-remote "http://127.0.0.1:$SILVERBULLET_MCP_PORT/mcp" \
  --transport http-only \
  --header "Authorization:Bearer \${MCP_TOKEN}"
EOF
    run_root install -m 0750 -o root -g "$wrapper_group" "$tmp_wrapper" "$SILVERBULLET_MCP_STDIO_WRAPPER"
  fi
}

resolve_filebrowser_ids() {
  if [ -n "$FILEBROWSER_PUID" ] && [ -n "$FILEBROWSER_PGID" ]; then
    return 0
  fi

  if id "$SERVICE_USER" >/dev/null 2>&1; then
    FILEBROWSER_PUID=${FILEBROWSER_PUID:-$(id -u "$SERVICE_USER")}
    FILEBROWSER_PGID=${FILEBROWSER_PGID:-$(id -g "$SERVICE_USER")}
  else
    FILEBROWSER_PUID=${FILEBROWSER_PUID:-1000}
    FILEBROWSER_PGID=${FILEBROWSER_PGID:-1000}
  fi
}

hash_filebrowser_password() {
  password=$1
  hash=$(run_root docker run --rm --entrypoint filebrowser "$FILEBROWSER_IMAGE" hash "$password")
  [ -n "$hash" ] || fail "File Browser password hash command returned an empty hash"
  printf '%s\n' "$hash"
}

escape_compose_env_dollars() {
  printf '%s' "$1" | sed 's/\$/$$/g'
}

normalize_existing_filebrowser_credentials() {
  [ -f "$FILEBROWSER_ENV_FILE" ] || return 0
  grep -E '^FB_PASSWORD=.*[$]' "$FILEBROWSER_ENV_FILE" >/dev/null 2>&1 || return 0
  if grep -E '^FB_PASSWORD=.*[$][$]' "$FILEBROWSER_ENV_FILE" >/dev/null 2>&1; then
    return 0
  fi

  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd sh -c "escape dollar signs in FB_PASSWORD at $FILEBROWSER_ENV_FILE for Docker Compose"
    return 0
  fi

  tmp_env=$(mktemp)
  trap 'rm -f "$tmp_env"' EXIT INT TERM
  sed '/^FB_PASSWORD=/ s/\$/$$/g' "$FILEBROWSER_ENV_FILE" > "$tmp_env"
  run_root install -m 0600 -o root -g root "$tmp_env" "$FILEBROWSER_ENV_FILE"
}

filebrowser_plaintext_password_from_env() {
  sed -n 's/^# password=//p' "$FILEBROWSER_ENV_FILE" | sed -n '1p'
}

filebrowser_container_running() {
  if run_root docker ps --format '{{.Names}}' | grep -Fx 'teamd-filebrowser' >/dev/null 2>&1; then
    return 0
  fi
  return 1
}

stop_running_filebrowser_for_db_update() {
  filebrowser_container_running || return 0
  run_root docker stop teamd-filebrowser >/dev/null
}

sync_filebrowser_admin_user() {
  [ -f "$FILEBROWSER_ENV_FILE" ] || return 0
  [ -f "$FILEBROWSER_DB_DIR/filebrowser.db" ] || return 0

  password=$(filebrowser_plaintext_password_from_env)
  if [ -z "$password" ]; then
    printf 'File Browser database exists, but %s has no plaintext password comment; skipping admin password reconciliation.\n' "$FILEBROWSER_ENV_FILE" >&2
    return 0
  fi

  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd sh -c "sync File Browser admin user in $FILEBROWSER_DB_DIR/filebrowser.db from $FILEBROWSER_ENV_FILE"
    return 0
  fi

  if [ "$SKIP_START" -eq 1 ] && filebrowser_container_running; then
    printf 'Skipping File Browser admin password reconciliation because --no-start was set and teamd-filebrowser is running.\n' >&2
    return 0
  fi

  stop_running_filebrowser_for_db_update
  if ! run_root docker run --rm --entrypoint filebrowser \
    -v "$FILEBROWSER_DB_DIR:/database:rw" \
    "$FILEBROWSER_IMAGE" \
    -d /database/filebrowser.db \
    users update "$FILEBROWSER_ADMIN_USER" --password "$password" --perm.admin=true >/dev/null; then
    run_root docker run --rm --entrypoint filebrowser \
      -v "$FILEBROWSER_DB_DIR:/database:rw" \
      "$FILEBROWSER_IMAGE" \
      -d /database/filebrowser.db \
      users add "$FILEBROWSER_ADMIN_USER" "$password" --perm.admin=true >/dev/null
  fi
  run_root chown -R "$FILEBROWSER_PUID:$FILEBROWSER_PGID" "$FILEBROWSER_DB_DIR"
}

write_filebrowser_settings() {
  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd sh -c "write File Browser settings.json with baseURL=$FILEBROWSER_BASE_URL"
    return 0
  fi

  tmp_settings=$(mktemp)
  trap 'rm -f "$tmp_settings"' EXIT INT TERM
  cat > "$tmp_settings" <<EOF
{
  "port": $FILEBROWSER_CONTAINER_PORT,
  "baseURL": "$FILEBROWSER_BASE_URL",
  "address": "0.0.0.0",
  "log": "stdout",
  "database": "/database/filebrowser.db",
  "root": "/srv"
}
EOF
  run_root install -m 0644 -o "$FILEBROWSER_PUID" -g "$FILEBROWSER_PGID" "$tmp_settings" "$FILEBROWSER_CONFIG_DIR/settings.json"
}

seed_filebrowser_credentials() {
  if [ -f "$FILEBROWSER_ENV_FILE" ]; then
    # Preserve existing credentials. Operators rotate them by editing this file
    # and recreating the container, or through the File Browser admin UI.
    normalize_existing_filebrowser_credentials
    return 0
  fi

  password=$FILEBROWSER_ADMIN_PASSWORD
  if [ -z "$password" ]; then
    password=$(generate_secret_key | cut -c 1-24)
  fi

  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd sh -c "seed File Browser credentials and config at $FILEBROWSER_ENV_FILE"
    return 0
  fi

  hashed_password=$(hash_filebrowser_password "$password")
  escaped_hashed_password=$(escape_compose_env_dollars "$hashed_password")
  tmp_env=$(mktemp)
  trap 'rm -f "$tmp_env"' EXIT INT TERM
  cat > "$tmp_env" <<EOF
# File Browser admin credentials for operator login.
# username=$FILEBROWSER_ADMIN_USER
# password=$password
FB_ADDRESS=0.0.0.0
FB_PORT=$FILEBROWSER_CONTAINER_PORT
FB_ROOT=/srv
FB_CONFIG=/config/settings.json
FB_DATABASE=/database/filebrowser.db
FB_BASEURL=$FILEBROWSER_BASE_URL
FB_USERNAME=$FILEBROWSER_ADMIN_USER
FB_PASSWORD=$escaped_hashed_password
FB_DISABLE_EXEC=true
PUID=$FILEBROWSER_PUID
PGID=$FILEBROWSER_PGID
EOF
  run_root install -m 0600 -o root -g root "$tmp_env" "$FILEBROWSER_ENV_FILE"
}

write_filebrowser_files() {
  resolve_filebrowser_ids
  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd mkdir -p "$FILEBROWSER_DIR" "$FILEBROWSER_DB_DIR" "$FILEBROWSER_CONFIG_DIR" "$FILEBROWSER_AGENT_HOMES_DIR" "$FILEBROWSER_WORKSPACES_DIR" "$FILEBROWSER_ARTIFACTS_DIR" "$FILEBROWSER_KNOWLEDGE_DIR"
    print_cmd sh -c "write $FILEBROWSER_COMPOSE for teamd-filebrowser"
    return 0
  fi

  run_root mkdir -p \
    "$FILEBROWSER_DIR" \
    "$FILEBROWSER_DB_DIR" \
    "$FILEBROWSER_CONFIG_DIR" \
    "$FILEBROWSER_AGENT_HOMES_DIR" \
    "$FILEBROWSER_WORKSPACES_DIR" \
    "$FILEBROWSER_ARTIFACTS_DIR" \
    "$FILEBROWSER_KNOWLEDGE_DIR"
  seed_filebrowser_credentials
  run_root chown -R "$FILEBROWSER_PUID:$FILEBROWSER_PGID" \
    "$FILEBROWSER_DB_DIR" \
    "$FILEBROWSER_CONFIG_DIR"
  write_filebrowser_settings
  sync_filebrowser_admin_user
  run_root chown "$FILEBROWSER_PUID:$FILEBROWSER_PGID" \
    "$FILEBROWSER_AGENT_HOMES_DIR" \
    "$FILEBROWSER_WORKSPACES_DIR" \
    "$FILEBROWSER_ARTIFACTS_DIR" \
    "$FILEBROWSER_KNOWLEDGE_DIR"

  docs_volume=
  if [ -n "$FILEBROWSER_DOCS_DIR" ]; then
    run_root mkdir -p "$FILEBROWSER_DOCS_DIR"
    run_root chown "$FILEBROWSER_PUID:$FILEBROWSER_PGID" "$FILEBROWSER_DOCS_DIR"
    docs_volume="      - \"$FILEBROWSER_DOCS_DIR:/srv/docs:rw\""
  fi

  tmp_compose=$(mktemp)
  trap 'rm -f "$tmp_compose"' EXIT INT TERM
  cat > "$tmp_compose" <<EOF
services:
  filebrowser:
    image: $FILEBROWSER_IMAGE
    container_name: teamd-filebrowser
    restart: unless-stopped
    env_file:
      - "$FILEBROWSER_ENV_FILE"
    ports:
      - "127.0.0.1:$FILEBROWSER_PORT:$FILEBROWSER_CONTAINER_PORT"
    networks:
      - $EDGE_NETWORK
    volumes:
      - "$FILEBROWSER_DB_DIR:/database:rw"
      - "$FILEBROWSER_CONFIG_DIR:/config:rw"
      - "$FILEBROWSER_AGENT_HOMES_DIR:/srv/agent-homes:rw"
      - "$FILEBROWSER_WORKSPACES_DIR:/srv/workspaces:rw"
      - "$FILEBROWSER_ARTIFACTS_DIR:/srv/artifacts:rw"
      - "$FILEBROWSER_KNOWLEDGE_DIR:/srv/knowledge:rw"
$docs_volume

networks:
  $EDGE_NETWORK:
    external: true
EOF
  run_root install -m 0644 -o root -g root "$tmp_compose" "$FILEBROWSER_COMPOSE"
}

remove_legacy_manual_mcp_runtime() {
  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd sh -c "remove legacy anonymous Node MCP containers using mcp-remote or mcpvault"
    return 0
  fi

  run_root docker ps -a --no-trunc --format '{{.ID}}\t{{.Image}}\t{{.Command}}' |
    while IFS='	' read -r container_id image command; do
      case "$image $command" in
        *node:22-alpine*mcp-remote*|*node:22-alpine*mcpvault*)
          run_root docker rm -f "$container_id" >/dev/null
          ;;
      esac
    done
}

ensure_teamd_docker_access() {
  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd usermod -aG docker "$SERVICE_USER"
    return 0
  fi

  if ! id "$SERVICE_USER" >/dev/null 2>&1; then
    printf 'Warning: service user %s does not exist; cannot grant Docker access for MCP connector wrappers.\n' "$SERVICE_USER" >&2
    return 0
  fi
  if ! getent group docker >/dev/null 2>&1; then
    printf 'Warning: docker group does not exist; cannot grant Docker access for MCP connector wrappers.\n' >&2
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
  if [ "$ENABLE_SILVERBULLET" -eq 1 ] && [ -n "$SILVERBULLET_HTTPS_PORT" ]; then
    ports_block="$ports_block
      - \"$SILVERBULLET_HTTPS_PORT:$SILVERBULLET_HTTPS_PORT\""
  fi

  caddy_volumes="      - \"$CADDYFILE:/etc/caddy/Caddyfile:ro\"
      - \"$CADDY_DATA_DIR:/data:rw\"
      - \"$CADDY_CONFIG_DIR:/config:rw\""

  cat > "$tmp_compose" <<EOF
services:
  caddy:
    image: $CADDY_IMAGE
    container_name: teamd-caddy
    restart: unless-stopped
    ports:
$ports_block
    extra_hosts:
      - "host.docker.internal:host-gateway"
    volumes:
$caddy_volumes
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
    if [ -n "$CADDY_DOMAIN" ] && [ "$CADDY_SINGLE_DOMAIN" -eq 0 ]; then
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

  filebrowser_domain_block=
  filebrowser_http_redirect=
  filebrowser_handle=
  if [ "$ENABLE_FILEBROWSER" -eq 1 ]; then
    if [ -n "$CADDY_DOMAIN" ] && [ "$CADDY_SINGLE_DOMAIN" -eq 0 ]; then
      filebrowser_domain_block="
files.$CADDY_DOMAIN {
  reverse_proxy teamd-filebrowser:$FILEBROWSER_CONTAINER_PORT
}
"
    else
      filebrowser_http_redirect="  redir /files /files/ 308"
      filebrowser_handle="  handle /files/* {
    reverse_proxy teamd-filebrowser:$FILEBROWSER_CONTAINER_PORT
  }"
    fi
  fi

  webhook_handle="  handle /v1/telegram/webhook/* {
    reverse_proxy $CADDY_DAEMON_UPSTREAM
  }"
  core_web_handle="  redir /web /web/ 308

  handle_path /web/* {
    reverse_proxy $CADDY_WEB_UPSTREAM
  }

  handle /api/agentd/* {
    reverse_proxy $CADDY_WEB_UPSTREAM
  }

  handle /api/events {
    reverse_proxy $CADDY_WEB_UPSTREAM
  }

  handle /v1/web/* {
    reverse_proxy $CADDY_DAEMON_UPSTREAM
  }"

	  silverbullet_domain_block=
	  silverbullet_https_block=
	  silverbullet_http_redirect=
	  silverbullet_compat_redirects=
	  silverbullet_single_handle=
	  caddy_global_options=
	  if [ "$ENABLE_SILVERBULLET" -eq 1 ]; then
    if [ -n "$CADDY_DOMAIN" ] && [ "$CADDY_SINGLE_DOMAIN" -eq 0 ]; then
      silverbullet_domain_block="
notes.$CADDY_DOMAIN {
  reverse_proxy teamd-silverbullet:$SILVERBULLET_CONTAINER_PORT
}
"
    elif [ -n "$SILVERBULLET_HTTPS_PORT" ]; then
      silverbullet_https_block="
https://$CADDY_HOST:$SILVERBULLET_HTTPS_PORT {
  tls internal
  reverse_proxy teamd-silverbullet:$SILVERBULLET_CONTAINER_PORT
}
"
	    fi
	    silverbullet_prefix=$(silverbullet_effective_url_prefix)
	    if [ -n "$silverbullet_prefix" ] && [ -n "$CADDY_DOMAIN" ] && [ "$CADDY_SINGLE_DOMAIN" -eq 1 ]; then
	      silverbullet_http_redirect="  redir $silverbullet_prefix $silverbullet_prefix/ 308"
	      silverbullet_compat_redirects="  redir /r $silverbullet_prefix/r 308
  redir /r/* $silverbullet_prefix{uri} 308
  redir /p $silverbullet_prefix/p 308
  redir /p/* $silverbullet_prefix{uri} 308
  redir /a $silverbullet_prefix/a 308
  redir /a/* $silverbullet_prefix{uri} 308
  redir /journals $silverbullet_prefix/journals 308
  redir /journals/* $silverbullet_prefix{uri} 308
  redir /template $silverbullet_prefix/template 308
  redir /template/* $silverbullet_prefix{uri} 308
  redir /Projects $silverbullet_prefix/Projects 308
  redir /Areas $silverbullet_prefix/Areas 308
  redir /Resources $silverbullet_prefix/Resources 308
  redir /Archive $silverbullet_prefix/Archive 308
  redir /00-Inbox $silverbullet_prefix/00-Inbox 308
  redir /05-Journal $silverbullet_prefix/05-Journal 308
  redir /06-Zettelkasten $silverbullet_prefix/06-Zettelkasten 308"
	      silverbullet_single_handle="  handle $silverbullet_prefix/* {
    reverse_proxy teamd-silverbullet:$SILVERBULLET_CONTAINER_PORT
  }"
	    fi
	  fi

	  if [ -n "$CADDY_HTTPS_PORT" ] || { [ "$ENABLE_SILVERBULLET" -eq 1 ] && [ -n "$SILVERBULLET_HTTPS_PORT" ]; }; then
	    caddy_global_options="{
  auto_https disable_redirects
  default_sni $CADDY_HOST
}

"
	  fi

	  single_domain_root_handle='  respond / "teamD container edge: /searxng/, /jaeger/ and optional add-ons"'
	  if [ "$ENABLE_SILVERBULLET" -eq 1 ] && [ -z "$silverbullet_single_handle" ]; then
	    single_domain_root_handle="  handle {
    reverse_proxy teamd-silverbullet:$SILVERBULLET_CONTAINER_PORT
  }"
	  elif [ "$ENABLE_SILVERBULLET" -eq 1 ]; then
	    single_domain_root_handle='  respond / "teamD container edge: /sb/, /files/, /searxng/, /jaeger/"'
	  fi

	  if [ -n "$CADDY_DOMAIN" ]; then
	    if [ "$CADDY_SINGLE_DOMAIN" -eq 1 ]; then
	      cat > "$tmp_caddyfile" <<EOF
$CADDY_DOMAIN {
  redir /searxng /searxng/ 308
$jaeger_http_redirect
$filebrowser_http_redirect
$silverbullet_http_redirect
$silverbullet_compat_redirects

  handle /searxng/* {
    reverse_proxy teamd-searxng:8080 {
      header_up X-Script-Name /searxng
    }
  }
$webhook_handle
$core_web_handle
$jaeger_handle
$filebrowser_handle
$silverbullet_single_handle

$single_domain_root_handle
}
EOF
	    else
	      cat > "$tmp_caddyfile" <<EOF
search.$CADDY_DOMAIN {
  reverse_proxy teamd-searxng:8080
}

$CADDY_DOMAIN {
$webhook_handle
$core_web_handle
  respond / "teamD core edge"
}
$jaeger_domain_block
$silverbullet_domain_block
$filebrowser_domain_block
EOF
	    fi
	  else
	    if [ -n "$CADDY_HTTPS_PORT" ]; then
      cat > "$tmp_caddyfile" <<EOF
$caddy_global_options
:80 {
  redir /searxng /searxng/ 308
$jaeger_http_redirect
$filebrowser_http_redirect

  handle /searxng/* {
    reverse_proxy teamd-searxng:8080 {
      header_up X-Script-Name /searxng
    }
  }
$webhook_handle
$core_web_handle
$jaeger_handle
$filebrowser_handle

  respond / "teamD container edge: /searxng/ on HTTP; optional add-ons when enabled"
}

https://$CADDY_HOST {
  tls internal

  handle /searxng/* {
    reverse_proxy teamd-searxng:8080 {
      header_up X-Script-Name /searxng
    }
  }
$webhook_handle
$core_web_handle
$jaeger_handle
$filebrowser_handle

  respond / "teamD container edge (TLS): /searxng/ and optional add-ons"
}
$silverbullet_https_block
EOF
    else
      cat > "$tmp_caddyfile" <<EOF
$caddy_global_options
:80 {
  redir /searxng /searxng/ 308
$jaeger_http_redirect
$filebrowser_http_redirect

  handle /searxng/* {
    reverse_proxy teamd-searxng:8080 {
      header_up X-Script-Name /searxng
    }
  }
$webhook_handle
$core_web_handle
$jaeger_handle
$filebrowser_handle

  respond / "teamD container edge: /searxng/ and optional add-ons"
}
$silverbullet_https_block
EOF
    fi
  fi

  run_root install -m 0644 -o root -g root "$tmp_compose" "$CADDY_COMPOSE"
  run_root install -m 0644 -o root -g root "$tmp_caddyfile" "$CADDYFILE"
}

compose_up() {
  compose_file=$1
  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd docker compose -f "$compose_file" up -d
    return 0
  fi
  if [ "$SKIP_START" -eq 1 ]; then
    printf 'Skipping container start for %s because --no-start was set.\n' "$compose_file"
    return 0
  fi
  run_root docker compose -f "$compose_file" up -d
}

compose_up_build() {
  compose_file=$1
  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd docker compose -f "$compose_file" up -d --build
    return 0
  fi
  if [ "$SKIP_START" -eq 1 ]; then
    printf 'Skipping container start for %s because --no-start was set.\n' "$compose_file"
    return 0
  fi
  run_root docker compose -f "$compose_file" up -d --build
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

compose_up_filebrowser() {
  if [ "$DRY_RUN" -eq 1 ]; then
    print_cmd docker compose -f "$FILEBROWSER_COMPOSE" up -d --force-recreate
    return 0
  fi
  if [ "$SKIP_START" -eq 1 ]; then
    printf 'Skipping container start for %s because --no-start was set.\n' "$FILEBROWSER_COMPOSE"
    return 0
  fi

  # settings.json is bind-mounted and read on process startup.
  run_root docker compose -f "$FILEBROWSER_COMPOSE" up -d --force-recreate
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
    --no-nats) ENABLE_NATS=0 ;;
    --no-searxng) ENABLE_SEARXNG=0 ;;
    --no-caddy) ENABLE_CADDY=0 ;;
    --with-jaeger) ENABLE_JAEGER=1 ;;
    --with-mem0) ENABLE_MEM0=1 ;;
    --with-silverbullet)
      ENABLE_SILVERBULLET=1
      ;;
    --with-silverbullet-mcp)
      ENABLE_SILVERBULLET=1
      ENABLE_SILVERBULLET_MCP=1
      WRITE_SILVERBULLET_MCP_EXAMPLE=1
      ;;
    --with-silverbullet-mcp-example)
      ENABLE_SILVERBULLET=1
      WRITE_SILVERBULLET_MCP_EXAMPLE=1
      ;;
    --with-browserless)
      ENABLE_BROWSERLESS=1
      INSTALL_AGENT_BROWSER=1
      ;;
    --with-agent-browser)
      INSTALL_AGENT_BROWSER=1
      ;;
    --with-filebrowser)
      ENABLE_FILEBROWSER=1
      ;;
    --single-domain) CADDY_SINGLE_DOMAIN=1 ;;
    --no-restart-teamd) RESTART_TEAMD_SERVICES=0 ;;
    --searxng-port)
      shift
      [ "$#" -gt 0 ] || fail "--searxng-port requires a value"
      valid_port "$1" || fail "invalid --searxng-port: $1"
      SEARXNG_PORT=$1
      ;;
    --jaeger-ui-port)
      shift
      [ "$#" -gt 0 ] || fail "--jaeger-ui-port requires a value"
      valid_port "$1" || fail "invalid --jaeger-ui-port: $1"
      JAEGER_UI_PORT=$1
      ;;
    --silverbullet-port)
      shift
      [ "$#" -gt 0 ] || fail "--silverbullet-port requires a value"
      valid_port "$1" || fail "invalid --silverbullet-port: $1"
      SILVERBULLET_PORT=$1
      ;;
    --silverbullet-https-port)
      shift
      [ "$#" -gt 0 ] || fail "--silverbullet-https-port requires a value"
      valid_port "$1" || fail "invalid --silverbullet-https-port: $1"
      SILVERBULLET_HTTPS_PORT=$1
      ;;
    --filebrowser-port)
      shift
      [ "$#" -gt 0 ] || fail "--filebrowser-port requires a value"
      valid_port "$1" || fail "invalid --filebrowser-port: $1"
      FILEBROWSER_PORT=$1
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
  printf 'DRY RUN: no Docker packages, compose files, data directories, browser binaries or containers will be changed.\n'
fi

need_command id
need_command sed
resolve_filebrowser_base_url
validate_caddy_domain_mode
ensure_silverbullet_https_port
validate_silverbullet_url_prefix
ensure_caddy_host
valid_port "$NATS_PORT" || fail "invalid TEAMD_NATS_PORT: $NATS_PORT"
valid_port "$NATS_MONITOR_PORT" || fail "invalid TEAMD_NATS_MONITOR_PORT: $NATS_MONITOR_PORT"
valid_port "$MEM0_PORT" || fail "invalid TEAMD_MEM0_PORT: $MEM0_PORT"
valid_port "$FILEBROWSER_PORT" || fail "invalid TEAMD_FILEBROWSER_PORT: $FILEBROWSER_PORT"

if [ "$(id -u)" -ne 0 ] && [ "$DRY_RUN" -eq 0 ]; then
  need_command sudo
fi

if [ "$ENABLE_NATS" -eq 0 ] && [ "$ENABLE_SEARXNG" -eq 0 ] && [ "$ENABLE_JAEGER" -eq 0 ] && [ "$ENABLE_MEM0" -eq 0 ] && [ "$ENABLE_SILVERBULLET" -eq 0 ] && [ "$ENABLE_BROWSERLESS" -eq 0 ] && [ "$INSTALL_AGENT_BROWSER" -eq 0 ] && [ "$ENABLE_FILEBROWSER" -eq 0 ] && [ "$ENABLE_CADDY" -eq 0 ]; then
  fail "nothing to deploy: NATS disabled, SearXNG disabled, Jaeger not enabled, Mem0 not enabled, SilverBullet not enabled, Browserless/agent-browser not enabled, File Browser not enabled, and Caddy disabled"
fi

if docker_components_enabled; then
  ensure_docker
  ensure_edge_network
  remove_legacy_manual_mcp_runtime
fi

if [ "$ENABLE_NATS" -eq 1 ]; then
  write_nats_files
  configure_agentd_nats_env
  compose_up "$NATS_COMPOSE"
fi

if [ "$ENABLE_SEARXNG" -eq 1 ]; then
  write_searxng_files
  configure_agentd_web_search_env
  compose_up "$SEARXNG_COMPOSE"
fi

if [ "$ENABLE_JAEGER" -eq 1 ]; then
  write_jaeger_files
  configure_agentd_otlp_env
  compose_up "$JAEGER_COMPOSE"
fi

if [ "$ENABLE_MEM0" -eq 1 ]; then
  write_mem0_files
  configure_agentd_mem0_env
  compose_up_build "$MEM0_COMPOSE"
  configure_mem0_server
fi

if [ "$ENABLE_BROWSERLESS" -eq 1 ]; then
  write_browserless_files
  compose_up "$BROWSERLESS_COMPOSE"
fi

if [ "$INSTALL_AGENT_BROWSER" -eq 1 ]; then
  install_agent_browser_cli
  configure_agentd_browser_env
fi

if [ "$ENABLE_SILVERBULLET" -eq 1 ]; then
  write_silverbullet_files
  if [ "$ENABLE_SILVERBULLET_MCP" -eq 1 ]; then
    compose_up_build "$SILVERBULLET_COMPOSE"
  else
    compose_up "$SILVERBULLET_COMPOSE"
  fi
fi

if [ "$WRITE_SILVERBULLET_MCP_EXAMPLE" -eq 1 ]; then
  write_silverbullet_mcp_example
fi

if [ "$ENABLE_FILEBROWSER" -eq 1 ]; then
  write_filebrowser_files
  compose_up_filebrowser
fi

if [ "$ENABLE_SILVERBULLET_MCP" -eq 1 ]; then
  append_silverbullet_mcp_connector_config
  ensure_teamd_docker_access
fi

if [ "$ENABLE_CADDY" -eq 1 ]; then
  write_caddy_files
  compose_up_caddy
  reload_caddy_if_running
fi

if [ "$ENABLE_NATS" -eq 1 ] || [ "$ENABLE_SEARXNG" -eq 1 ] || [ "$ENABLE_SILVERBULLET_MCP" -eq 1 ] || [ "$ENABLE_JAEGER" -eq 1 ] || [ "$ENABLE_MEM0" -eq 1 ] || [ "$ENABLE_FILEBROWSER" -eq 1 ] || [ "$INSTALL_AGENT_BROWSER" -eq 1 ]; then
  restart_teamd_services
fi

cat <<EOF

Container add-ons:
EOF

if [ "$ENABLE_NATS" -eq 1 ]; then
  cat <<EOF
  NATS JetStream:
    Container: teamd-nats
    Client URL: nats://127.0.0.1:$NATS_PORT
    Monitor URL: http://127.0.0.1:$NATS_MONITOR_PORT
    Compose: $NATS_COMPOSE
    Start command: docker compose -f $NATS_COMPOSE up -d
    Data: $NATS_DATA_DIR
    agentd event bus:
      Env file: $ENV_FILE
      TEAMD_EVENT_BUS_BACKEND=nats_jetstream
      TEAMD_NATS_URL=nats://127.0.0.1:$NATS_PORT
EOF
fi

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
  if [ "$ENABLE_CADDY" -eq 1 ] && [ -n "$CADDY_DOMAIN" ] && [ "$CADDY_SINGLE_DOMAIN" -eq 1 ]; then
    cat <<EOF
    Caddy URL: https://$CADDY_DOMAIN/searxng/
EOF
  elif [ "$ENABLE_CADDY" -eq 1 ] && [ -n "$CADDY_DOMAIN" ]; then
    cat <<EOF
    Caddy URL: https://search.$CADDY_DOMAIN/
EOF
  elif [ "$ENABLE_CADDY" -eq 1 ]; then
    cat <<EOF
    Caddy URL: http://127.0.0.1:$CADDY_HTTP_PORT/searxng/
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
	  if [ -n "$CADDY_DOMAIN" ] && [ "$CADDY_SINGLE_DOMAIN" -eq 1 ]; then
	    cat <<EOF
    Caddy URL: https://$CADDY_DOMAIN/jaeger/
EOF
	  elif [ -n "$CADDY_DOMAIN" ]; then
	    cat <<EOF
    Caddy URL: https://jaeger.$CADDY_DOMAIN/
EOF
  else
    cat <<EOF
    Caddy URL: http://127.0.0.1:$CADDY_HTTP_PORT/jaeger/
EOF
  fi
fi

if [ "$ENABLE_BROWSERLESS" -eq 1 ] || [ "$INSTALL_AGENT_BROWSER" -eq 1 ]; then
  cat <<EOF
  Agent Browser / Browserless:
    agent-browser command: $AGENT_BROWSER_BIN
    PATH symlink: $AGENT_BROWSER_PATH_LINK
    npm package: $AGENT_BROWSER_NPM_PACKAGE
    npm prefix: $AGENT_BROWSER_INSTALL_DIR
    agentd browser config:
      Env file: $ENV_FILE
      TEAMD_BROWSER_ENABLED=true
      TEAMD_BROWSER_COMMAND=$AGENT_BROWSER_BIN
      TEAMD_BROWSER_PROVIDER=cdp
      TEAMD_BROWSER_SESSION_PREFIX=$AGENT_BROWSER_SESSION_PREFIX
      TEAMD_BROWSER_DEFAULT_TIMEOUT_MS=$AGENT_BROWSER_DEFAULT_TIMEOUT_MS
      TEAMD_BROWSER_MAX_OUTPUT_CHARS=$AGENT_BROWSER_MAX_OUTPUT_CHARS
      TEAMD_BROWSERLESS_API_URL=$BROWSERLESS_API_URL
      TEAMD_BROWSERLESS_CDP_URL=$BROWSERLESS_CDP_URL?token=<redacted>
      TEAMD_BROWSERLESS_BROWSER_TYPE=$BROWSERLESS_BROWSER_TYPE
      TEAMD_BROWSERLESS_TTL_MS=$BROWSERLESS_TTL_MS
      TEAMD_BROWSERLESS_STEALTH=$BROWSERLESS_STEALTH
EOF
  if [ "$ENABLE_BROWSERLESS" -eq 1 ]; then
    cat <<EOF
    Browserless:
      Container: teamd-browserless
      Local URL: http://127.0.0.1:$BROWSERLESS_PORT
      Image: $BROWSERLESS_IMAGE
      Compose: $BROWSERLESS_COMPOSE
      Env file: $BROWSERLESS_ENV_FILE
      Start command: docker compose -f $BROWSERLESS_COMPOSE up -d
      Smoke: curl -X POST 'http://127.0.0.1:$BROWSERLESS_PORT/content?token=<token>' -H 'Content-Type: application/json' -d '{"url":"https://example.com"}'
EOF
  fi
fi

if [ "$ENABLE_MEM0" -eq 1 ]; then
  cat <<EOF
  Mem0 semantic memory:
    Container: teamd-mem0
    Postgres: teamd-mem0-postgres
    REST API base: $MEM0_API_BASE
    Compose: $MEM0_COMPOSE
    Env file: $MEM0_ENV_FILE
    Local embeddings: fastembed / $MEM0_FASTEMBED_MODEL ($MEM0_EMBEDDING_DIMS dims)
    LLM extraction: $MEM0_LLM_MODEL via $MEM0_LLM_API_BASE
    agentd Env file: $ENV_FILE
    TEAMD_MEM0_ENABLED=true
    TEAMD_MEM0_DEFAULT_USER_ID=$MEM0_DEFAULT_USER_ID
    TEAMD_MEM0_REQUEST_TIMEOUT_MS=$MEM0_REQUEST_TIMEOUT_MS
    TEAMD_MEM0_DEFAULT_LIMIT=$MEM0_DEFAULT_LIMIT
    TEAMD_MEM0_MAX_LIMIT=$MEM0_MAX_LIMIT
    TEAMD_MEMORY_CURATOR_ENABLED=$MEMORY_CURATOR_ENABLED
    TEAMD_MEMORY_CURATOR_MODE=$MEMORY_CURATOR_MODE
    TEAMD_MEMORY_CURATOR_MIN_CONFIDENCE=$MEMORY_CURATOR_MIN_CONFIDENCE
    TEAMD_MEMORY_CURATOR_MAX_CANDIDATES=$MEMORY_CURATOR_MAX_CANDIDATES
    TEAMD_MEMORY_CURATOR_MAX_OUTPUT_TOKENS=$MEMORY_CURATOR_MAX_OUTPUT_TOKENS
    TEAMD_MEMORY_RECALL_ENABLED=$MEMORY_RECALL_ENABLED
    TEAMD_MEMORY_RECALL_SCOPES=$MEMORY_RECALL_SCOPES
    TEAMD_MEMORY_RECALL_MAX_RESULTS=$MEMORY_RECALL_MAX_RESULTS
    TEAMD_MEMORY_RECALL_MAX_QUERY_CHARS=$MEMORY_RECALL_MAX_QUERY_CHARS
    TEAMD_MEMORY_RECALL_MAX_MEMORY_CHARS=$MEMORY_RECALL_MAX_MEMORY_CHARS
    Start command: docker compose -f $MEM0_COMPOSE up -d --build
    Smoke: POST $MEM0_API_BASE/memories and POST $MEM0_API_BASE/search with X-API-Key from $MEM0_ENV_FILE
EOF
fi

if [ "$ENABLE_FILEBROWSER" -eq 1 ]; then
  cat <<EOF
  File Browser:
    Container: teamd-filebrowser
    Local URL: http://127.0.0.1:$FILEBROWSER_PORT$FILEBROWSER_BASE_URL
    Compose: $FILEBROWSER_COMPOSE
    Start command: docker compose -f $FILEBROWSER_COMPOSE up -d
    Credentials env file: $FILEBROWSER_ENV_FILE
    Roots:
      /srv/agent-homes -> $FILEBROWSER_AGENT_HOMES_DIR
      /srv/workspaces  -> $FILEBROWSER_WORKSPACES_DIR
      /srv/artifacts   -> $FILEBROWSER_ARTIFACTS_DIR
      /srv/knowledge   -> $FILEBROWSER_KNOWLEDGE_DIR
EOF
  if [ -n "$FILEBROWSER_DOCS_DIR" ]; then
    cat <<EOF
      /srv/docs        -> $FILEBROWSER_DOCS_DIR
EOF
  fi
  if [ "$ENABLE_CADDY" -eq 1 ] && [ -n "$CADDY_DOMAIN" ] && [ "$CADDY_SINGLE_DOMAIN" -eq 1 ]; then
    cat <<EOF
    Caddy URL: https://$CADDY_DOMAIN/files/
EOF
  elif [ "$ENABLE_CADDY" -eq 1 ] && [ -n "$CADDY_DOMAIN" ]; then
    cat <<EOF
    Caddy URL: https://files.$CADDY_DOMAIN/
EOF
  elif [ "$ENABLE_CADDY" -eq 1 ]; then
    cat <<EOF
    Caddy URL: http://127.0.0.1:$CADDY_HTTP_PORT/files/
EOF
  fi
fi

if [ "$ENABLE_SILVERBULLET" -eq 1 ]; then
  silverbullet_summary_prefix=$(silverbullet_effective_url_prefix)
  cat <<EOF
  SilverBullet:
    Container: teamd-silverbullet
    Local URL: http://127.0.0.1:$SILVERBULLET_PORT
    URL prefix: ${silverbullet_summary_prefix:-<none>}
    Space: $SILVERBULLET_SPACE_DIR
    Compose: $SILVERBULLET_COMPOSE
    Start command: docker compose -f $SILVERBULLET_COMPOSE up -d
    SB_USER credentials file: $SILVERBULLET_ENV_FILE
EOF
  if [ "$ENABLE_SILVERBULLET_MCP" -eq 1 ]; then
    cat <<EOF
    MCP container: teamd-silverbullet-mcp
    MCP HTTP URL: http://127.0.0.1:$SILVERBULLET_MCP_PORT/mcp
    MCP stdio wrapper: $SILVERBULLET_MCP_STDIO_WRAPPER
    MCP connector: [daemon.mcp_connectors.silverbullet] in $CONFIG_FILE
EOF
  fi
	  if [ -n "$CADDY_DOMAIN" ] && [ "$CADDY_SINGLE_DOMAIN" -eq 1 ]; then
	    silverbullet_summary_url="https://$CADDY_DOMAIN/"
	    if [ -n "$silverbullet_summary_prefix" ]; then
	      silverbullet_summary_url="https://$CADDY_DOMAIN$silverbullet_summary_prefix/"
	    fi
	    cat <<EOF
    Caddy URL: $silverbullet_summary_url
EOF
	  elif [ -n "$CADDY_DOMAIN" ]; then
	    cat <<EOF
    Caddy URL: https://notes.$CADDY_DOMAIN/
EOF
  elif [ -n "$SILVERBULLET_HTTPS_PORT" ]; then
    cat <<EOF
    Caddy URL: https://$CADDY_HOST:$SILVERBULLET_HTTPS_PORT/
EOF
  else
    cat <<EOF
    Caddy URL: <none; set TEAMD_CADDY_DOMAIN or TEAMD_SILVERBULLET_HTTPS_PORT for browser-safe remote access>
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
	  if [ -n "$CADDY_DOMAIN" ] && [ "$CADDY_SINGLE_DOMAIN" -eq 1 ]; then
	    cat <<EOF
    Single-domain mode: yes
    Routes with TEAMD_CADDY_DOMAIN single-domain: /sb/ for SilverBullet, /searxng/, /jaeger/, /files/ when enabled
EOF
	  elif [ -n "$CADDY_DOMAIN" ]; then
	    cat <<EOF
    Routes with TEAMD_CADDY_DOMAIN: search.<domain> plus enabled notes.<domain>, jaeger.<domain>, files.<domain>
EOF
	  elif [ -n "$CADDY_HTTPS_PORT" ]; then
	    cat <<EOF
    Routes without TEAMD_CADDY_DOMAIN:
      HTTP: /searxng/
      HTTP: /jaeger/ when enabled
      HTTP/HTTPS: /files/ when File Browser is enabled
      HTTPS: https://$CADDY_HOST:$SILVERBULLET_HTTPS_PORT/ when SilverBullet is enabled
EOF
	  else
	    cat <<EOF
    Routes without TEAMD_CADDY_DOMAIN: /searxng/ plus enabled /jaeger/ and /files/; SilverBullet uses https://$CADDY_HOST:$SILVERBULLET_HTTPS_PORT/ when enabled
EOF
	  fi
fi
