# Container add-ons: Docker, SearXNG, Obsidian, Caddy

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

- `teamd-obsidian` — browser-accessible Obsidian container, если передать `--with-obsidian`.
- filesystem-backed Obsidian MCP connector для `agentd`, если передать `--with-obsidian-mcp`.

Проверить действия без изменений:

```bash
./scripts/deploy-teamd-containers.sh --dry-run --non-interactive --no-start --with-obsidian
```

## SearXNG для `web_search`

Скрипт поднимает SearXNG на localhost:

```text
http://127.0.0.1:8888
```

Проверка JSON API:

```bash
curl 'http://127.0.0.1:8888/search?q=test&format=json'
```

Чтобы `agentd web_search` использовал SearXNG, добавьте в `/etc/teamd/teamd.env`:

```bash
TEAMD_WEB_SEARCH_BACKEND='searxng_json'
TEAMD_WEB_SEARCH_URL='http://127.0.0.1:8888/search'
```

Потом перезапустите сервисы:

```bash
sudo systemctl restart teamd-daemon.service teamd-telegram.service
```

Если вы редактируете TOML вместо env:

```toml
[web]
search_backend = "searxng_json"
search_url = "http://127.0.0.1:8888/search"
```

`web_fetch` остаётся прямым HTTP fetch tool. Он не ходит через SearXNG, потому что SearXNG — поисковый backend, а не универсальный proxy.

## MCP для SearXNG

Скрипт также пишет пример MCP-конфига:

```text
/opt/teamd/containers/searxng/mcp-searxng.example.json
```

Это не включает MCP автоматически. Это шаблон для подключения `mcp-searxng` как отдельного MCP connector, если нужен search как MCP capability.

Ориентир по проекту: <https://github.com/ihor-sokoliuk/mcp-searxng>.

## Obsidian: web UI для оператора

Obsidian UI включается явно:

```bash
./scripts/deploy-teamd-containers.sh --with-obsidian
```

Default paths:

- vaults: `/var/lib/teamd/vaults`;
- managed vault: `/var/lib/teamd/vaults/teamd`;
- compatibility path для агентов, которые ошибочно пишут в `~/vault`: `/var/lib/teamd/vault -> /var/lib/teamd/vaults/teamd`;
- container config: `/var/lib/teamd/containers/obsidian/config`;
- local URL: `http://127.0.0.1:8080/obsidian/`.

В этой схеме Obsidian — это внешний UI для человека. Оператор открывает его в браузере и редактирует vault. `agentd` не встраивает Obsidian в prompt path автоматически.

Канонический путь vault для всех агентов и операторских команд — `/var/lib/teamd/vaults/teamd`. Путь `/var/lib/teamd/vault` существует только как совместимость с ошибочным `~/vault`, потому что production user `teamd` имеет home `/var/lib/teamd`. Production `agentd` запускается с `WorkingDirectory=/var/lib/teamd`, поэтому workspace-relative path `vault/...` тоже попадает в этот же vault через symlink. Новые инструкции, skills и tooling должны считать canonical source of truth путём `/var/lib/teamd/vaults/teamd`.

Без отдельного домена скрипт запускает Obsidian в subfolder mode:

```text
SUBFOLDER=/obsidian/
```

Важно: у образа `ghcr.io/sytone/obsidian-remote` subfolder должен начинаться и заканчиваться `/`. Значение `obsidian` без слэшей ломает web route. Caddy в этом режиме не срезает `/obsidian/`, а прокидывает путь как есть.

Если включён Caddy, нормальный доступ выглядит так:

```bash
TEAMD_CADDY_DOMAIN='example.com' ./scripts/deploy-teamd-containers.sh --with-obsidian
```

После этого web UI доступен как `obsidian.example.com`.

## Obsidian: доступ агента через MCP

Первый поддерживаемый вариант для агента:

```text
agentd -> stdio MCP connector -> docker run node -> @bitbonsai/mcpvault -> vault
```

Почему так:

- Obsidian остаётся в Docker и доступен оператору через web UI;
- агент работает не generic filesystem write tools, а через Obsidian-aware MCP tools;
- MCP server работает напрямую с vault directory и не зависит от того, открыт ли Obsidian UI;
- не нужен Obsidian Local REST API plugin и не нужен ручной клик в GUI для включения plugin;
- текущий `agentd` поддерживает MCP transport только `stdio`, поэтому MCP запускается как дочерний процесс `docker run -i --rm`, а не как постоянный HTTP/SSE sidecar.

