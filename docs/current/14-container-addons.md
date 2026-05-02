# Container add-ons: Docker, SearXNG, Logseq, SilverBullet, Jaeger, Caddy

Этот документ описывает второй deploy path: не core `agentd`, а внешнюю обвязку вокруг него.

Core runtime ставится через:

```bash
./scripts/deploy-teamd.sh
```

Container add-ons ставятся отдельно:

```bash
./scripts/deploy-teamd-containers.sh
```

Такой разделённый путь нужен, чтобы `agentd` оставался обычным host process под systemd и мог работать с локальным workspace/процессами без docker-in-docker и лишних прав.

## Что ставит второй скрипт

По умолчанию:

- Docker Engine + Compose plugin, если их нет;
- `teamd-searxng` — локальный SearXNG search endpoint;
- `teamd-caddy` — Caddy reverse proxy;
- shared Docker network `teamd-edge`.

Опционально:

- `teamd-logseq-publish` — read-only web view над canonical Logseq graph, если передать `--with-logseq`.
- `teamd-silverbullet` — browser editor над тем же Markdown graph, если передать `--with-silverbullet`.
- `teamd-jaeger` — Jaeger UI и OTLP receiver для runtime traces, если передать `--with-jaeger`.
- `teamd-obsidian` и filesystem-backed Obsidian MCP connector — legacy path, если явно передать `--with-obsidian` или `--with-obsidian-mcp`.

Проверить действия без изменений:

```bash
./scripts/deploy-teamd-containers.sh --dry-run --non-interactive --no-start --with-logseq --with-silverbullet
```

## SearXNG для `web_search`

Скрипт поднимает SearXNG на localhost и автоматически прописывает для `agentd` search backend в `/etc/teamd/teamd.env`:

```text
http://127.0.0.1:8888
```

Проверка JSON API:

```bash
curl 'http://127.0.0.1:8888/search?q=test&format=json'
```

Фактические env-переменные, которые upsert-ит скрипт:

```bash
TEAMD_WEB_SEARCH_BACKEND='searxng_json'
TEAMD_WEB_SEARCH_URL='http://127.0.0.1:8888/search'
```

Если `teamd` systemd services уже установлены и активны, скрипт перезапускает их сам. Ручной перезапуск нужен только после ручной правки env/config:

```bash
sudo systemctl restart teamd-daemon.service teamd-telegram.service
```

Если вы редактируете TOML вместо env:

```toml
[web]
search_backend = "searxng_json"
search_url = "http://127.0.0.1:8888/search"
```

`web_fetch` остаётся прямым HTTP fetch tool. Он не ходит через SearXNG, потому что SearXNG — поисковый backend, а не универсальный proxy. Агенту в prompts явно сказано: сначала `web_search`, потом `web_fetch` по точному URL из результата поиска, user input или known canonical source.

Для HTML-страниц runtime теперь конвертирует ответ в markdown-подобный readable text и `title`, а не прокидывает сырой HTML в модель. Это делается внутри встроенного `web_fetch` через `html-to-markdown-rs`; SearXNG здесь не участвует. Если результат слишком большой, он offloadится в artifact и в prompt попадает только компактная ссылка+preview.

## MCP для SearXNG

Скрипт также пишет пример MCP-конфига:

```text
/opt/teamd/containers/searxng/mcp-searxng.example.json
```

Это не включает MCP автоматически. Это шаблон для подключения `mcp-searxng` как отдельного MCP connector, если нужен search как MCP capability.

Ориентир по проекту: <https://github.com/ihor-sokoliuk/mcp-searxng>.

## Logseq graph: текущий knowledge layer

Текущий основной путь для заметок и working knowledge:

```bash
./scripts/deploy-teamd-containers.sh --with-logseq --with-silverbullet
```

Default paths:

- canonical graph root: `/var/lib/teamd/knowledge/logseq`;
- canonical graph: `/var/lib/teamd/knowledge/logseq/teamd`;
- Logseq Publish compose/config: `/opt/teamd/containers/logseq`;
- Logseq Publish output: `/var/lib/teamd/containers/logseq/output`;
- SilverBullet compose/config: `/opt/teamd/containers/silverbullet`;
- SilverBullet credentials: `/opt/teamd/containers/silverbullet/silverbullet.env`.

