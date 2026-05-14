# Container add-ons: Docker, SearXNG, SilverBullet, File Browser, Browserless, Mem0, Jaeger, Caddy

Этот документ описывает второй deploy layer вокруг host `agentd`.

Главный принцип: контейнеры дают внешнюю обвязку, но не создают второй agent runtime. `agentd` остаётся host systemd service и продолжает владеть sessions, runs, tools, schedules, Telegram delivery, PostgreSQL state, artifacts и audit logs.

## Что ставит deploy script

Скрипт:

```bash
./scripts/deploy-teamd-containers.sh
```

По умолчанию поднимает:

- `teamd-nats` — local NATS JetStream для MIMO/webhook event runtime;
- `teamd-searxng` — локальный search backend для `web_search`;
- `teamd-caddy` — edge reverse proxy.
- `teamd-web` — host systemd service для native web console; Caddy проксирует `/web/` и `/api/agentd/*` на него.

Опционально:

- `teamd-silverbullet` — browser UI для canonical Markdown knowledge space;
- `teamd-silverbullet-mcp` — SilverBullet MCP bridge;
- `teamd-filebrowser` — browser UI для редактирования agent homes, `SYSTEM.md`, `AGENTS.md`, `skills/`, workspaces, artifacts и knowledge files;
- `teamd-jaeger` — Jaeger UI и OTLP receiver для traces;
- `teamd-browserless` + `agent-browser` — recommended browser automation backend для built-in `browser_*` tools;
- Mem0/OpenMemory REST endpoint — optional semantic long-term memory backend для built-in `memory_*` tools.

## Recommended install

Основной production вариант:

```bash
./scripts/deploy-teamd-containers.sh --with-silverbullet-mcp --with-filebrowser --with-jaeger --single-domain
```

Если нужен recommended browser automation backend:

```bash
./scripts/deploy-teamd-containers.sh --with-browserless
```

Если нужно включить `memory_*` tools на уже поднятый Mem0/OpenMemory REST API:

```bash
TEAMD_MEM0_API_BASE='http://127.0.0.1:18888' \
TEAMD_MEM0_API_KEY='optional-api-key' \
TEAMD_MEM0_DEFAULT_USER_ID='anton' \
  ./scripts/deploy-teamd-containers.sh --no-searxng --no-caddy --with-mem0
```

Если нужен только `agent-browser` CLI/config без Browserless container:

```bash
./scripts/deploy-teamd-containers.sh --no-searxng --no-caddy --with-agent-browser
```

Если нужен только SilverBullet без MCP:

```bash
./scripts/deploy-teamd-containers.sh --with-silverbullet
```

Если нужен только File Browser для правки prompts/skills/workspaces:

```bash
./scripts/deploy-teamd-containers.sh --no-searxng --with-filebrowser
```

Dry-run без изменений на сервере:

```bash
./scripts/deploy-teamd-containers.sh --dry-run --non-interactive --no-start --with-silverbullet-mcp
```

## NATS JetStream

NATS — обязательная внешняя зависимость для webhook/MIMO event runtime, но не source of truth. Source of truth остаётся PostgreSQL; NATS доставляет события между ingress/router/session/delivery workers.

Default deployment:

```text
container: teamd-nats
client:    nats://127.0.0.1:4222
monitor:   http://127.0.0.1:8222
data:      /var/lib/teamd/containers/nats
compose:   /opt/teamd/containers/nats/docker-compose.yml
```

Отключить NATS, если нужен только legacy polling stack:

```bash
./scripts/deploy-teamd-containers.sh --no-nats
```

Скрипт пишет в `/etc/teamd/teamd.env`:

```bash
TEAMD_EVENT_BUS_BACKEND=nats_jetstream
TEAMD_NATS_URL=nats://127.0.0.1:4222
```

Он не включает `TEAMD_EVENT_BUS_REQUIRED=true` автоматически. Это делает оператор отдельно, когда переводит Telegram с polling на webhook.

## SilverBullet Space

Canonical space:

```text
/var/lib/teamd/knowledge/silverbullet/teamd
```