Полностью автоматический путь:

```bash
./scripts/deploy-teamd-containers.sh --with-obsidian-mcp
```

Он делает всё, что нужно для первого запуска:

- создаёт managed vault `/var/lib/teamd/vaults/teamd`;
- seed'ит Obsidian vault registry в `/var/lib/teamd/containers/obsidian/config/.config/obsidian/obsidian.json`;
- добавляет или заменяет enabled MCP connector `[daemon.mcp_connectors.obsidian]` в `/etc/teamd/config.toml`;
- connector запускает `docker run -i --rm -v /var/lib/teamd/vaults/teamd:/vault:rw docker.io/library/node:22-alpine npx -y @bitbonsai/mcpvault@latest /vault`;
- добавляет systemd-пользователя `teamd` в группу `docker`, чтобы `agentd` мог запускать stdio MCP через `docker run`;
- перезапускает `teamd-daemon.service` и `teamd-telegram.service`, если они существуют и не указан `--no-start`.

Проверка без изменений:

```bash
./scripts/deploy-teamd-containers.sh --dry-run --non-interactive --no-start --with-obsidian-mcp
```

Ручной fallback — только сгенерировать пример коннектора:

```bash
./scripts/deploy-teamd-containers.sh --with-obsidian-mcp-example
```

Скрипт создаёт:

```text
/opt/teamd/containers/obsidian/obsidian-mcp.example.toml
```

Для ручного fallback порядок такой:

1. Скопируйте блок из:

```text
/opt/teamd/containers/obsidian/obsidian-mcp.example.toml
```

2. Вставьте его в `/etc/teamd/config.toml`.
3. Поменяйте:

```toml
enabled = true
```

4. Перезапустите сервисы:

```bash
sudo systemctl restart teamd-daemon.service teamd-telegram.service
```

Проверка через TUI/REPL:

```bash
teamdctl tui
```

Дальше используйте `\mcp`, чтобы увидеть коннектор, или попросите агента найти/прочитать заметку через MCP tools.

Нормальный агентский flow:

- сначала `mcp_search_resources` или прямой вызов обнаруженного MCP tool;
- затем `mcp__obsidian__read_note`, `mcp__obsidian__search_notes`, `mcp__obsidian__write_note` или другие exposed tools, которые вернул connector;
- перед изменением существующей заметки агент читает её через MCP;
- generic `fs_write_text`/`fs_patch_text` для vault — только аварийный fallback, если MCP недоступен и оператор явно согласился.

## Obsidian vault skill и PARA contract

Default agent получает встроенный agent-local skill:

```text
/var/lib/teamd/state/agents/default/skills/obsidian-vault/SKILL.md
```

Skill активируется автоматически, когда в сессии есть контекст про `Obsidian`, `vault`, `PARA`, `projects`, `areas`, `resources`, `archive`, `notes`, `knowledge base`, Markdown notes, daily notes, tasks, links или frontmatter. Его также можно включить вручную:

```bash
teamdctl session enable-skill <session_id> obsidian-vault
teamdctl session skills <session_id>
```

Итоговый контракт skill:

- агент работает с vault через `obsidian` MCP connector first;
- агент не использует generic filesystem write tools для нормальной работы с заметками;
- filesystem fallback допустим только для аварийной/admin-операции, если MCP недоступен и оператор явно согласился;
- перед изменением существующей заметки агент сначала читает её;
- после успешного write/update агент сообщает, что именно изменил и где;
- если tool call упал, агент не утверждает, что заметка сохранена.

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

Tags and Obsidian syntax:

- use tags sparingly: `#project`, `#area`, `#resource`, `#task`, `#daily`, `#inbox`, `#archive`;
- priority tags: `#p0`, `#p1`, `#p2`, `#p3`, only when priority matters;
- prefer wikilinks like `[[note name]]`;
- use checkboxes `- [ ]` and `- [x]`;
- use callouts for important blocks: `> [!note]`, `> [!warning]`, `> [!decision]`;
- preserve embeds `![[...]]`, links, aliases, headings and frontmatter.

Operating rules:

- не удалять и не архивировать user material без запроса или явного основания в note;
- не выдумывать completed tasks, sources, dates или decisions;
- при неоднозначном target folder выбрать ближайший PARA folder и явно назвать assumption;
- имена notes должны быть стабильными и читаемыми; timestamp-only filenames допустимы только для daily notes;
- если пользовательское сообщение содержит durable fact, decision, task или resource, агент должен предложить сохранить это или сохранить сразу, если запрос подразумевает persistence.

