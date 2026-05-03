# Agent Browser + Browserless Design

## Контекст

Lightpanda MCP оказался полезным для простых DOM-задач, но на реальных публичных сайтах может ломаться из-за неполной совместимости browser API. Для production browser automation нужен настоящий Chrome/Chromium, при этом нельзя создавать второй chat loop, второй prompt path или скрытый shell-snippet слой.

## Решение

teamD добавляет канонические built-in tools семейства `browser_*`. Эти tools проходят через тот же provider loop, permission check, tool ledger, artifact/offload, debug UI и Telegram/TUI surfaces, что и остальные tools.

Исполнение делается через `agent-browser` CLI, потому что он даёт agent-friendly workflow: `open`, `snapshot`, refs `@eN`, `click`, `fill`, `wait`, `screenshot`, `pdf`, `close`. Browserless используется как browser provider для реального Chromium/Chrome и не становится отдельным runtime.

Поток:

```text
Model
  -> provider tool call browser_*
  -> ExecutionService / ToolRuntime
  -> BrowserToolClient
  -> agent-browser CLI
  -> Browserless container or Browserless SaaS
  -> Chrome/Chromium
```

## Контракт tools

Минимальный v1 surface:

- `browser_open({ url, wait_until? })`
- `browser_snapshot({ interactive?, compact?, depth?, selector?, max_chars? })`
- `browser_text({ selector?, max_chars? })`
- `browser_click({ selector, wait_until? })`
- `browser_fill({ selector, text })`
- `browser_press({ key })`
- `browser_wait({ kind, value?, state? })`
- `browser_scroll({ direction, pixels? })`
- `browser_eval({ script, max_chars? })`
- `browser_screenshot({ path?, full?, annotate? })`
- `browser_pdf({ path })`
- `browser_status({})`
- `browser_close({ all? })`

`selector` может быть `@eN` из последнего snapshot, CSS selector или locator, который поддерживает agent-browser. После действий, меняющих страницу, модель должна вызвать `browser_snapshot` заново.

## Конфигурация

Новый `[browser]` config управляет включением tools и CLI:

```toml
[browser]
enabled = false
command = "/opt/teamd/bin/agent-browser"
provider = "browserless"
session_prefix = "teamd"
default_timeout_ms = 30000
max_output_chars = 20000

[browser.browserless]
api_url = "http://127.0.0.1:3000"
api_key = ""
browser_type = "chromium"
ttl_ms = 300000
stealth = true
```

Env overrides:

- `TEAMD_BROWSER_ENABLED`
- `TEAMD_BROWSER_COMMAND`
- `TEAMD_BROWSER_PROVIDER`
- `TEAMD_BROWSER_SESSION_PREFIX`
- `TEAMD_BROWSER_DEFAULT_TIMEOUT_MS`
- `TEAMD_BROWSER_MAX_OUTPUT_CHARS`
- `TEAMD_BROWSERLESS_API_URL`
- `TEAMD_BROWSERLESS_API_KEY`
- `TEAMD_BROWSERLESS_BROWSER_TYPE`
- `TEAMD_BROWSERLESS_TTL_MS`
- `TEAMD_BROWSERLESS_STEALTH`

## Isolation

Каждая teamD session получает отдельный `AGENT_BROWSER_SESSION`:

```text
<session_prefix>-<sanitized_session_id>
```

Это защищает параллельные Telegram/TUI sessions от общей browser state. `browser_close({ all: true })` остаётся доступным, но модель должна предпочитать закрытие только текущей session.

## Deploy

`scripts/deploy-teamd-containers.sh` получает Browserless add-on:

- Docker container `teamd-browserless`;
- image `ghcr.io/browserless/chromium`;
- local binding `127.0.0.1:3000`;
- generated token in `/opt/teamd/containers/browserless/browserless.env`;
- generated `/etc/teamd/config.toml` blocks `[browser]` и `[browser.browserless]`;
- install/update `agent-browser` under `/opt/teamd/bin/agent-browser`.

Browserless не публикуется наружу через Caddy по умолчанию. Это инфраструктурный endpoint для `agentd`, а не операторский UI.

## Ошибки и диагностика

Ошибки CLI возвращаются как обычные tool failures и попадают в session tool ledger. В debug UI оператор видит:

- имя tool;
- arguments;
- command summary без секретов;
- stdout/stderr preview;
- result path для screenshot/pdf.

Большие `browser_snapshot`, `browser_text` и `browser_eval` outputs offload-ятся через существующий artifact mechanism.

## Источники

- agent-browser README: https://github.com/vercel-labs/agent-browser
- agent-browser Browserless provider: https://github.com/vercel-labs/agent-browser/blob/main/CHANGELOG.md
- Browserless connection URLs: https://docs.browserless.io/overview/connection-urls
- Browserless Docker open-source deployment: https://docs.browserless.io/enterprise/open-source