Это обычная директория с Markdown-файлами. Её видят:

- оператор через SilverBullet web UI;
- агент через `silverbullet-space` skill;
- агент через `silverbullet` MCP connector, если он включён;
- агент через canonical filesystem tools как fallback, если MCP недоступен.

SilverBullet не заменяет runtime state. В space не должны переезжать:

- transcripts;
- runs;
- tool calls;
- schedules;
- approvals;
- artifacts;
- audit logs;
- PostgreSQL runtime state.

Эти данные остаются в `agentd`.

После durable create/update заметки агент должен сохранить короткий pointer в Mem0, если заметка не помечена `memory: false`. Pointer не копирует всю заметку: он хранит путь, тему, краткое содержание, tags и причину важности. Это нужно, чтобы следующая сессия через pre-turn `Memory Recall` нашла указатель и затем прочитала полную заметку в SilverBullet.

Актуальные правила структуры пространства хранятся в самом SilverBullet:

- `[[r/silverbullet-instrukciya]]` / `https://teamd.qlbc.ru/sb/r/silverbullet-instrukciya`;
- `[[r/system-guide]]` / `https://teamd.qlbc.ru/sb/r/system-guide`.

Краткий контракт для агента:

- контейнеры PARA/Zettelkasten — корневые страницы `Projects.md`, `Areas.md`, `Resources.md`, `Archive.md`, `00-Inbox.md`, `05-Journal.md`, `06-Zettelkasten.md`;
- реальные заметки создаются в одноуровневых namespace: `p/`, `a/`, `r/`, `journals/`, `template/`;
- SilverBullet queries ищут inline `#tag` в теле заметки, поэтому YAML `tags:` без inline tag недостаточен;
- для SilverBullet v2 используется Space Lua / Lua Integrated Query, а не старый `[query: ...]`, который отображается как обычный текст;
- контейнерные страницы не должны превращаться в длинные заметки: они остаются каталогами, query/list страницами и точками навигации;
- агент должен использовать active skill `silverbullet-space`; если skill не активен, перед durable note changes нужно прочитать его через `skill_read`.

Пример query-блока для контейнерной страницы:

```text
${template.each(query[[
from p = index.tag "project"
where p.tag == "page" and p.name:startsWith("p/") and table.includes(p.tags, "done")
order by p.name
]], templates.pageItem)}
```

## SilverBullet credentials and tokens

Credentials file:

```text
/opt/teamd/containers/silverbullet/silverbullet.env
```

Deploy script создаёт или сохраняет:

```bash
SB_USER='admin:<generated-password>'
SB_AUTH_TOKEN='<generated-token>'
MCP_TOKEN='<generated-token>'
```

`SB_USER` можно задать заранее:

```bash
TEAMD_SILVERBULLET_USER='admin:change-me' \
  ./scripts/deploy-teamd-containers.sh --with-silverbullet-mcp
```

`silverbullet.env` устанавливается с правами `0640 root:teamd`, если группа `teamd` существует. Не коммитьте этот файл.

## SilverBullet MCP

`--with-silverbullet-mcp` добавляет:

- `teamd-silverbullet-mcp` container;
- local HTTP endpoint `http://127.0.0.1:4000/mcp`;
- stdio wrapper `/opt/teamd/containers/silverbullet/silverbullet-mcp-stdio.sh`;
- enabled connector в `/etc/teamd/config.toml`.

Config block:

```toml
[daemon.mcp_connectors.silverbullet]
transport = "stdio"
command = "/opt/teamd/containers/silverbullet/silverbullet-mcp-stdio.sh"
args = []
enabled = true
```

`agentd` видит connector как обычный MCP connector. Модель не должна знать Docker internals: она должна использовать `mcp_search_resources`, `mcp_read_resource` и discovered MCP tools, либо skill `silverbullet-space`.

Если нужен только пример config без изменения `/etc/teamd/config.toml`:

```bash
./scripts/deploy-teamd-containers.sh --with-silverbullet-mcp-example
```

Пример будет здесь:

