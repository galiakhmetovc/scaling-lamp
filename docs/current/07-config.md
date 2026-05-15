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

### `[database]`

Управляет PostgreSQL control-plane store. PostgreSQL хранит sessions, runs, jobs, schedules, tool-call ledger, KV, search indexes, Telegram bindings и trace links. Большие тела transcript/artifact по-прежнему лежат payload-файлами в `data_dir`.

Параметры:

- `url`
- `connect_timeout_seconds`
- `application_name`

Рекомендуемое правило: `database.url` не хранить в `config.toml`, а задавать через `TEAMD_DATABASE_URL` в `/etc/teamd/teamd.env`, потому URL обычно содержит пароль.

Пример:

```toml
[database]
connect_timeout_seconds = 5
application_name = "teamd"
```

```bash
export TEAMD_DATABASE_URL='postgresql://teamd:password@127.0.0.1:5432/teamd'
```

`scripts/deploy-teamd.sh` делает это автоматически: если `TEAMD_DATABASE_URL` не задан, он ставит/использует local PostgreSQL, создаёт роль и БД `teamd`, генерирует пароль и пишет URL в `/etc/teamd/teamd.env`.

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
- `kimi_anthropic_messages` — Kimi Code Anthropic-compatible Messages API.

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

Для Kimi Code используется Anthropic-compatible endpoint из официальной документации Kimi:

```toml
[provider]
kind = "kimi_anthropic_messages"
api_base = "https://api.kimi.com/coding"
default_model = "kimi-for-coding"
max_output_tokens = 32768
```

```bash
export TEAMD_PROVIDER_API_KEY='replace-with-kimi-key'
```

Если `api_base` и `default_model` не заданы, runtime использует Kimi defaults:

- `api_base = "https://api.kimi.com/coding"`
- `default_model = "kimi-for-coding"`

Legacy alias `kimi_chat_completions` пока принимается и мапится на `kimi_anthropic_messages`, но новый конфиг должен использовать явное имя `kimi_anthropic_messages`.

### `[permissions]`

Определяет permission mode для tool execution. Это часть общей security/control модели runtime.

### `[telegram]`

Управляет Telegram-интеграцией. Практический setup описан отдельно в [telegram/01-install-and-configure.md](telegram/01-install-and-configure.md).

Основные параметры:

- `enabled`
- `bot_token`
- `mode`
- `webhook_public_url`
- `webhook_secret`
- `poll_interval_ms`
- `poll_request_timeout_seconds`
- `progress_update_min_interval_ms`
- `pairing_token_ttl_seconds`
- `max_upload_bytes`
- `max_download_bytes`
- `private_chat_auto_create_session`
- `group_require_mention`
- `default_autoapprove`
- `inbound_queue_default_mode`
- `inbound_coalesce_window_ms`
- `inbound_min_coalesce_window_ms`
- `message_text_soft_cap`
- `caption_soft_cap`
- `status_detail_char_cap`
- `status_ttl_seconds`
- `typing_initial_delay_ms`
- `typing_heartbeat_interval_seconds`
- `delivery_retry_attempts`
- `delivery_retry_base_delay_ms`

Рекомендуемое правило: `telegram.bot_token` не хранить в `config.toml`, а задавать через `TEAMD_TELEGRAM_BOT_TOKEN` в `.env` или environment.

`mode = "polling"` запускает legacy long polling worker через `agentd telegram run`. `mode = "webhook"` отключает polling и принимает Telegram updates через daemon HTTP route `/v1/telegram/webhook/<secret>`. Webhook mode требует `telegram.webhook_public_url`, `telegram.webhook_secret` и `event_bus.required = true`.

Env overrides:

```bash
TEAMD_TELEGRAM_MODE=webhook
TEAMD_TELEGRAM_WEBHOOK_PUBLIC_URL=https://teamd.example/v1/telegram/webhook/...
TEAMD_TELEGRAM_WEBHOOK_SECRET=...
```

`group_require_mention = true` означает строгий режим для неактивированных Telegram users. Если user уже прошёл pairing и запись activated, worker принимает его обычный текст в group/supergroup chat без mention и routes его в выбранную group session.