Смысл разделения:

- `Logseq Publish` отдаёт read-only browser view, удобный для просмотра графа и ссылок;
- `SilverBullet` даёт мобильный и desktop web editor для тех же `.md` файлов;
- `agentd` работает с теми же файлами через canonical filesystem tools и `logseq-graph` skill;
- graph не является runtime state: transcripts, runs, tool calls, artifacts, schedules, approvals, audit logs и SQLite state остаются в `agentd`;
- graph не заменяет repository docs: стабильная документация живёт в git под `docs/`, а graph используется для working notes, drafts, decisions, research, project logs и подготовки материала.

Скрипт seed'ит минимальный Logseq config, если его ещё нет:

```text
/var/lib/teamd/knowledge/logseq/teamd/logseq/config.edn
```

И создаёт стартовую страницу:

```text
/var/lib/teamd/knowledge/logseq/teamd/teamD.md
```

Оба файла создаются только если отсутствуют. Существующий graph скрипт не перезаписывает.

### Web URLs

Без dedicated domain:

```text
Logseq Publish via Caddy: http://127.0.0.1:8088/logseq/
SilverBullet local:       http://127.0.0.1:8091/
SilverBullet via Caddy:   https://<host>:8444/
```

SilverBullet без домена вынесен на отдельный HTTPS site, а не в `/notes/`. Это сделано намеренно: single-page редакторы и web socket/asset paths обычно плохо живут в произвольном subpath.

Если автоопределение host выбрало не тот адрес, задайте его явно:

```bash
TEAMD_CADDY_HOST='31.130.128.89' ./scripts/deploy-teamd-containers.sh --with-logseq --with-silverbullet
```

С dedicated domain в subdomain mode:

```text
https://logseq.example.com/
https://notes.example.com/
```

Команда:

```bash
TEAMD_CADDY_DOMAIN='example.com' ./scripts/deploy-teamd-containers.sh --with-logseq --with-silverbullet
```

Если DNS заведён только на один host, используйте single-domain mode:

```bash
TEAMD_CADDY_DOMAIN='teamd.qlbc.ru' ./scripts/deploy-teamd-containers.sh --with-logseq --with-silverbullet --with-jaeger --single-domain
```

В этом режиме Caddy публикует всё на одном домене:

```text
https://teamd.qlbc.ru/         -> SilverBullet editor
https://teamd.qlbc.ru/logseq/  -> Logseq Publish
https://teamd.qlbc.ru/searxng/ -> SearXNG browser UI
https://teamd.qlbc.ru/jaeger/  -> Jaeger UI
```

SilverBullet остаётся в root path `/`, чтобы editor SPA, websocket/API paths и assets не ломались на произвольном subpath.

### SilverBullet authentication

SilverBullet требует `SB_USER`. Deploy script делает одно из двух:

- если задан `TEAMD_SILVERBULLET_USER`, записывает его в credentials file;
- если credentials file уже есть, оставляет его как есть;
- если ничего нет, генерирует `admin:<random-password>` и сохраняет в `/opt/teamd/containers/silverbullet/silverbullet.env`.

Формат:

```bash
SB_USER='username:password'
```

Файл создаётся с mode `0600`. Не коммитьте его в git и не вставляйте пароль в публичные логи.

Проверить credentials на host:

```bash
sudo cat /opt/teamd/containers/silverbullet/silverbullet.env
```

### Agent skill и PARA contract

Default agent получает встроенный agent-local skill:

```text
/var/lib/teamd/state/agents/default/skills/logseq-graph/SKILL.md
```

Legacy compatibility skill тоже остаётся, но только как указатель на новый путь:

```text
/var/lib/teamd/state/agents/default/skills/obsidian-vault/SKILL.md
```

`logseq-graph` активируется автоматически, когда в сессии есть контекст про `Logseq`, `SilverBullet`, `graph`, `PARA`, `projects`, `areas`, `resources`, `archive`, `notes`, `knowledge base`, Markdown notes, daily notes, tasks, links или frontmatter. Его также можно включить вручную:

```bash
teamdctl session enable-skill <session_id> logseq-graph
teamdctl session skills <session_id>
```

