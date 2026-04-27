# Конфигурация

## Где лежит конфиг

По умолчанию `agentd` читает:

- `~/.config/teamd/config.toml`

Путь можно переопределить через:

- `TEAMD_CONFIG`

Полный пример лежит в [config.example.toml](../../config.example.toml).

В systemd-установке из `scripts/deploy-teamd.sh` используется явный production-like layout:

- `/etc/teamd/config.toml` — TOML-конфиг без секретов;
- `/etc/teamd/teamd.env` — секреты и env overrides для systemd/CLI;
- `/var/lib/teamd/state` — `data_dir`, то есть runtime state.

`config.toml` и `teamd.env` не дублируют друг друга. TOML описывает устойчивую конфигурацию, а env file хранит секреты и то, что удобно переопределять вне TOML.

## Главные секции

### `[daemon]`

Управляет daemon-слоем:

- `bind_host`
- `bind_port`
- `bearer_token`
- `skills_dir`
- `public_base_url`
- `a2a_peers`
- `mcp_connectors`

Если у daemon есть `bearer_token`, HTTP API начинает требовать `Authorization: Bearer ...`.

### `[provider]`

Управляет LLM provider:

- `kind`
- `api_base`
- `api_key`
- `default_model`
- `connect_timeout_seconds`
- `request_timeout_seconds`
- `stream_idle_timeout_seconds`
- `max_tool_rounds`
- `max_output_tokens`

Поддерживаемые provider kinds:

- `openai_responses` — OpenAI Responses API;
- `zai_chat_completions` — Z.ai Chat Completions-compatible API.

Для Z.ai достаточно указать kind и ключ в environment:

```toml
[provider]
kind = "zai_chat_completions"
```

```bash
export TEAMD_PROVIDER_API_KEY='replace-with-zai-key'
```

Если `api_base` и `default_model` не заданы, runtime использует defaults:

- `api_base = "https://api.z.ai/api/coding/paas/v4"`
- `default_model = "glm-5-turbo"`

`/chat/completions` дописывать в `api_base` не надо: driver добавляет endpoint сам.

### `[permissions]`

Определяет permission mode для tool execution. Это часть общей security/control модели runtime.

### `[telegram]`

Управляет Telegram-интеграцией. Практический setup описан отдельно в [telegram/01-install-and-configure.md](telegram/01-install-and-configure.md).

Основные параметры:

- `enabled`
- `bot_token`
- `poll_interval_ms`
- `poll_request_timeout_seconds`
- `progress_update_min_interval_ms`
- `pairing_token_ttl_seconds`
- `max_upload_bytes`
- `max_download_bytes`
- `private_chat_auto_create_session`
- `group_require_mention`
- `default_autoapprove`

Рекомендуемое правило: `telegram.bot_token` не хранить в `config.toml`, а задавать через `TEAMD_TELEGRAM_BOT_TOKEN` в `.env` или environment.

### `[session_defaults]`

Управляет настройками новых session:

- `working_memory_limit`
- `project_memory_enabled`

### `[workspace]`

Управляет default project workspace для новых session.

Параметры:

- `default_root`

Порядок выбора workspace при создании session:

- если у выбранного `Agent profile` задан `default_workspace_root`, используется он;
- иначе используется `workspace.default_root`;
- если и он не задан, bootstrap fallback идёт в текущий process workspace;
- `data_dir`, `audit`, `transcripts`, `artifacts` и `runs` использовать как workspace нельзя.

Пример:

```toml
[workspace]
default_root = "/srv/projects/teamd"
```

То же через env:

```bash
export TEAMD_WORKSPACE_DEFAULT_ROOT='/srv/projects/teamd'
```

### `[context]`

Управляет compaction policy:

- `compaction_min_messages`
- `compaction_keep_tail_messages`
- `compaction_max_output_tokens`
- `compaction_max_summary_chars`
- `auto_compaction_trigger_ratio`
- `context_window_tokens_override`

Роли полей:

- `compaction_*` управляют тем, как именно создаётся summary;
- `auto_compaction_trigger_ratio` управляет тем, когда runtime автоматически запускает compaction перед provider turn;
- `context_window_tokens_override` явно задаёт размер окна контекста для auto-compaction.