Быстрая проверка на production host:

```bash
teamdctl session skills <session_id>
grep -nE 'PARA structure|04-Archive|Templates' \
  /var/lib/teamd/state/agents/default/skills/obsidian-vault/SKILL.md
curl -fsS http://127.0.0.1:5140/v1/mcp/connectors
```

### Важное ограничение Docker/MCP

Такой коннектор требует, чтобы systemd-пользователь `teamd` мог выполнить `docker run ...`. Автоматический режим `--with-obsidian-mcp` добавляет `teamd` в группу `docker`. Это почти root-level право, потому что доступ к Docker socket фактически позволяет управлять host'ом. Если это неприемлемо, используйте `--with-obsidian-mcp-example` и настройте более узкий wrapper/transport вручную.

Более строгий вариант на будущее:

- добавить в `agentd` MCP transport `streamable-http`/SSE;
- держать Obsidian MCP как отдельный long-running container;
- подключать его по HTTP с bearer token;
- не давать `teamd` прямой доступ к Docker socket.

### Skill, MCP и CLI

В этой схеме `obsidian-cli` не обязателен: MCP server уже даёт semantic tools для read/write/search/update frontmatter. Skill остаётся полезным как слой инструкций для агента: как называть заметки, как искать, как писать daily notes, как не ломать структуру vault.

Отдельный `obsidian-cli` path можно добавить позже, если понадобится именно CLI workflow. Его надо проектировать отдельно, чтобы не создать второй скрытый tool loop. Ориентир по skill: <https://github.com/kepano/obsidian-skills/blob/main/skills/obsidian-cli/SKILL.md>.

## Caddy

Без домена Caddy слушает local port:

```text
http://127.0.0.1:8088
```

Routes:

- `/searxng/`;
- `/obsidian/`.

В path mode `/searxng/` прокидывается без срезания префикса и с upstream header `X-Script-Name: /searxng`. Это важно: SearXNG генерирует root-relative ссылки и `form action`; без `X-Script-Name` browser UI уходит на `/search`, `/static/...` и фактически выпадает из `/searxng/`. Для `/searxng` без trailing slash Caddy делает redirect на `/searxng/`.

`/obsidian/` тоже прокидывается без срезания префикса, потому что Obsidian container сам запущен с `SUBFOLDER=/obsidian/`.

После записи Caddyfile deploy script пересоздаёт `teamd-caddy` через `docker compose up -d --force-recreate`, затем делает `caddy reload`. Это важно: Caddyfile смонтирован как отдельный bind-mounted file, а атомарная замена файла на host может оставить уже запущенный контейнер на старом inode.

Для нормального browser usage можно задать домен:

```bash
TEAMD_CADDY_DOMAIN='example.com' ./scripts/deploy-teamd-containers.sh --with-obsidian
```

Тогда Caddy создаёт:

- `search.example.com`;
- `obsidian.example.com`.

## Obsidian web UI и мобильный браузер

Текущий контейнер Obsidian (`ghcr.io/sytone/obsidian-remote`) публикует desktop Obsidian через browser/VNC-подобный web UI. Это рабочий вариант для desktop browser, но не полноценный мобильный web client. На телефоне интерфейс мелкий, жесты и ввод неудобны, и это ограничение выбранного контейнера, а не ошибка Caddy.

Принятый mobile workflow на текущем этапе:

- оператор пишет агенту в Telegram;
- агент создаёт и обновляет заметки через Obsidian MCP connector, который смонтирован на `/var/lib/teamd/vaults/teamd`;
- Obsidian web UI остаётся desktop/admin-интерфейсом для проверки vault, включения plugins и ручного редактирования с большого экрана.

Если нужен прямой mobile UI для заметок, это следующий отдельный слой поверх vault:

- оставить Obsidian container как desktop/admin UI;
- добавить web UI для Markdown vault, который рассчитан на мобильный браузер;
- либо перейти на dedicated domain/TLS/auth и использовать нативный Obsidian-клиент с синхронизацией vault вне этого контейнера.

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
- Obsidian remote Docker image: <https://github.com/sytone/obsidian-remote>
- MCPVault filesystem-backed Obsidian MCP: <https://github.com/bitbonsai/mcpvault>
- Obsidian CLI skill: <https://github.com/kepano/obsidian-skills/blob/main/skills/obsidian-cli/SKILL.md>