Текущий агентский flow:

- сначала найти/прочитать существующие notes через `fs_find_in_files`, `fs_list`, `fs_read_text` или `fs_read_lines`;
- перед изменением существующей заметки обязательно прочитать её;
- менять graph через `fs_write_text`, `fs_patch_text`, `fs_replace_lines`, `fs_insert_text`, `fs_mkdir`, `fs_move` или `fs_trash`;
- писать только в canonical graph path `/var/lib/teamd/knowledge/logseq/teamd`;
- не создавать второй graph в `~/vault`, `/root/vault`, `/var/lib/teamd/vault`, project workspace или старом Obsidian path;
- после успешного write/update сообщить, что именно изменено и где;
- если tool call упал, не утверждать, что заметка сохранена.

Отдельного Logseq MCP сейчас нет. Это осознанно: не создаём второй скрытый tool loop. Когда появится нормальный MCP/semantic layer для graph, он должен быть добавлен как connector поверх того же canonical graph path, а не как отдельная база.

PARA — default organization model:

| Folder | Назначение |
| --- | --- |
| `00-Inbox` | Быстрые captures, сырые идеи, unsorted Telegram notes, временный вход. |
| `01-Projects` | Активные outcomes с deadline или понятным finish condition. |
| `02-Areas` | Постоянные области ответственности без даты завершения. |
| `03-Resources` | Reference material, research, guides, snippets, domain notes. |
| `04-Archive` | Неактивные проекты, старые resources, завершённые или deprecated notes. |
| `05-Journal` | Daily notes, reviews, logs, timeline entries. |
| `06-Tasks` | Task notes, когда задаче нужна отдельная страница. |
| `attachments` | Файлы, embedded или linked from notes. |
| `templates` | Reusable note templates. |

Daily notes по умолчанию живут в:

```text
05-Journal/YYYY-MM-DD.md
```

Не создавайте отдельный `daily/` tree, если он уже не существует или оператор явно не попросил.

Common operations:

- capture idea: создать/дополнить короткую заметку в `00-Inbox` с source и timestamp;
- create task: создать/обновить note в `06-Tasks` с checklist и priority;
- start project: создать `01-Projects/<project-name>.md` с goal, status, next actions, resources, open questions;
- add resource: создать `03-Resources/<topic>.md` с summary, source links, related notes;
- add daily entry: обновить `05-Journal/YYYY-MM-DD.md`;
- process inbox: разложить inbox items в Projects, Areas, Resources, Archive или Tasks;
- complete work: обновить status/result и переносить в `04-Archive` только если пользователь согласился или завершение явно следует из note;
- search: искать существующие notes перед созданием дубля.

Lightweight frontmatter для новых notes, когда это полезно:

```markdown
---
type: project|area|resource|task|daily|note
status: active|waiting|done|archived
created: YYYY-MM-DD
updated: YYYY-MM-DD
tags: []
---
```

Recommended note contents:

- project: goal, status, next actions, decisions, resources, log;
- task: priority, status, checklist, context, result;
- daily: date, focus, log, tasks, captures;
- resource: summary, key points, sources, related notes.

Tags and Logseq syntax:

- use tags sparingly: `#project`, `#area`, `#resource`, `#task`, `#daily`, `#inbox`, `#archive`;
- priority tags: `#p0`, `#p1`, `#p2`, `#p3`, only when priority matters;
- prefer page links like `[[note name]]`;
- use checkboxes `- [ ]` and `- [x]`;
- preserve Logseq properties, block refs, embeds, links, aliases, headings and frontmatter.

Operating rules:

- не удалять и не архивировать user material без запроса или явного основания в note;
- не выдумывать completed tasks, sources, dates или decisions;
- при неоднозначном target folder выбрать ближайший PARA folder и явно назвать assumption;
- имена notes должны быть стабильными и читаемыми; timestamp-only filenames допустимы только для daily notes;
- если пользовательское сообщение содержит durable fact, decision, task или resource, агент должен предложить сохранить это или сохранить сразу, если запрос подразумевает persistence;
- перед substantial work агент ищет/читает релевантные project, area или resource notes;
- после важного решения или завершённой задачи агент обновляет соответствующую project note или daily journal;
- когда working note стала стабильной документацией, агент предлагает перенести её в repository docs и commit.