`inbound_queue_default_mode` управляет тем, что Telegram worker делает с обычным сообщением, если в выбранной session уже выполняется turn. Допустимые значения: `reject`, `queue`, `coalesce`, `restart`. Default — `coalesce`. `inbound_coalesce_window_ms` задаёт окно объединения входящих сообщений; минимум задаётся явно через `inbound_min_coalesce_window_ms` и применяется при сохранении `/queue coalesce ...`.

Telegram rendering/delivery limits тоже являются config policy: `message_text_soft_cap`, `caption_soft_cap`, `status_detail_char_cap`, `status_ttl_seconds`, typing heartbeat и retry-настройки delivery. Это важно для hotfix'ов вроде `MESSAGE_TOO_LONG`: оператор меняет policy в `config.toml`, а не правит Rust-код.

### `[event_bus]`

Управляет event backbone для MIMO/webhook runtime. PostgreSQL остаётся source of truth, а NATS JetStream используется для durable delivery, replay, backpressure и независимых workers.

Параметры:

- `required`
- `backend`
- `nats_url`
- `input_stream`
- `session_stream`
- `delivery_stream`
- `task_stream`
- `dlq_stream`

Единственный поддерживаемый backend сейчас — `nats_jetstream`.

Пример:

```toml
[event_bus]
required = true
backend = "nats_jetstream"
nats_url = "nats://127.0.0.1:4222"
input_stream = "TEAMD_INPUT"
session_stream = "TEAMD_SESSION"
delivery_stream = "TEAMD_DELIVERY"
task_stream = "TEAMD_TASK"
dlq_stream = "TEAMD_DLQ"
```

Env overrides:

```bash
TEAMD_EVENT_BUS_REQUIRED=true
TEAMD_EVENT_BUS_BACKEND=nats_jetstream
TEAMD_NATS_URL=nats://127.0.0.1:4222
TEAMD_EVENT_BUS_INPUT_STREAM=TEAMD_INPUT
TEAMD_EVENT_BUS_SESSION_STREAM=TEAMD_SESSION
TEAMD_EVENT_BUS_DELIVERY_STREAM=TEAMD_DELIVERY
TEAMD_EVENT_BUS_TASK_STREAM=TEAMD_TASK
TEAMD_EVENT_BUS_DLQ_STREAM=TEAMD_DLQ
```

Если `event_bus.required = true` и Telegram включён, runtime валидирует `telegram.mode = "webhook"`. Это защита от двух конкурирующих ingestion paths для одного bot token.

### `[retention]`

Управляет ручной очисткой TeamD-owned runtime-файлов. Команды:

```bash
teamdctl disk usage
teamdctl disk prune
teamdctl disk prune --execute
```

`disk prune` по умолчанию всегда dry-run: он только показывает кандидатов. Реальное удаление возможно только с `--execute`.

Параметры:

- `audit_rotated_log_max_age_days` — rotated audit logs в `data_dir/audit`, но не текущий `runtime.jsonl`.
- `debug_bundle_max_age_days` — `data_dir/audit/debug-bundles`.
- `deploy_backup_max_age_days` — deploy backups из `deploy_backup_dir`.
- `diagnostics_max_age_days` — diagnostic bundles из `diagnostics_dir`.
- `legacy_sqlite_max_age_days` — старые `data_dir/state.sqlite*` после перехода на PostgreSQL.
- `workspace_trash_max_age_days` — `.trash` внутри generated agent workspaces.
- `workspace_scratch_max_age_days` — `scratch` внутри generated agent workspaces.
- `session_archive_max_age_days` — `data_dir/archives`.
- `deploy_backup_dir` — внешний путь к backup старых binary, например `/opt/teamd/backups`.
- `diagnostics_dir` — внешний путь к diagnostic bundles, например `/var/lib/teamd/diagnostics`.

Пример:

```toml
[retention]
audit_rotated_log_max_age_days = 30
debug_bundle_max_age_days = 14
deploy_backup_max_age_days = 14
diagnostics_max_age_days = 14
legacy_sqlite_max_age_days = 7
workspace_trash_max_age_days = 30
workspace_scratch_max_age_days = 14
session_archive_max_age_days = 180
deploy_backup_dir = "/opt/teamd/backups"
diagnostics_dir = "/var/lib/teamd/diagnostics"
```