```text
/opt/teamd/containers/silverbullet/silverbullet-mcp.example.toml
```

Важно: текущий SilverBullet MCP bridge — community add-on, а не часть core `silverbullet.md`. Поэтому deploy script держит его явно отдельно и позволяет менять repository/ref:

```bash
TEAMD_SILVERBULLET_MCP_REPOSITORY='https://github.com/Ahmad-A0/silverbullet-mcp.git'
TEAMD_SILVERBULLET_MCP_REF='v1.1.0'
```

## File Browser

File Browser нужен оператору как простой web editor для runtime-owned файлов, которые неудобно править через SSH:

- agent homes: `/var/lib/teamd/state/agents`;
- agent workspaces: `/var/lib/teamd/workspaces`;
- artifacts: `/var/lib/teamd/state/artifacts`;
- knowledge files: `/var/lib/teamd/knowledge`;
- optional docs mount через `TEAMD_FILEBROWSER_DOCS_DIR`.

Deploy:

```bash
./scripts/deploy-teamd-containers.sh --with-filebrowser
```

Credentials лежат в:

```text
/opt/teamd/containers/filebrowser/filebrowser.env
```

Если `TEAMD_FILEBROWSER_ADMIN_PASSWORD` не задан, deploy script генерирует пароль и сохраняет его в комментарии внутри этого env file. В `FB_PASSWORD` передаётся hashed password, как ожидает File Browser quick setup. Файл не коммитится.

По умолчанию container слушает только localhost:

```text
http://127.0.0.1:8092/files
```

Caddy routes:

- без domain: `http://127.0.0.1:8088/files/`;
- dedicated domain: `https://files.<domain>/`;
- single-domain mode: `https://<domain>/files/`.

Безопасность:

- контейнер запускается с `PUID/PGID` service user `teamd`, если этот user существует;
- root в File Browser равен `/srv`;
- в `/srv` монтируются только перечисленные allowlisted roots, а не весь host filesystem;
- `FB_DISABLE_EXEC=true`, то есть shell execution внутри File Browser отключён.

## Agent skills

Built-in default agent получает skills текущего production stack:

- `silverbullet-space` — Markdown knowledge space и SilverBullet UI/MCP;
- `mem0-memory` — долговременная семантическая память;
- `scoped-kv` — точные scoped key-value настройки и малые JSON records;
- `telegram-operator-workflow` — команды и mobile workflow Telegram;
- `browser-search` — `web_search`, `web_fetch`, Browserless/agent-browser;
- `file-artifact-workflow` — Telegram documents, artifacts и `deliver_file`;
- `planning-session-lifecycle` — plan, schedules, `continue_later`, session lifecycle;
- `agent-browser` — built-in `browser_*` tools.

Путь в agent home:

```text
/var/lib/teamd/state/agents/default/skills/<skill-name>/SKILL.md
```

Путь в runtime template:

```text
/var/lib/teamd/state/agent-templates/default/skills/<skill-name>/SKILL.md
```

Если нужно поменять shipped skill без пересборки `agentd`, правьте runtime template. Если нужно поменять только поведение уже существующего `default` agent profile, правьте `agents/default/skills/...` напрямую. Bootstrap создаёт отсутствующие template-файлы из bundled repo `agent-templates/`, но не должен молча затирать operator-modified agent files.

Рекомендуемый workflow правки skill через File Browser:

1. Открыть `/srv/state/agent-templates/default/skills/<skill-name>/SKILL.md`, если нужно поменять template для будущих/synced profiles.
2. Открыть `/srv/state/agents/<agent_id>/skills/<skill-name>/SKILL.md`, если нужно поменять только конкретный agent profile.
3. После правки проверить catalog и activation:

```bash
teamdctl session skills <session_id>
teamdctl session enable-skill <session_id> <skill-name>
teamdctl session disable-skill <session_id> <skill-name>
```

Правка `SKILL.md`, `SYSTEM.md`, `AGENTS.md` не требует пересборки binary. Нужен только новый turn/session; если менялся runtime template и нужно синхронизировать agent home, используйте agent/profile sync команду или создайте новый profile из template.