Роль Logseq graph в общей архитектуре:

```text
Telegram/TUI dialogue
-> agent reasoning and tools
-> Logseq graph notes through canonical filesystem tools
-> SilverBullet web editing for operator
-> Logseq Publish read-only graph view
-> optional semantic search index over graph later
-> stable docs promoted to git docs/current
```

Быстрая проверка на production host:

```bash
teamdctl session skills <session_id>
grep -nE 'PARA structure|04-Archive|SilverBullet' \
  /var/lib/teamd/state/agents/default/skills/logseq-graph/SKILL.md
ls -la /var/lib/teamd/knowledge/logseq/teamd
curl -fsS http://127.0.0.1:8088/logseq/ >/dev/null
```

### Миграция из старого Obsidian vault

Legacy Obsidian path остаётся доступен, но новые заметки должны идти в Logseq graph:

```text
Old vault:         /var/lib/teamd/vaults/teamd
Compatibility symlink: /var/lib/teamd/vault
New graph:         /var/lib/teamd/knowledge/logseq/teamd
```

Безопасная ручная миграция:

```bash
sudo mkdir -p /var/lib/teamd/knowledge/logseq/teamd
sudo rsync -a --ignore-existing \
  /var/lib/teamd/vaults/teamd/ \
  /var/lib/teamd/knowledge/logseq/teamd/
sudo chown -R teamd:teamd /var/lib/teamd/knowledge/logseq/teamd
```

После миграции проверьте:

- нет ли дублей `00-Inbox`, `01-Projects`, `02-Areas`;
- работают ли links `[[...]]`;
- attachments лежат в ожидаемом каталоге;
- старые Obsidian-only callouts/plugins не стали критичными для чтения.

## Obsidian legacy path

Obsidian support не удалён, но больше не является primary path. Он оставлен для старых инсталляций и ручного восстановления старого vault.

Obsidian UI включается явно:

```bash
./scripts/deploy-teamd-containers.sh --with-obsidian
```

Obsidian MCP включается явно:

```bash
./scripts/deploy-teamd-containers.sh --with-obsidian-mcp
```

Legacy paths:

```text
Vaults:             /var/lib/teamd/vaults
Managed vault:      /var/lib/teamd/vaults/teamd
Compatibility path: /var/lib/teamd/vault -> /var/lib/teamd/vaults/teamd
Local UI:           http://127.0.0.1:8080/obsidian/
Caddy HTTPS:        https://<host>:8443/obsidian/
MCP example:        /opt/teamd/containers/obsidian/obsidian-mcp.example.toml
```

`--with-obsidian-mcp` добавляет systemd-пользователя `teamd` в группу `docker`, потому что MCP connector запускается через `docker run -i --rm`. Это почти root-level право. Для новой Logseq/SilverBullet схемы это не требуется.

Obsidian compatibility skill:

```text
/var/lib/teamd/state/agents/default/skills/obsidian-vault/SKILL.md
```

Новый bootstrap перезаписывает старый generated Obsidian skill коротким deprecated shim, который отправляет агента в `logseq-graph`.

## Jaeger: web UI для runtime traces

Jaeger включается явно:

```bash
./scripts/deploy-teamd-containers.sh --with-jaeger
```

Что делает скрипт:

- поднимает контейнер `teamd-jaeger` на образе `jaegertracing/all-in-one`;
- включает OTLP receiver внутри Jaeger (`COLLECTOR_OTLP_ENABLED=true`);
- публикует UI на `127.0.0.1:${TEAMD_JAEGER_UI_PORT:-16686}`;
- публикует OTLP/gRPC на `127.0.0.1:${TEAMD_JAEGER_OTLP_GRPC_PORT:-4317}`;
- публикует OTLP/HTTP на `127.0.0.1:${TEAMD_JAEGER_OTLP_HTTP_PORT:-4318}`;
- включает persistent Badger storage в `/var/lib/teamd/containers/jaeger/badger`;
- выставляет ownership Badger storage под UID/GID контейнера Jaeger (`TEAMD_JAEGER_UID`, `TEAMD_JAEGER_GID`, default `10001:10001`);
- upsert-ит в `/etc/teamd/teamd.env` настройки auto-export для `agentd`;
- перезапускает `teamd-daemon.service` и `teamd-telegram.service`, если они существуют и не указан `--no-start`/`--no-restart-teamd`.