Что не чистится автоматически: PostgreSQL rows, projects, обычные workspace-файлы, текущий `audit/runtime.jsonl`, canonical artifact payloads с metadata в PostgreSQL и SilverBullet space. Для таких данных нужен отдельный жизненный цикл, чтобы не получить битые ссылки в ledger.

### `[session_defaults]`

Управляет настройками новых session:

- `working_memory_limit`
- `project_memory_enabled`

### `[workspace]`

Управляет fallback project workspace для новых session.

Параметры:

- `default_root`

Порядок выбора workspace при создании session:

- если у выбранного `Agent profile` задан `default_workspace_root`, используется он. Built-in profiles и новые profiles из templates получают отдельный workspace вида `<data_dir-parent>/workspaces/agents/<agent_id>/`;
- иначе используется `workspace.default_root`;
- если и он не задан, bootstrap fallback идёт в текущий process workspace;
- `data_dir`, `audit`, `transcripts`, `artifacts` и `runs` использовать как workspace нельзя.

Пример:

```toml
[workspace]
default_root = "/srv/projects/teamd"
```

Важно: `workspace.default_root` теперь именно fallback/base runtime workspace, а не “один каталог для всех агентов”. Нормальный рабочий путь для нового agent profile хранится в `agent_profiles.default_workspace_root` и виден через:

```bash
teamdctl agent show <agent_id>
teamdctl agent open <agent_id>
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

### `[mem0]`

Управляет optional semantic long-term memory через self-hosted Mem0/OpenMemory REST API.

Default:

```toml
[mem0]
enabled = false
api_base = "http://127.0.0.1:18888"
default_user_id = "local-operator"
request_timeout_ms = 120000
default_limit = 10
max_limit = 50
# api_key = "..."
```

Env:

```bash
export TEAMD_MEM0_ENABLED='true'
export TEAMD_MEM0_API_BASE='http://127.0.0.1:18888'
export TEAMD_MEM0_API_KEY='m0sk_or_admin_key'
export TEAMD_MEM0_DEFAULT_USER_ID='anton'
export TEAMD_MEM0_REQUEST_TIMEOUT_MS='120000'
export TEAMD_MEM0_DEFAULT_LIMIT='10'
export TEAMD_MEM0_MAX_LIMIT='50'
```

### `[memory_curator]`

Управляет post-turn самообучением агента поверх Mem0. Это не отдельный chat loop и не скрытый tool path: основной ответ пользователю сначала полностью завершается и пишется в transcript/run, затем runtime делает отдельный короткий provider-вызов без tools с `think_level = off`, просит вернуть строгий JSON с memory candidates и применяет их через тот же Mem0 слой.

Default:

```toml
[memory_curator]
enabled = false
mode = "auto"
min_confidence = 0.8
max_candidates = 5
max_output_tokens = 512
```

Env:

```bash
export TEAMD_MEMORY_CURATOR_ENABLED='true'
export TEAMD_MEMORY_CURATOR_MODE='auto' # auto | review | off
export TEAMD_MEMORY_CURATOR_MIN_CONFIDENCE='0.8'
export TEAMD_MEMORY_CURATOR_MAX_CANDIDATES='5'
export TEAMD_MEMORY_CURATOR_MAX_OUTPUT_TOKENS='512'
```

Как работает:

- запускается только если одновременно `memory_curator.enabled = true` и `mem0.enabled = true`;
- prompt curator хранится в `data_dir/agent-templates/system/memory-curator/SYSTEM.md`; если файла нет, daemon создаёт его из bundled repo template;
- получает compact turn packet: `session_id`, `run_id`, `agent_profile_id`, `workspace_root`, последнее сообщение пользователя, финальный ответ ассистента и summaries tool calls текущего run;
- сохраняет только durable facts: предпочтения оператора, устойчивые факты проекта/workspace, долгоживущие правила;
- перед сохранением делает `memory_search` по тому же scope и пропускает exact duplicates;
- не сохраняет секреты, пароли, токены, API keys, pairing keys и похожие credential-like строки;
- ошибки curator, provider или Mem0 пишутся в `audit/runtime.jsonl`, но не ломают основной chat turn;
- в `mode = "review"` candidates только фиксируются в audit как `review_required`, без auto-save;
- в `mode = "off"` curator не запускается.

### `[memory_recall]`

Управляет pre-turn чтением Mem0. Это не tool loop модели: перед обычным provider request runtime берёт последнее user-сообщение, делает bounded `POST /search` по настроенным scopes и вставляет найденное в prompt отдельным видимым system-блоком `Memory Recall`. Ошибка Mem0 не валит turn: она пишется в `audit/runtime.jsonl`, а prompt собирается без recall-блока.

Default:

```toml
[memory_recall]
enabled = true
scopes = ["operator", "workspace", "agent_shared"]
max_results = 6
max_query_chars = 512
max_memory_chars = 800
```

Env:

```bash
export TEAMD_MEMORY_RECALL_ENABLED='true'
export TEAMD_MEMORY_RECALL_SCOPES='operator,workspace,agent_shared'
export TEAMD_MEMORY_RECALL_MAX_RESULTS='6'
export TEAMD_MEMORY_RECALL_MAX_QUERY_CHARS='512'
export TEAMD_MEMORY_RECALL_MAX_MEMORY_CHARS='800'
```

Как работает:

- запускается только если одновременно `memory_recall.enabled = true` и `mem0.enabled = true`;
- ищет по последнему `user` transcript entry текущего turn;
- default scopes: `operator` для предпочтений оператора, `workspace` для проектных решений и `agent_shared` для общих уроков всех агентов;
- результат попадает в prompt после `SessionHead`/`AutonomyState` и до `Plan`;
- блок остаётся inspectable: его видно в provider prompt preview/debug, а не только внутри модели;
- если модели нужно больше деталей, она всё ещё может явно вызвать `memory_search` или `memory_list`.

Scope mapping в Mem0 сделан через entity filters, чтобы разные уровни памяти не смешивались:

- `operator` -> `user_id = mem0.default_user_id`;
- `agent` -> `agent_id = <agent_profile_id>`;
- `agent_shared` -> `agent_id = teamd-agent-shared`;
- `workspace` -> `agent_id = teamd-workspace-<sha256(workspace_root)[0..16]>`;
- `session` -> `run_id = <session_id>`.

Для поиска runtime отправляет `POST /search` с `filters` и `top_k`, а не отдельные top-level `user_id`/`limit`. Пользовательские `filters` объединяются с entity filter выбранного scope. Mem0 здесь используется как semantic memory, а не как KV/state store: точные ключи, locks, counters и runtime-состояние должны жить во встроенном PostgreSQL-backed KV-слое.

### Built-in KV layer

У KV нет отдельной внешней инфраструктуры и отдельной config section. Он всегда хранится в PostgreSQL в таблице `kv_entries` и доступен модели через built-in tools:

- `kv_get`;
- `kv_put`;
- `kv_list`;
- `kv_delete`.

KV scopes совпадают с Mem0 scopes по смыслу:

- `operator` — namespace оператора, сейчас использует `mem0.default_user_id` как общий локальный operator id;
- `agent` — namespace текущего `agent_profile_id`;
- `agent_shared` — общий namespace `teamd-agent-shared`;
- `workspace` — default, namespace `teamd-workspace-<sha256(workspace_root)[0..16]>`;
- `session` — namespace текущего `session_id`.

Это точный JSON store, а не semantic memory. Используйте KV для exact state, counters, cursors, flags и lightweight coordination. Используйте Mem0 для durable facts/lessons/preferences, которые надо потом найти по смыслу.

Backend deploy через `scripts/deploy-teamd-containers.sh --with-mem0`:

- поднимает `teamd-mem0` на `127.0.0.1:18888` и `teamd-mem0-postgres`;
- использует local `fastembed` для embeddings, default `sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2`, 384 dimensions;
- использует OpenAI-compatible LLM endpoint для extraction, default `glm-4.5-air` через Z.ai;
- патчит self-hosted Mem0 `pgvector` adapter: raw cosine distance из PostgreSQL нормализуется в similarity score (`1 - distance`), иначе Mem0 ранжирует semantic results в неправильную сторону;
- генерирует `ADMIN_API_KEY`, `JWT_SECRET`, `POSTGRES_PASSWORD` в `/opt/teamd/containers/mem0/mem0.env`;
- upsert'ит `TEAMD_MEM0_*`, `TEAMD_MEMORY_CURATOR_*` и `TEAMD_MEMORY_RECALL_*` в `/etc/teamd/teamd.env`.

Дополнительные env для backend deploy:

```bash
export TEAMD_MEM0_PORT='18888'
export TEAMD_MEM0_LLM_API_BASE='https://api.z.ai/api/coding/paas/v4'
export TEAMD_MEM0_LLM_API_KEY='...' # optional; fallback: TEAMD_PROVIDER_API_KEY
export TEAMD_MEM0_LLM_MODEL='glm-4.5-air'
export TEAMD_MEM0_FASTEMBED_MODEL='sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2'
export TEAMD_MEM0_EMBEDDING_DIMS='384'
export TEAMD_MEM0_COLLECTION_NAME='teamd_memories_fastembed_384'
```

Почему default port `18888`, а не официальный Mem0 `8888`: в стандартной teamD container обвязке `8888` уже занят SearXNG. Если Mem0 запущен отдельно на другом endpoint, явно задайте `TEAMD_MEM0_API_BASE`.

Что меняется при `enabled = true`:

- model-facing список tools получает `memory_add`, `memory_search`, `memory_list`, `memory_update`, `memory_delete`;
- runtime вызывает Mem0 endpoints `POST /memories`, `POST /search`, `GET /memories`, `PUT /memories/{id}`, `DELETE /memories/{id}`;
- при наличии `api_key` он отправляется как `X-API-Key`;
- все вызовы остаются в canonical provider loop и tool-call ledger.

Что не меняется:

- PostgreSQL остаётся источником истины для sessions/runs/schedules/tool calls;
- transcript, artifacts и `ContextSummary` не переезжают в Mem0;
- SilverBullet/docs остаются knowledge/documentation layer.
- KV/state store уже есть во встроенном PostgreSQL runtime store; Mem0 хранит семантически извлекаемые memories, а не точные runtime-ключи.

`scripts/deploy-teamd-containers.sh --with-mem0` поднимает backend и конфигурирует `agentd`. Если нужен внешний Mem0/OpenMemory endpoint, задайте `TEAMD_MEM0_API_BASE` и `TEAMD_MEM0_API_KEY` перед запуском script.

### `[knowledge]`

Управляет operator context и SilverBullet-интеграцией, которая попадает в канонический prompt через `SessionHead`. Runtime mirror в SilverBullet выключен по умолчанию: live/status state надо смотреть через Telegram status, web UI, traces, transcripts, tool ledger и artifacts.

Параметры:

- `operator_timezone` — timezone оператора для относительных дат, daily journals и человекочитаемых timestamp; production default — `Europe/Moscow`.
- `silverbullet_space_dir` — путь к canonical SilverBullet Space на диске.
- `silverbullet_base_url` — optional browser URL; сейчас используется как операторская подсказка/документация, а не как источник runtime state.
- `silverbullet_journal_context_enabled` — включает bounded чтение `journals/<today>.md` и `journals/<yesterday>.md` в `SessionHead`.
- `silverbullet_mirror_enabled` — включает best-effort запись runtime mirror pages в SilverBullet Space; production default `false`.
- `silverbullet_session_area_path` — относительный путь index page для зеркал сессий.
- `silverbullet_text_artifact_extensions` и `silverbullet_script_artifact_extensions` — какие artifact-файлы можно inline'ить в mirror page как текст/скрипт.
- `source_files`, `source_dirs`, `allowed_extensions`, `max_file_bytes` — канонические корни и размерный лимит для `knowledge_search`/`knowledge_read`. Это больше не зашито в коде: можно менять, какие файлы и папки workspace индексируются как project knowledge.
- `knowledge_search` использует PostgreSQL full-text search только по этим configured roots. Это не произвольный filesystem search; если в root попал недоступный файл, индексатор пропускает его, а не валит весь tool.
- `max_file_bytes` защищает daemon от индексации больших generated outputs вроде `result.json`; такие файлы нужно читать через filesystem/artifact tools, а не через global FTS.

Default:

```toml
[knowledge]
operator_timezone = "Europe/Moscow"
silverbullet_space_dir = "/var/lib/teamd/knowledge/silverbullet/teamd"
# silverbullet_base_url = "https://teamd.example/sb"
silverbullet_journal_context_enabled = true
silverbullet_mirror_enabled = false
silverbullet_session_area_path = "a/teamd-agents.md"
silverbullet_text_artifact_extensions = [
  "bash", "css", "csv", "html", "js", "json", "lua", "md", "py", "rs",
  "sh", "sql", "toml", "ts", "txt", "xml", "yaml", "yml",
]
silverbullet_script_artifact_extensions = ["bash", "js", "lua", "py", "rs", "sh", "ts"]
allowed_extensions = ["md", "txt", "json", "yaml", "yml", "toml"]
max_file_bytes = 1048576

