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

Obsidian включается явно:

```bash
./scripts/deploy-teamd-containers.sh --with-obsidian
```

Default paths:

- vaults: `/var/lib/teamd/vaults`;
- container config: `/var/lib/teamd/containers/obsidian/config`;
- local URL: `http://127.0.0.1:8080`.

В этой схеме Obsidian — это внешний UI для человека. Оператор открывает его в браузере, редактирует vault и управляет плагинами. `agentd` не встраивает Obsidian в prompt path автоматически.

Если включён Caddy, нормальный доступ выглядит так:

```bash
TEAMD_CADDY_DOMAIN='example.com' ./scripts/deploy-teamd-containers.sh --with-obsidian
```

После этого web UI доступен как `obsidian.example.com`.

## Obsidian: доступ агента через Local REST API + MCP

Первый поддерживаемый вариант для агента:

```text
agentd -> stdio MCP connector -> docker run obsidian-mcp -> Obsidian Local REST API -> vault
```

Почему так:

- Obsidian остаётся в Docker и доступен оператору через web UI;
- Local REST API plugin живёт внутри Obsidian;
- MCP server превращает Obsidian REST API в MCP tools/resources;
- текущий `agentd` поддерживает MCP transport только `stdio`, поэтому MCP запускается как дочерний процесс `docker run -i --rm`, а не как постоянный HTTP/SSE sidecar.

Сгенерировать пример коннектора:

```bash
./scripts/deploy-teamd-containers.sh --with-obsidian-mcp-example
```

Скрипт создаёт:

```text
/opt/teamd/containers/obsidian/obsidian-mcp.example.toml
/opt/teamd/containers/obsidian/obsidian-mcp.env.example
```

Дальше порядок такой:

1. Откройте Obsidian web UI.
2. Установите и включите community plugin `Local REST API`.
3. Скопируйте API key из настроек plugin.
4. Создайте runtime env file:

```bash
sudo install -m 0640 -o root -g teamd \
  /opt/teamd/containers/obsidian/obsidian-mcp.env.example \
  /etc/teamd/obsidian-mcp.env
sudoedit /etc/teamd/obsidian-mcp.env
```

Внутри `/etc/teamd/obsidian-mcp.env` должны быть значения вида:

```text
API_KEY=replace-with-local-rest-api-key
API_URLS=["https://127.0.0.1:27124","http://127.0.0.1:27123"]
VERIFY_SSL=false
```

`API_URLS` указывает адреса Local REST API с точки зрения MCP-контейнера. Пример запускает MCP-контейнер с `--network container:teamd-obsidian`, поэтому `127.0.0.1` — это network namespace контейнера Obsidian, а не host. Контейнер `teamd-obsidian` должен быть запущен. REST API не публикуется наружу отдельным портом.

Затем перенесите блок из:

```text
/opt/teamd/containers/obsidian/obsidian-mcp.example.toml
```

в `/etc/teamd/config.toml` и поменяйте:

```toml
enabled = true
```

Перезапустите сервисы:

```bash
sudo systemctl restart teamd-daemon.service teamd-telegram.service
```

Проверка через TUI/REPL:

```bash
teamdctl tui
```

Дальше используйте `\mcp`, чтобы увидеть коннектор, или попросите агента найти/прочитать заметку через MCP tools.

### Важное ограничение Docker/MCP

Такой коннектор требует, чтобы systemd-пользователь `teamd` мог выполнить `docker run ...`. Самый простой способ — добавить `teamd` в группу `docker`, но это почти root-level право. Поэтому deploy script только пишет пример и не включает его автоматически.

Более строгий вариант на будущее:

- добавить в `agentd` MCP transport `streamable-http`/SSE;
- держать Obsidian MCP как отдельный long-running container;
- подключать его по HTTP с bearer token;
- не давать `teamd` прямой доступ к Docker socket.

### Skill, MCP и CLI

В этой схеме `obsidian-cli` не обязателен: MCP server работает через Local REST API plugin. Skill остаётся полезным как слой инструкций для агента: как называть заметки, как искать, как писать daily notes, как не ломать структуру vault.

Отдельный `obsidian-cli` path можно добавить позже, если понадобится именно CLI workflow. Его надо проектировать отдельно, чтобы не создать второй скрытый tool loop. Ориентир по skill: <https://github.com/kepano/obsidian-skills/blob/main/skills/obsidian-cli/SKILL.md>.

## Caddy

Без домена Caddy слушает local port:

```text
http://127.0.0.1:8088
```

Routes:

- `/searxng/`;
- `/obsidian/`.

Этот path mode полезен для локального smoke. Для нормального browser usage лучше задать домен:

```bash
TEAMD_CADDY_DOMAIN='example.com' ./scripts/deploy-teamd-containers.sh --with-obsidian
```

Тогда Caddy создаёт:

- `search.example.com`;
- `obsidian.example.com`.

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
- SearXNG MCP example project: <https://github.com/ihor-sokoliuk/mcp-searxng>
- Obsidian CLI skill: <https://github.com/kepano/obsidian-skills/blob/main/skills/obsidian-cli/SKILL.md>
- Obsidian MCP via Local REST API: <https://github.com/OleksandrKucherenko/mcp-obsidian-via-rest>
- Obsidian MCP Docker image docs: <https://hub.docker.com/r/oleksandrkucherenko/obsidian-mcp>