Фактические env-переменные:

```bash
TEAMD_OTLP_EXPORT_ENABLED='true'
TEAMD_OTLP_ENDPOINT='http://127.0.0.1:4318/v1/traces'
TEAMD_OTLP_TIMEOUT_MS='2000'
```

Эти значения можно задать вручную без контейнерного скрипта:

```toml
[observability]
otlp_export_enabled = true
otlp_endpoint = "http://127.0.0.1:4318/v1/traces"
otlp_timeout_ms = 2000
```

После включения `agentd` автоматически экспортирует completed run traces в OTLP/HTTP. Экспорт best-effort:

- сбой Jaeger/OTLP не ломает chat turn;
- ошибка экспорта пишется в `audit/runtime.jsonl` как `component=otel`, `op=export`;
- в Jaeger уходят compact span attributes и ссылки на локальные сущности (`session_id`, `run_id`, `tool_call_id`, `artifact_id`), а не raw transcript/tool output;
- локальный `state.sqlite`, `transcripts/`, `artifacts/` и debug-view остаются источником истины.

Ручной экспорт уже существующего trace:

```bash
teamdctl trace push <trace_id>
```

Локальный просмотр без Jaeger:

```bash
teamdctl trace run <run_id>
teamdctl trace show <trace_id>
teamdctl trace export <trace_id>
```

URL без dedicated domain:

```text
Direct UI: http://127.0.0.1:16686/jaeger/
Caddy UI: http://127.0.0.1:8088/jaeger/
OTLP HTTP: http://127.0.0.1:4318/v1/traces
```

С `TEAMD_CADDY_DOMAIN='example.com'` Jaeger в subdomain mode публикуется как:

```text
https://jaeger.example.com/
```

В single-domain mode:

```text
https://teamd.qlbc.ru/jaeger/
```

Почему Jaeger, а не Grafana Tempo сразу:

- Jaeger all-in-one проще для одного host: один контейнер, UI и collector в одном процессе;
- Tempo обычно требует Grafana рядом, зато лучше подходит для долгосрочного хранения большого объёма traces;
- OpenTelemetry Collector можно добавить позже как отдельный routing/redaction layer между `agentd` и backend.

Важно: Jaeger — это visual/debug слой, не база знаний и не transcript store. Не кладите в OTLP raw prompts, user messages, assistant answers, API keys, Telegram names или большие tool outputs. Для глубокого debug используйте `teamdctl session tools --results`, `teamdctl session tool-result`, `teamdctl trace show` и TUI debug-view.

## Caddy

Без домена Caddy слушает local port:

```text
http://127.0.0.1:8088
```

Routes:

- `/searxng/`;
- `/logseq/`, если включён `--with-logseq`;
- `/jaeger/`, если включён `--with-jaeger`.
- `/obsidian/`, если включён legacy `--with-obsidian`.

В path mode `/searxng/` прокидывается без срезания префикса и с upstream header `X-Script-Name: /searxng`. Это важно: SearXNG генерирует root-relative ссылки и `form action`; без `X-Script-Name` browser UI уходит на `/search`, `/static/...` и фактически выпадает из `/searxng/`. Для `/searxng` без trailing slash Caddy делает redirect на `/searxng/`.

`/logseq/` отдаёт static SPA из Logseq Publish output и срезает prefix через `handle_path`. Для `/logseq` без trailing slash Caddy делает redirect на `/logseq/`.

SilverBullet без domain не публикуется как subpath. Он получает отдельный Caddy HTTPS site на `https://<host>:8444/`, потому что editor SPA должен видеть себя в root path.

`/obsidian/` тоже прокидывается без срезания префикса, потому что Obsidian container сам запущен с `SUBFOLDER=/obsidian/`.

