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

## Obsidian

Obsidian включается явно:

```bash
./scripts/deploy-teamd-containers.sh --with-obsidian
```

Default paths:

- vaults: `/var/lib/teamd/vaults`;
- container config: `/var/lib/teamd/containers/obsidian/config`;
- local URL: `http://127.0.0.1:8080`.

Идея для первого этапа:

- оператор редактирует заметки в браузере;
- agentd получает доступ к vault как к обычным файлам или через будущий MCP/skill слой;
- Obsidian не становится частью prompt path автоматически.

Про `obsidian-cli` skill можно смотреть здесь: <https://github.com/kepano/obsidian-skills/blob/main/skills/obsidian-cli/SKILL.md>.

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