[[knowledge.source_files]]
path = "README.md"
root = "root_docs"
kind = "root_doc"

[[knowledge.source_files]]
path = "SYSTEM.md"
root = "root_docs"
kind = "root_doc"

[[knowledge.source_files]]
path = "AGENTS.md"
root = "root_docs"
kind = "root_doc"

[[knowledge.source_dirs]]
path = "docs"
root = "docs"
kind = "project_doc"

[[knowledge.source_dirs]]
path = "projects"
root = "projects"
kind = "project_doc"

[[knowledge.source_dirs]]
path = "notes"
root = "notes"
kind = "project_note"
```

Env:

```bash
export TEAMD_OPERATOR_TIMEZONE='Europe/Moscow'
export TEAMD_SILVERBULLET_SPACE_DIR='/var/lib/teamd/knowledge/silverbullet/teamd'
export TEAMD_SILVERBULLET_BASE_URL='https://teamd.example/sb'
export TEAMD_SILVERBULLET_JOURNAL_CONTEXT_ENABLED='true'
export TEAMD_SILVERBULLET_MIRROR_ENABLED='false'
```

Как это работает:

- `data_dir/USER.md` создаётся из встроенного template при первом чтении и потом редактируется оператором без пересборки бинаря;
- `USER.md` попадает в `SessionHead` bounded-блоком `Operator Context`;
- если включён journal context и space существует, runtime читает today/yesterday daily notes из `journals/YYYY-MM-DD.md` с учётом `operator_timezone`;
- если оператор явно включает mirror и space существует, runtime после успешных chat/wakeup/inter-agent/approval turns и compaction пишет человекочитаемые pages в `silverbullet_session_area_path` и `p/teamd-session-<session_id>.md`;
- `knowledge_search` индексирует только `source_files` и файлы из `source_dirs` с расширениями из `allowed_extensions`, затем ищет по PostgreSQL full-text search;
- unreadable/stale/non-UTF8 файлы в этих roots пропускаются при обновлении индекса;
- SilverBullet mirror не является источником истины: plan, transcript, tool calls, artifacts и schedules остаются в `agentd` state. По умолчанию mirror выключен, чтобы агент не транслировал transient runtime status в базу заметок.

### `[observability]`

Управляет внешним экспортом runtime traces.

Параметры:

- `otlp_export_enabled` — включает best-effort auto-export completed run traces;
- `otlp_endpoint` — OTLP/HTTP endpoint, обычно `http://127.0.0.1:4318/v1/traces`;
- `otlp_timeout_ms` — timeout одной отправки trace.