После записи Caddyfile deploy script пересоздаёт `teamd-caddy` через `docker compose up -d --force-recreate`, затем делает `caddy reload`. Это важно: Caddyfile смонтирован как отдельный bind-mounted file, а атомарная замена файла на host может оставить уже запущенный контейнер на старом inode.

Для нормального browser usage можно задать домен в subdomain mode:

```bash
TEAMD_CADDY_DOMAIN='example.com' ./scripts/deploy-teamd-containers.sh --with-logseq --with-silverbullet
```

Тогда Caddy создаёт:

- `search.example.com`;
- `logseq.example.com`, если включён `--with-logseq`;
- `notes.example.com`, если включён `--with-silverbullet`;
- `jaeger.example.com`, если включён `--with-jaeger`.
- `obsidian.example.com`, если включён legacy `--with-obsidian`.

Если доступен только один DNS host, используйте single-domain mode:

```bash
TEAMD_CADDY_DOMAIN='teamd.qlbc.ru' ./scripts/deploy-teamd-containers.sh --with-logseq --with-silverbullet --with-jaeger --single-domain
```

Тогда Caddy создаёт один site `teamd.qlbc.ru`:

- `/` -> SilverBullet, если включён `--with-silverbullet`;
- `/searxng/`;
- `/logseq/`, если включён `--with-logseq`;
- `/jaeger/`, если включён `--with-jaeger`;
- `/obsidian/`, если включён legacy `--with-obsidian`.

## Mobile/browser workflow

Принятый mobile workflow:

- оператор пишет агенту в Telegram;
- агент создаёт и обновляет Markdown notes в canonical Logseq graph;
- оператор читает graph через Logseq Publish;
- оператор редактирует notes через SilverBullet web UI;
- позже поверх graph можно добавить semantic search/indexing.

Obsidian legacy web UI остаётся desktop/admin интерфейсом. Он не решает mobile editing хорошо, поэтому не является текущим рекомендуемым путём.

## Security notes

- `SearXNG` и `Logseq Publish` не имеют встроенной авторизации в этой схеме. Не публикуйте их в публичный интернет без reverse-proxy auth/firewall/VPN.
- `SilverBullet` защищён `SB_USER`, но пароль всё равно должен быть сильным, а доступ лучше закрывать HTTPS и firewall.
- `Jaeger` может раскрывать topology, session/run IDs и tool metadata. Не публикуйте Jaeger без доступа только для оператора.
- Не используйте SSH tunnels как штатный deployment path. Нормальный путь — Caddy, domain/TLS, firewall и явная auth.

## Почему `agentd` пока не в Docker

На текущем этапе `agentd` оставлен host service:

- tools должны работать с host workspace;
- `exec_*` должен запускать реальные команды в ожидаемой среде;
- systemd lifecycle уже понятен оператору;
- не нужно проектировать отдельную модель bind mounts, прав и docker socket access.

Если позже переносить `agentd` в Docker, это отдельное архитектурное решение: надо явно описать workspace mounts, UID/GID, доступ к host tools, artifacts/state и security boundary.

## Внешние источники

- Docker Engine install: <https://docs.docker.com/engine/install/ubuntu/>
- SearXNG Docker install: <https://docs.searxng.org/admin/installation-docker.html>
- SearXNG reverse proxy subpath header `X-Script-Name`: <https://docs.searxng.org/admin/installation-nginx.html>
- SearXNG MCP example project: <https://github.com/ihor-sokoliuk/mcp-searxng>
- Logseq: <https://logseq.com/>
- Logseq Publish SPA image used by deploy script: <https://github.com/l-trump/logseq-publish-spa>
- SilverBullet: <https://silverbullet.md/>
- LinuxServer Obsidian Docker image: <https://github.com/linuxserver/docker-obsidian>
- MCPVault filesystem-backed Obsidian MCP: <https://github.com/bitbonsai/mcpvault>
- Obsidian CLI skill: <https://github.com/kepano/obsidian-skills/blob/main/skills/obsidian-cli/SKILL.md>
- Jaeger getting started: <https://www.jaegertracing.io/docs/latest/getting-started/>
- Jaeger deployment/docker: <https://www.jaegertracing.io/docs/latest/deployment/>
- OpenTelemetry OTLP protocol: <https://opentelemetry.io/docs/specs/otlp/>