Пример:

```toml
[context]
compaction_min_messages = 20
compaction_keep_tail_messages = 6
compaction_max_output_tokens = 4096
compaction_max_summary_chars = 12000
auto_compaction_trigger_ratio = 0.7
context_window_tokens_override = 200000
```

То же через env:

```bash
export TEAMD_CONTEXT_AUTO_COMPACTION_TRIGGER_RATIO='0.7'
export TEAMD_CONTEXT_WINDOW_TOKENS='200000'
```

Если `context_window_tokens_override` не задан, runtime сначала пытается использовать built-in mapping для известных моделей. Если модель неизвестна и override не задан, автоматическая compaction не сработает заранее и останется доступна только ручная `\компакт`.

### `[web]`

Управляет встроенными web tools.

Сейчас конфигурируется `web_search`:

- `search_backend = "duckduckgo_html"` — default: встроенный HTML-парсер DuckDuckGo;
- `search_backend = "searxng_json"` — локальный/свой SearXNG endpoint с JSON output;
- `search_url` — endpoint поиска.

Пример default:

```toml
[web]
search_backend = "duckduckgo_html"
search_url = "https://duckduckgo.com/html/"
```

Пример локального SearXNG из `scripts/deploy-teamd-containers.sh`:

```toml
[web]
search_backend = "searxng_json"
search_url = "http://127.0.0.1:8888/search"
```

То же через env:

```bash
export TEAMD_WEB_SEARCH_BACKEND='searxng_json'
export TEAMD_WEB_SEARCH_URL='http://127.0.0.1:8888/search'
```

`scripts/deploy-teamd-containers.sh` upsert-ит эти env-переменные в `/etc/teamd/teamd.env`, когда SearXNG включён.

Важно: `web_fetch` не переключается на SearXNG. Это прямой HTTP fetch указанного URL. SearXNG закрывает именно search backend. Model-facing guidance требует сначала использовать `web_search` для current/external информации и только потом `web_fetch` по точному URL.

Важно: для `text/html`/`xhtml` `web_fetch` теперь не отдаёт модели сырой HTML по умолчанию. Runtime конвертирует HTML в markdown-подобный readable text через `html-to-markdown-rs`, извлекает заголовок страницы, а большие результаты уводит в context offload artifact вместо inline prompt payload.

### `[runtime_timing]`

Это теперь каноническое место для всех operator-facing timing policies:

- SQLite busy timeout
- daemon HTTP connect/request timeouts
- A2A connect timeout
- autospawn polling
- shutdown/restart polling
- server request poll interval
- background worker tick interval
- TUI event polling
- MCP stdio polling
- provider retry delay

Раньше такие числа были размазаны по коду. Теперь они собраны в одном config surface.

### `[runtime_limits]`

Здесь собраны operator/runtime-facing лимиты:

- diagnostic tail size
- active run step preview limits
- transcript tail run limit
- agent/schedule/MCP/session search limits
- session read limits
- knowledge read/search limits
- timeline preview chars
- session warm idle seconds

Идея та же: убрать магические числа из runtime path и сделать policy явно конфигурируемой.

## Env overrides

`AppConfig` поддерживает и environment overrides. В коде они читаются в [`crates/agent-persistence/src/config.rs`](../../crates/agent-persistence/src/config.rs).

Полезно знать, что можно переопределять:

- data dir;
- workspace default root;
- daemon bind host/port/token/public URL/skills dir;
- context compaction thresholds;
- web search backend/URL;
- Telegram bot token;
- provider kind/base/key/model/timeouts/max rounds/max output tokens;
- permission mode;
- session defaults.

На практике это удобно для:

- локальных smoke tests;
- запуска под `sudo`;
- временных экспериментов без переписывания основного TOML.

## `data_dir`

`data_dir` особенно важен. От него зависят:

- `agents/`
- `state.sqlite`
- `artifacts/`
- `archives/`
- `runs/`
- `transcripts/`
- `audit/runtime.jsonl`

Если запускать бинарь то из-под пользователя, то из-под `root`, легко случайно получить два разных state root’а. Поэтому в production-like запуске стоит явно понимать, какой `data_dir` вы используете.