Включить вручную:

```bash
teamdctl session enable-skill <session_id> silverbullet-space
teamdctl session skills <session_id>
```

## Caddy routes

Без dedicated domain:

- SearXNG: `http://127.0.0.1:8088/searxng/`;
- Jaeger через Caddy: `http://127.0.0.1:8088/jaeger/`, если включён `--with-jaeger`;
- File Browser: `http://127.0.0.1:8088/files/`, если включён `--with-filebrowser`;
- SilverBullet: `https://<host>:8444/`, если включён `--with-silverbullet` или `--with-silverbullet-mcp`.

С dedicated domain:

```bash
TEAMD_CADDY_DOMAIN='example.com' \
  ./scripts/deploy-teamd-containers.sh --with-silverbullet-mcp --with-jaeger
```

Routes:

- `https://search.example.com/` -> SearXNG;
- `https://notes.example.com/` -> SilverBullet;
- `https://jaeger.example.com/` -> Jaeger, если включён;
- `https://files.example.com/` -> File Browser, если включён.

Single-domain mode:

```bash
TEAMD_CADDY_DOMAIN='teamd.qlbc.ru' \
  ./scripts/deploy-teamd-containers.sh --with-silverbullet-mcp --with-jaeger --single-domain
```

Routes:

- `https://teamd.qlbc.ru/sb/` -> SilverBullet;
- `https://teamd.qlbc.ru/web/` -> native teamD Web Console;
- `https://teamd.qlbc.ru/searxng/` -> SearXNG;
- `https://teamd.qlbc.ru/jaeger/` -> Jaeger;
- `https://teamd.qlbc.ru/files/` -> File Browser.

В single-domain mode deploy script автоматически выставляет `SB_URL_PREFIX=/sb` и настраивает SilverBullet MCP на `http://silverbullet:3000/sb`, чтобы браузерный UI и агентский MCP работали через один и тот же prefixed SilverBullet server. Переопределить prefix можно через `TEAMD_SILVERBULLET_URL_PREFIX`; значение должно начинаться с `/` и не должно заканчиваться `/`.

## SearXNG

SearXNG остаётся рекомендуемым backend для `web_search`.

Deploy script upsert'ит env:

```bash
TEAMD_WEB_SEARCH_BACKEND='searxng_json'
TEAMD_WEB_SEARCH_URL='http://127.0.0.1:8888/search'
```

Smoke check:

```bash
curl 'http://127.0.0.1:8888/search?q=test&format=json'
```

## Mem0 semantic memory

Mem0/OpenMemory — optional external semantic memory service. В teamD он подключается как backend для built-in `memory_*` tools:

- `memory_add`;
- `memory_search`;
- `memory_list`;
- `memory_update`;
- `memory_delete`.

Это не MCP connector и не второй runtime. Tools идут через canonical provider loop, approvals, tool-call ledger и debug UI.

Если дополнительно включён `[memory_curator]`, agentd после каждого успешного chat turn делает короткий provider-вызов без tools, получает JSON candidates и применяет их через `memory_search` + `memory_add`. Deploy script при `--with-mem0` по умолчанию upsert'ит:

```bash
TEAMD_MEMORY_CURATOR_ENABLED=true
TEAMD_MEMORY_CURATOR_MODE=auto
TEAMD_MEMORY_CURATOR_MIN_CONFIDENCE=0.8
TEAMD_MEMORY_CURATOR_MAX_CANDIDATES=5
TEAMD_MEMORY_CURATOR_MAX_OUTPUT_TOKENS=512
TEAMD_MEMORY_RECALL_ENABLED=true
TEAMD_MEMORY_RECALL_SCOPES=operator,workspace,agent_shared
TEAMD_MEMORY_RECALL_MAX_RESULTS=6
TEAMD_MEMORY_RECALL_MAX_QUERY_CHARS=512
TEAMD_MEMORY_RECALL_MAX_MEMORY_CHARS=800
```