Default:

```toml
[observability]
otlp_export_enabled = false
otlp_endpoint = "http://127.0.0.1:4318/v1/traces"
otlp_timeout_ms = 2000
```

То же через env:

```bash
export TEAMD_OTLP_EXPORT_ENABLED='true'
export TEAMD_OTLP_ENDPOINT='http://127.0.0.1:4318/v1/traces'
export TEAMD_OTLP_TIMEOUT_MS='2000'
```

`scripts/deploy-teamd-containers.sh --with-jaeger` поднимает локальный Jaeger и upsert-ит endpoint в `/etc/teamd/teamd.env`, но оставляет `TEAMD_OTLP_EXPORT_ENABLED='false'`. Автоэкспорт включается только явным изменением env/config; ручной `agentd trace push <trace_id>` продолжает работать через настроенный endpoint.

Важно: exporter не отправляет raw prompts, transcript bodies или большие tool outputs. В spans попадают compact attributes и ссылки на локальные сущности (`session_id`, `run_id`, `tool_call_id`, `artifact_id`). Сбой OTLP export не ломает пользовательский turn, а пишется в `audit/runtime.jsonl`.

### `[runtime_timing]`

Это теперь каноническое место для всех operator-facing timing policies:

- store retry delay
- daemon HTTP connect/request timeouts
- A2A connect timeout
- autospawn polling
- shutdown/restart polling
- server request poll interval
- background worker tick interval
- background worker MCP connector maintenance interval
- background worker heavy memory/index maintenance interval
- background worker job lease duration
- TUI event polling
- MCP stdio polling
- provider retry delay