`data_dir/agents/<agent_id>` сейчас является `agent_home` профиля агента: там prompts и skills, а не рабочий каталог проекта. План явного разделения `agent_home` и `workspace` описан в [11-workspace-modernization-plan.md](11-workspace-modernization-plan.md).

Подробная карта файлов в `data_dir` описана в [06-storage-recovery-and-diagnostics.md](06-storage-recovery-and-diagnostics.md).

## MCP connectors в конфиге

В `[daemon.mcp_connectors]` можно seed’ить stdio MCP connectors:

- command
- args
- env
- cwd
- enabled

Это initial state для MCP runtime surface. Потом оператор может управлять коннекторами через TUI/HTTP/CLI.

Автоматический Obsidian vault MCP connector добавляет второй deploy script:

```bash
./scripts/deploy-teamd-containers.sh --with-obsidian-mcp
```

Этот connector работает через `stdio`: `agentd` запускает `docker run -i --rm ... node:22-alpine npx -y @bitbonsai/mcpvault@latest /vault`, где `/vault` — mount на `/var/lib/teamd/vaults/teamd`. Поэтому для базовой работы не нужен Obsidian Local REST API plugin и не нужен ручной клик в Obsidian UI.

По умолчанию без dedicated domain Obsidian запускается с `TEAMD_OBSIDIAN_SUBFOLDER=/obsidian/`. Значение должно быть пустым или иметь ведущий и завершающий `/`, иначе web route у контейнера будет некорректным.

По умолчанию deploy script использует образ `lscr.io/linuxserver/obsidian:latest`. Его web UI внутри контейнера слушает `TEAMD_OBSIDIAN_CONTAINER_PORT=3000`, а наружу публикуется как `TEAMD_OBSIDIAN_PORT` и затем проксируется Caddy.

Если Obsidian включён без `TEAMD_CADDY_DOMAIN`, deploy script автоматически включает `TEAMD_CADDY_HTTPS_PORT=8443` и переводит внешний доступ к `/obsidian/` на HTTPS, потому что Selkies/WebCodecs не работает в plain HTTP origin.

Для self-signed HTTPS без домена deploy script использует `TEAMD_CADDY_HOST`. Если переменная не задана, он пытается определить primary IPv4 автоматически. Если снаружи нужен другой адрес, задайте `TEAMD_CADDY_HOST` явно.

Если нужен только шаблон без изменения `/etc/teamd/config.toml`, используйте:

```bash
./scripts/deploy-teamd-containers.sh --with-obsidian-mcp-example
```

Шаблон `stdio`-коннектора лежит в `/opt/teamd/containers/obsidian/obsidian-mcp.example.toml`. Подробный runbook: [14-container-addons.md](14-container-addons.md).

## A2A peers в конфиге

В `[daemon.a2a_peers.<id>]` можно описывать удалённых peer daemon’ов:

- `base_url`
- `bearer_token`

Это нужно только для remote delegation; локальный judge не требует A2A.

## Практический минимум

Для обычного локального запуска часто достаточно:

```toml
[provider]
kind = "openai_responses"
default_model = "gpt-5.4"

[permissions]
mode = "default"
```

и ключа в env.

Для Z.ai минимум выглядит так:

```toml
[provider]
kind = "zai_chat_completions"

[permissions]
mode = "default"
```

```bash
export TEAMD_PROVIDER_API_KEY='...'
agentd provider smoke
```

## Где смотреть в коде

- Config structs и defaults: [`crates/agent-persistence/src/config.rs`](../../crates/agent-persistence/src/config.rs)
- Runtime использование timing/limits: [`cmd/agentd/src/http/client.rs`](../../cmd/agentd/src/http/client.rs), [`cmd/agentd/src/daemon.rs`](../../cmd/agentd/src/daemon.rs), [`cmd/agentd/src/tui.rs`](../../cmd/agentd/src/tui.rs), [`cmd/agentd/src/mcp.rs`](../../cmd/agentd/src/mcp.rs), [`cmd/agentd/src/execution/provider_loop.rs`](../../cmd/agentd/src/execution/provider_loop.rs)
