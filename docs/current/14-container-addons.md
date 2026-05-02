# Container add-ons: Docker, SearXNG, SilverBullet, Lightpanda, Jaeger, Caddy

Этот документ описывает второй deploy layer вокруг host `agentd`.

Главный принцип: контейнеры дают внешнюю обвязку, но не создают второй agent runtime. `agentd` остаётся host systemd service и продолжает владеть sessions, runs, tools, schedules, Telegram delivery, SQLite state, artifacts и audit logs.

## Что ставит deploy script

Скрипт:

```bash
./scripts/deploy-teamd-containers.sh
```

По умолчанию поднимает:

- `teamd-searxng` — локальный search backend для `web_search`;
- `teamd-caddy` — edge reverse proxy.

Опционально:

- `teamd-silverbullet` — browser UI для canonical Markdown knowledge space;
- `teamd-silverbullet-mcp` — SilverBullet MCP bridge;
- `teamd-jaeger` — Jaeger UI и OTLP receiver для traces;
- `lightpanda` MCP connector — optional headless browser для JS-страниц, форм, кликов и DOM/content extraction;
- `teamd-obsidian` — legacy browser Obsidian для восстановления старых vault workflows;
- `obsidian` MCP connector — legacy filesystem-backed MCP для старого vault.

Logseq Publish больше не является runtime-компонентом. Deploy script удаляет legacy containers `teamd-logseq-publish` и `logseq-publish`, если они остались на хосте. Старые Markdown-файлы не удаляются: путь `/var/lib/teamd/knowledge/logseq/teamd` используется только как migration source при первом создании SilverBullet Space.

## Recommended install

Основной production вариант:

```bash
./scripts/deploy-teamd-containers.sh --with-silverbullet-mcp --with-jaeger --single-domain
```

Если нужен браузерный MCP add-on для динамических страниц:

```bash
./scripts/deploy-teamd-containers.sh --with-lightpanda-mcp
```

Если нужен только Lightpanda без контейнерной обвязки:

```bash
./scripts/deploy-teamd-containers.sh --no-searxng --no-caddy --with-lightpanda-mcp
```

Если нужен только SilverBullet без MCP:

```bash
./scripts/deploy-teamd-containers.sh --with-silverbullet
```

Dry-run без изменений на сервере:

```bash
./scripts/deploy-teamd-containers.sh --dry-run --non-interactive --no-start --with-silverbullet-mcp
```

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
- `state.sqlite`.

Эти данные остаются в `agentd`.

## Migration from old Logseq graph

По умолчанию legacy source:

```text
/var/lib/teamd/knowledge/logseq/teamd
```

При запуске `--with-silverbullet` или `--with-silverbullet-mcp` deploy script:

1. создаёт `/var/lib/teamd/knowledge/silverbullet/teamd`;
2. если legacy Logseq graph существует и новый SilverBullet Space пустой, копирует содержимое legacy graph в новый space;
3. выставляет ownership под `teamd`;
4. оставляет legacy директорию на диске как backup/migration source.

Legacy source можно переопределить:

```bash
TEAMD_LEGACY_LOGSEQ_GRAPH_DIR='/path/to/old/graph' \
  ./scripts/deploy-teamd-containers.sh --with-silverbullet-mcp
```

Повторный запуск не перетирает уже заполненный SilverBullet Space.

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

## Agent skill

Built-in default agent получает основной skill:

```text
silverbullet-space
```

Путь в agent home:

```text
/var/lib/teamd/state/agents/default/skills/silverbullet-space/SKILL.md
```

Включить вручную:

```bash
teamdctl session enable-skill <session_id> silverbullet-space
teamdctl session skills <session_id>
```

`logseq-graph` и `obsidian-vault` остаются только как deprecated compatibility shims. Если старый prompt или операторская команда активирует их, они должны отправить агента в `silverbullet-space`.

## Caddy routes

Без dedicated domain:

- SearXNG: `http://127.0.0.1:8088/searxng/`;
- Jaeger через Caddy: `http://127.0.0.1:8088/jaeger/`, если включён `--with-jaeger`;
- SilverBullet: `https://<host>:8444/`, если включён `--with-silverbullet` или `--with-silverbullet-mcp`;
- legacy Obsidian: `https://<host>:8443/obsidian/`, если включён `--with-obsidian`.

С dedicated domain:

```bash
TEAMD_CADDY_DOMAIN='example.com' \
  ./scripts/deploy-teamd-containers.sh --with-silverbullet-mcp --with-jaeger
```

Routes:

- `https://search.example.com/` -> SearXNG;
- `https://notes.example.com/` -> SilverBullet;
- `https://jaeger.example.com/` -> Jaeger, если включён;
- `https://obsidian.example.com/` -> legacy Obsidian, если включён.

Single-domain mode:

```bash
TEAMD_CADDY_DOMAIN='teamd.qlbc.ru' \
  ./scripts/deploy-teamd-containers.sh --with-silverbullet-mcp --with-jaeger --single-domain
```

Routes:

- `https://teamd.qlbc.ru/` -> SilverBullet;
- `https://teamd.qlbc.ru/searxng/` -> SearXNG;
- `https://teamd.qlbc.ru/jaeger/` -> Jaeger;
- `https://teamd.qlbc.ru/obsidian/` -> legacy Obsidian, если включён.

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

## Lightpanda

Lightpanda — optional MCP-first браузерный add-on. Он нужен, когда обычных `web_search` и `web_fetch` недостаточно:

- страница рендерится JavaScript;
- нужен переход по ссылкам, click/fill/scroll/wait;
- нужно достать semantic DOM, markdown view, links или structured data после загрузки страницы;
- нужно проверить форму или интерактивный flow без полноценного screenshot/browser UI.