Раньше такие числа были размазаны по коду. Теперь они собраны в одном config surface.

`daemon_background_worker_tick_interval_ms` отвечает за быстрый lightweight tick: schedules, jobs, inbox wakeups. `daemon_mcp_maintenance_interval_seconds` отдельно ограничивает проверку и автозапуск MCP connectors. `daemon_memory_maintenance_interval_seconds` отдельно ограничивает тяжёлую maintenance-часть: session search index, knowledge index и retention recalculation. Не ставьте maintenance-интервалы равными 0 или слишком маленькими на больших workspaces: это будет постоянно перечитывать docs/projects/notes, перезапроверять MCP и грузить Postgres/CPU.

### `[runtime_limits]`

Здесь собраны operator/runtime-facing лимиты:

- diagnostic tail size
- store retry attempts
- active run step preview limits
- transcript tail run limit
- agent/schedule/MCP/session search limits
- session read limits
- knowledge read/search limits
- operator `USER.md` context limit
- SilverBullet today/yesterday journal context limits
- SilverBullet text artifact/script mirror limits
- timeline preview chars
- session warm idle seconds
- filesystem listing/page caps для `fs_list`/`fs_glob`
- process output and wait/kill timing limits для `exec_*`
- provider transient retry count
- provider loop guard для одинаковых tool-call подряд
- provider empty-response recovery count
- tool-call result preview cap before offloading full output into artifact
- context offload/artifact read caps
- KV key/value/list caps
- skill list/read/install caps
- autonomy state read caps
- SessionHead activity/tree limits
- default max hops for new inter-agent chains

Идея та же: убрать магические числа из runtime path и сделать policy явно конфигурируемой.

## Env overrides

`AppConfig` поддерживает и environment overrides. В коде они читаются в [`crates/agent-persistence/src/config.rs`](../../crates/agent-persistence/src/config.rs).

Полезно знать, что можно переопределять:

- data dir;
- workspace default root;
- PostgreSQL database URL/connect timeout/application name;
- daemon bind host/port/token/public URL/skills dir;
- context compaction thresholds;
- web search backend/URL;
- knowledge/operator context and SilverBullet mirror toggles;
- observability OTLP endpoint/export flag/timeout;
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
- PostgreSQL metadata/control-plane store
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

Текущий recommended knowledge add-on — SilverBullet Space + optional SilverBullet MCP:

```bash
./scripts/deploy-teamd-containers.sh --with-silverbullet-mcp
```

Он создаёт browser-editable Markdown space, поднимает SilverBullet, добавляет `silverbullet` MCP connector в `/etc/teamd/config.toml` и перезапускает `teamd` services. Агент работает с этим knowledge layer через `silverbullet-space` skill; если MCP connector доступен, он предпочтителен, иначе fallback — штатные filesystem tools внутри canonical space path.

Ключевые env-переменные container deploy path:

```bash
TEAMD_SILVERBULLET_SPACE_DIR='/var/lib/teamd/knowledge/silverbullet/teamd'
TEAMD_SILVERBULLET_PORT='8091'
TEAMD_SILVERBULLET_HTTPS_PORT='8444'
TEAMD_SILVERBULLET_USER='username:password'
TEAMD_SILVERBULLET_MCP_PORT='4000'
TEAMD_CADDY_DOMAIN='example.com'
TEAMD_CADDY_HOST='31.130.128.89'
```

Без dedicated domain SilverBullet публикуется как отдельный HTTPS site `https://<host>:8444/`. С `TEAMD_CADDY_DOMAIN` используется `https://notes.<domain>/`, а в `--single-domain` mode — `https://<domain>/`.

Для self-signed HTTPS без домена deploy script использует `TEAMD_CADDY_HOST`. Если переменная не задана, он пытается определить primary IPv4 автоматически. Если снаружи нужен другой адрес, задайте `TEAMD_CADDY_HOST` явно.

Устаревшие add-ons удалены из supported deploy path. Для заметок используйте SilverBullet, для браузера — Browserless/agent-browser.

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