Curator отвечает за post-turn запись durable facts. `memory_recall` отвечает за pre-turn чтение: runtime сам делает bounded search по последнему user-сообщению и вставляет найденное в видимый prompt block `Memory Recall`. Default recall scopes: `operator`, `workspace`, `agent_shared`. Ручные `memory_*` tools остаются нужны для явного поиска, списка, исправления и удаления memories.

Для Mem0 `workspace` scope хранится как `agent_id = teamd-workspace-<hash>`. Это не профиль агента, а совместимый с Mem0/OpenMemory способ получить изолированный semantic-search namespace, потому что self-hosted Mem0 search стабильно фильтрует по `user_id`, `agent_id` и `run_id`, а не по одному `app_id`. Если Postgres volume Mem0 переезжал между пользователями, права директории `/var/lib/teamd/containers/mem0/postgres` должны принадлежать uid/gid `999:999` контейнерного Postgres.

Официальный self-host Mem0 REST API использует paths без `/v1`: `POST /memories`, `POST /search`, `GET /memories`, `PUT /memories/{id}`, `DELETE /memories/{id}`. Auth для self-host endpoint делается через `X-API-Key`.

Ссылки на официальную документацию:

- Self-host setup: <https://docs.mem0.ai/open-source/setup>
- REST API server: <https://docs.mem0.ai/open-source/features/rest-api>
- OSS configuration: <https://docs.mem0.ai/open-source/configuration>

`deploy-teamd-containers.sh --with-mem0` поднимает воспроизводимый local backend:

- `teamd-mem0` — Mem0 REST API на `127.0.0.1:18888`;
- `teamd-mem0-postgres` — Postgres + pgvector;
- local embeddings — `fastembed`, default model `sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2`, 384 dimensions;
- LLM extraction — OpenAI-compatible endpoint, default `glm-4.5-air` через `https://api.z.ai/api/coding/paas/v4`;
- bundled Mem0 source patch — `pgvector` cosine distance (`<=>`) конвертируется в similarity score (`1 - distance`) до общего Mem0 reranking;
- secrets — `/opt/teamd/containers/mem0/mem0.env`, в git не попадают.

Команда:

```bash
TEAMD_MEM0_DEFAULT_USER_ID='anton' \
  ./scripts/deploy-teamd-containers.sh --no-searxng --no-caddy --with-mem0
```

Если `TEAMD_MEM0_LLM_API_KEY` не задан, deploy script берёт `TEAMD_PROVIDER_API_KEY` из `/etc/teamd/teamd.env`.

После деплоя script upsert'ит client-side настройки `agentd`:

```bash
TEAMD_MEM0_ENABLED='true'
TEAMD_MEM0_API_BASE='http://127.0.0.1:18888'
TEAMD_MEM0_API_KEY='generated-admin-api-key'
TEAMD_MEM0_DEFAULT_USER_ID='local-operator'
TEAMD_MEM0_REQUEST_TIMEOUT_MS='120000'
TEAMD_MEM0_DEFAULT_LIMIT='10'
TEAMD_MEM0_MAX_LIMIT='50'
```

Почему default port `18888`, а не официальный Mem0 `8888`: в стандартной teamD container обвязке `8888` уже занят SearXNG. Если вы подключаете внешний Mem0 endpoint, задайте `TEAMD_MEM0_API_BASE` и `TEAMD_MEM0_API_KEY` явно.

Smoke check endpoint:

```bash
admin_key=$(sudo awk -F= '/^ADMIN_API_KEY=/ { print $2 }' /opt/teamd/containers/mem0/mem0.env)

curl -sS -H "X-API-Key: $admin_key" \
  http://127.0.0.1:18888/configure/providers

curl -sS -X POST http://127.0.0.1:18888/memories \
  -H "X-API-Key: $admin_key" \
  -H 'Content-Type: application/json' \
  -d '{"messages":[{"role":"user","content":"TeamD smoke memory: отвечать кратко на русском."}],"user_id":"teamd-smoke","infer":false}'

curl -sS -X POST http://127.0.0.1:18888/search \
  -H "X-API-Key: $admin_key" \
  -H 'Content-Type: application/json' \
  -d '{"query":"кратко на русском","filters":{"user_id":"teamd-smoke"},"top_k":3}'
```