Он не заменяет канонические `web_search` и `web_fetch`:

- поиск источников по-прежнему начинается с `web_search`;
- прямой fetch известного URL по-прежнему делает `web_fetch`;
- Lightpanda включается как discovered MCP tools через общий provider/tool loop;
- для Lightpanda не создаётся отдельный prompt path, отдельный daemon или второй web extraction loop.

Команды:

```bash
./scripts/deploy-teamd-containers.sh --with-lightpanda
./scripts/deploy-teamd-containers.sh --with-lightpanda-mcp
./scripts/deploy-teamd-containers.sh --with-lightpanda-mcp-example
```

Deploy script ставит:

```text
/opt/teamd/bin/lightpanda
/usr/local/bin/lightpanda
/opt/teamd/containers/lightpanda/lightpanda-mcp-stdio.sh
/opt/teamd/containers/lightpanda/lightpanda-mcp.example.toml
```

`--with-lightpanda-mcp` добавляет enabled connector в `/etc/teamd/config.toml`:

```toml
[daemon.mcp_connectors.lightpanda]
transport = "stdio"
command = "/opt/teamd/containers/lightpanda/lightpanda-mcp-stdio.sh"
args = []
enabled = true
```

Wrapper запускает:

```bash
/opt/teamd/bin/lightpanda mcp
```

и по умолчанию выставляет:

```bash
LIGHTPANDA_DISABLE_TELEMETRY=true
```

Release tag и download URL можно переопределить:

```bash
TEAMD_LIGHTPANDA_RELEASE_TAG='nightly' \
  ./scripts/deploy-teamd-containers.sh --with-lightpanda-mcp

TEAMD_LIGHTPANDA_DOWNLOAD_URL='https://example.invalid/lightpanda' \
  ./scripts/deploy-teamd-containers.sh --with-lightpanda-mcp
```

Agent skill:

```text
lightpanda-browser
```

Включить вручную:

```bash
teamdctl session enable-skill <session_id> lightpanda-browser
teamdctl session skills <session_id>
```

Ожидаемая модель работы агента:

1. Найти candidate URL через `web_search`, если URL не задан пользователем.
2. Открыть выбранный URL через discovered Lightpanda MCP tool вроде `mcp__lightpanda__goto`.
3. Снять markdown/semantic tree/links/structured data через discovered MCP tools.
4. Для интерактива использовать discovered tools вроде click/fill/scroll/waitForSelector.
5. Если connector недоступен, явно сказать об этом и не выдумывать browser result.

Важно: Lightpanda сейчас стоит как nightly/beta browser binary. Это нормальный add-on для automation, но не гарантия, что каждая публичная страница примет его как обычный Chrome.

## Jaeger

`--with-jaeger` поднимает `teamd-jaeger` и включает best-effort OTLP export:

```bash
TEAMD_OTLP_EXPORT_ENABLED='true'
TEAMD_OTLP_ENDPOINT='http://127.0.0.1:4318/v1/traces'
TEAMD_OTLP_TIMEOUT_MS='2000'
```

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

## Legacy Obsidian

Obsidian остаётся только legacy/recovery path.

Команды:

```bash
./scripts/deploy-teamd-containers.sh --with-obsidian
./scripts/deploy-teamd-containers.sh --with-obsidian-mcp
```

Canonical legacy vault:

```text
/var/lib/teamd/vaults/teamd
```

Compatibility symlink:

```text
/var/lib/teamd/vault -> /var/lib/teamd/vaults/teamd
```

Новые knowledge notes должны идти в SilverBullet Space, а не в Obsidian vault.

## Security model

- SilverBullet защищается `SB_USER`.
- SearXNG и Jaeger в этой схеме не имеют пользовательской авторизации. Не публикуйте их наружу без reverse-proxy auth/firewall/VPN, если сервер доступен не только вам.
- MCP wrappers требуют Docker access для `teamd`, потому что `agentd` запускает stdio bridge через Docker. Это сильное право; выдавайте его только trusted runtime user.
- Lightpanda MCP wrapper запускает локальный browser binary от имени runtime user. Не используйте его для обхода access controls и не передавайте ему секреты страниц без явного намерения.
- Secrets лежат в env files под `/opt/teamd/containers/*/*.env`, а не в git.

## Проверка после deploy

Контейнеры:

```bash
docker ps --format 'table {{.Names}}\t{{.Status}}\t{{.Ports}}'
```

SilverBullet:

```bash
curl -I http://127.0.0.1:8091/
ls -la /var/lib/teamd/knowledge/silverbullet/teamd
```

MCP config:

```bash
grep -A5 'daemon.mcp_connectors.silverbullet' /etc/teamd/config.toml
grep -A5 'daemon.mcp_connectors.lightpanda' /etc/teamd/config.toml
systemctl restart teamd-daemon teamd-telegram
```

Legacy Logseq runtime должен отсутствовать:

```bash
docker ps -a --format '{{.Names}}' | grep -E 'logseq|Logseq' || true
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

Если Lightpanda MCP не появился в tools:

1. проверьте `/etc/teamd/config.toml`;
2. проверьте `/opt/teamd/containers/lightpanda/lightpanda-mcp-stdio.sh`;
3. проверьте `lightpanda --help` и `lightpanda mcp`;
4. перезапустите `teamd-daemon` и `teamd-telegram`.

## Ссылки

- SilverBullet: <https://silverbullet.md/>
- SilverBullet community MCP: <https://github.com/Ahmad-A0/silverbullet-mcp>
- Lightpanda browser: <https://github.com/lightpanda-io/browser>
- Lightpanda docs: <https://lightpanda.io/docs>
- SearXNG: <https://docs.searxng.org/>
- Jaeger: <https://www.jaegertracing.io/>
- Caddy: <https://caddyserver.com/docs/>