Smoke check через агента:

```text
Запомни в долгосрочную память: я предпочитаю краткие ответы на русском. Затем найди это в памяти.
```

## Browserless + agent-browser

`--with-browserless` включает current browser automation path:

- `teamd-browserless` container на `127.0.0.1:3000`;
- `agent-browser` CLI wrapper `/opt/teamd/bin/agent-browser`;
- PATH symlink `/usr/local/bin/agent-browser`;
- Browserless token в `/opt/teamd/containers/browserless/browserless.env`;
- `TEAMD_BROWSER_*` и `TEAMD_BROWSERLESS_*` в `/etc/teamd/teamd.env`.

Почему так:

- `browser_*` tools являются built-in tools, а не MCP shim;
- все вызовы идут через обычный provider/tool loop, ledger, artifacts/offload и debug UI;
- Browserless даёт нормальный Chromium backend без установки отдельного desktop browser в runtime user home;
- browser session name строится от teamD session id, поэтому разные чаты не делят cookies/state случайно.

Deploy:

```bash
./scripts/deploy-teamd-containers.sh --with-browserless
```

Что пишет deploy script:

```bash
TEAMD_BROWSER_ENABLED='true'
TEAMD_BROWSER_COMMAND='/opt/teamd/bin/agent-browser'
TEAMD_BROWSER_PROVIDER='cdp'
TEAMD_BROWSER_SESSION_PREFIX='teamd'
TEAMD_BROWSER_DEFAULT_TIMEOUT_MS='30000'
TEAMD_BROWSER_MAX_OUTPUT_CHARS='20000'
TEAMD_BROWSERLESS_API_URL='http://127.0.0.1:3000'
TEAMD_BROWSERLESS_CDP_URL='ws://127.0.0.1:3000/chromium?token=<generated token>'
TEAMD_BROWSERLESS_API_KEY='<generated token>'
TEAMD_BROWSERLESS_BROWSER_TYPE='chromium'
TEAMD_BROWSERLESS_TTL_MS='300000'
TEAMD_BROWSERLESS_STEALTH='true'
```

Smoke checks:

```bash
curl -X POST 'http://127.0.0.1:3000/content?token=<token>' \
  -H 'Content-Type: application/json' \
  -d '{"url":"https://example.com"}'
AGENT_BROWSER_CDP='ws://127.0.0.1:3000/chromium?token=<token>' agent-browser open https://example.com
AGENT_BROWSER_CDP='ws://127.0.0.1:3000/chromium?token=<token>' agent-browser snapshot -i -c
```

Default agent skill:

```text
agent-browser
```

Включить вручную:

```bash
teamdctl session enable-skill <session_id> agent-browser
teamdctl session skills <session_id>
```

Agent workflow:

1. `web_search`, если URL не задан.
2. `browser_open` для выбранного URL.
3. `browser_snapshot` для карты страницы и refs.
4. `browser_click`/`browser_fill`/`browser_press`/`browser_scroll`/`browser_wait` для интерактива.
5. После page-changing action снова `browser_snapshot`, потому что старые refs устаревают.
6. `browser_text`, `browser_eval`, `browser_screenshot`, `browser_pdf` только когда они реально нужны.

Официальные опорные документы:

- Browserless open-source deployment: <https://docs.browserless.io/enterprise/open-source>
- agent-browser package: <https://www.npmjs.com/package/agent-browser>

## Jaeger

`--with-jaeger` поднимает `teamd-jaeger` и настраивает OTLP endpoint. Best-effort OTLP auto-export остаётся выключенным по умолчанию:

```bash
TEAMD_OTLP_EXPORT_ENABLED='false'
TEAMD_OTLP_ENDPOINT='http://127.0.0.1:4318/v1/traces'
TEAMD_OTLP_TIMEOUT_MS='2000'
```

Для постоянного auto-export поменяй `TEAMD_OTLP_EXPORT_ENABLED` на `true` явно. Для разовой отправки trace достаточно `agentd trace push <trace_id>`.

UI:

```text
http://127.0.0.1:16686/jaeger/
```

Через Caddy:

```text
http://127.0.0.1:8088/jaeger/
https://jaeger.<domain>/
https://<single-domain>/jaeger/
```

## Security model

- SilverBullet защищается `SB_USER`.
- SearXNG и Jaeger в этой схеме не имеют пользовательской авторизации. Не публикуйте их наружу без reverse-proxy auth/firewall/VPN, если сервер доступен не только вам.
- MCP wrappers требуют Docker access для `teamd`, потому что `agentd` запускает stdio bridge через Docker. Это сильное право; выдавайте его только trusted runtime user.
- Secrets лежат в env files под `/opt/teamd/containers/*/*.env`, а не в git.
- Web Console Basic Auth включается на native `teamd-web` server через `TEAMD_WEB_AUTH_USER` и `TEAMD_WEB_AUTH_PASSWORD`; это закрывает и UI, и proxy `/api/agentd/*`.

## Проверка после deploy

Контейнеры:

```bash
docker ps --format 'table {{.Names}}\t{{.Status}}\t{{.Ports}}'
```

SilverBullet:

```bash
curl -I http://127.0.0.1:8091/
curl -I https://teamd.qlbc.ru/sb/
ls -la /var/lib/teamd/knowledge/silverbullet/teamd
```

Browserless:

```bash
. /opt/teamd/containers/browserless/browserless.env
curl -sS -X POST "http://127.0.0.1:3000/content?token=$TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"url":"https://example.com"}'
set -a
. /etc/teamd/teamd.env
set +a
AGENT_BROWSER_CDP="$TEAMD_BROWSERLESS_CDP_URL" agent-browser open https://example.com
AGENT_BROWSER_CDP="$TEAMD_BROWSERLESS_CDP_URL" agent-browser snapshot -i -c
```

MCP config:

```bash
grep -A5 'daemon.mcp_connectors.silverbullet' /etc/teamd/config.toml
grep 'TEAMD_BROWSER_' /etc/teamd/teamd.env
systemctl restart teamd-daemon teamd-telegram
```

## Troubleshooting

Если SilverBullet открывается, но агент не видит MCP:

1. проверьте `/etc/teamd/config.toml`;
2. проверьте `/opt/teamd/containers/silverbullet/silverbullet-mcp-stdio.sh`;
3. проверьте `docker logs teamd-silverbullet-mcp`;
4. перезапустите `teamd-daemon` и `teamd-telegram`.

Если `teamd-silverbullet-mcp` не собирается:

```bash
docker compose -f /opt/teamd/containers/silverbullet/docker-compose.yml build silverbullet-mcp
docker logs teamd-silverbullet-mcp
```

Если Caddy показывает старые routes, пересоберите Caddy config:

```bash
./scripts/deploy-teamd-containers.sh --with-silverbullet-mcp --single-domain
docker exec teamd-caddy caddy reload --config /etc/caddy/Caddyfile
```

Если `browser_*` tools недоступны агенту:

1. проверьте `/etc/teamd/teamd.env`: `TEAMD_BROWSER_ENABLED`, `TEAMD_BROWSER_COMMAND`, `TEAMD_BROWSER_PROVIDER`;
2. проверьте `/opt/teamd/bin/agent-browser --help`;
3. проверьте `docker ps | grep teamd-browserless`;
4. проверьте Browserless token в `/opt/teamd/containers/browserless/browserless.env`;
5. перезапустите `teamd-daemon` и `teamd-telegram`.

## Ссылки

- SilverBullet: <https://silverbullet.md/>
- SilverBullet community MCP: <https://github.com/Ahmad-A0/silverbullet-mcp>
- Browserless open-source deployment: <https://docs.browserless.io/enterprise/open-source>
- agent-browser npm package: <https://www.npmjs.com/package/agent-browser>
- SearXNG: <https://docs.searxng.org/>
- Jaeger: <https://www.jaegertracing.io/>
- Caddy: <https://caddyserver.com/docs/>
