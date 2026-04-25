# Шпаргалка оператора

## Минимальный старт

```bash
agentd version
agentd status
agentd tui
```

Если бинарь запускается из репозитория:

```bash
cargo run -p agentd -- version
cargo run -p agentd -- tui
```

## Telegram быстрый старт

Полная инструкция: [telegram/01-install-and-configure.md](telegram/01-install-and-configure.md).

Самый короткий production-like путь из checkout:

```bash
./scripts/deploy-teamd.sh
```

Скрипт проверит native build dependencies, `cargo` и `rustc`; при необходимости поставит системные build-пакеты и stable Rust через `rustup`; интерактивно спросит Telegram bot token и Z.ai/API key, соберёт release binary, установит `/opt/teamd/bin/agentd`, зарегистрирует `agentd` в `PATH` через `/usr/local/bin/agentd`, установит операторский helper `/usr/local/bin/teamdctl`, создаст `/etc/teamd/config.toml`, `/etc/teamd/teamd.env` и два systemd service.

Проверить действия без установки:

```bash
TEAMD_TELEGRAM_BOT_TOKEN='123456789:test-token' \
  TEAMD_PROVIDER_API_KEY='zai-test-key' \
  ./scripts/deploy-teamd.sh --dry-run --non-interactive --no-build --no-start
```

Запретить автоустановку Rust:

```bash
./scripts/deploy-teamd.sh --no-install-rust
```

Запретить автоустановку системных build dependencies:

```bash
./scripts/deploy-teamd.sh --no-install-system-deps
```

## Container add-ons

Полная инструкция: [14-container-addons.md](14-container-addons.md).

Core `agentd` ставится отдельно от контейнерной обвязки. Второй скрипт поднимает SearXNG/Caddy и опционально Obsidian:

```bash
./scripts/deploy-teamd-containers.sh
./scripts/deploy-teamd-containers.sh --with-obsidian
./scripts/deploy-teamd-containers.sh --with-obsidian-mcp
./scripts/deploy-teamd-containers.sh --with-obsidian-mcp-example
```

Проверка без изменений:

```bash
./scripts/deploy-teamd-containers.sh --dry-run --non-interactive --no-start --with-obsidian
./scripts/deploy-teamd-containers.sh --dry-run --non-interactive --no-start --with-obsidian-mcp
```

`--with-obsidian-mcp` ставит Obsidian, Local REST API plugin, `/etc/teamd/obsidian-mcp.env`, enabled MCP connector в `/etc/teamd/config.toml` и перезапускает `teamd` сервисы, если они уже установлены.

`--with-obsidian-mcp-example` дополнительно пишет шаблон MCP connector для Obsidian Local REST API:

```text
/opt/teamd/containers/obsidian/obsidian-mcp.example.toml
/opt/teamd/containers/obsidian/obsidian-mcp.env.example
```

Чтобы переключить встроенный `web_search` на SearXNG, добавьте в `/etc/teamd/teamd.env`:

```bash
TEAMD_WEB_SEARCH_BACKEND='searxng_json'
TEAMD_WEB_SEARCH_URL='http://127.0.0.1:8888/search'
```

и перезапустите:

```bash
sudo systemctl restart teamd-daemon.service teamd-telegram.service
```

Минимальный набор:

```bash
git clone https://github.com/galiakhmetovc/scaling-lamp.git
cd scaling-lamp
cargo build --release -p agentd
mkdir -p ~/.config/teamd
cp config.example.toml ~/.config/teamd/config.toml
```

В `~/.config/teamd/config.toml` включить:

```toml
[telegram]
enabled = true

[provider]
kind = "zai_chat_completions"
```

Секреты задать через `.env` или environment:

```bash
export TEAMD_TELEGRAM_BOT_TOKEN='...'
export TEAMD_PROVIDER_API_KEY='...'
```

Запуск:

```bash
./target/release/agentd telegram run
```

Pairing:

1. отправить боту `/start`;
2. для ручного запуска выполнить `./target/release/agentd telegram pair <key>` или `agentd telegram pair <key>`, если бинарь установлен в `PATH`;
3. для systemd-установки выполнить `teamdctl telegram pair <key>`;
4. проверить `agentd telegram pairings` или `teamdctl telegram pairings`.

## Telegram под systemd

Полная инструкция: [telegram/01-install-and-configure.md#9-настроить-systemd-вручную](telegram/01-install-and-configure.md#9-настроить-systemd-вручную).

Базовая схема:

- `teamd-daemon.service` держит daemon;
- `teamd-telegram.service` держит Telegram long polling worker;
- оба читают `/etc/teamd/teamd.env`;
- оба используют один `TEAMD_DATA_DIR`, например `/var/lib/teamd/state`.

Автозапуск и старт:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now teamd-daemon.service
sudo systemctl enable --now teamd-telegram.service
```

Статус и логи:

```bash
systemctl status teamd-daemon.service
systemctl status teamd-telegram.service
journalctl -u teamd-telegram.service -f
```

Pairing key из Telegram `/start` активируется не через unit-файл, а отдельной локальной командой от пользователя `teamd`:

```bash
teamdctl telegram pair tg...
teamdctl telegram pairings
```

Рестарт:

```bash
sudo systemctl restart teamd-daemon.service
sudo systemctl restart teamd-telegram.service
```

То же через helper:

```bash
teamdctl daemon status
teamdctl daemon restart
teamdctl telegram status
teamdctl telegram logs
```

## Binary в PATH и production helper

Если установка делалась не через `deploy-teamd.sh`, зарегистрировать binary в `PATH` можно так:

```bash
sudo mkdir -p /usr/local/bin
sudo ln -sf /opt/teamd/bin/agentd /usr/local/bin/agentd
hash -r
agentd version
```

`agentd` из `PATH` запускается от текущего пользователя и использует его environment. Для production-state под `/var/lib/teamd/state` используйте `teamdctl`: он сам читает `/etc/teamd/teamd.env`, переключается на пользователя `teamd` и запускает `/opt/teamd/bin/agentd`.

## Проверить версию и release state

```bash
agentd version
```

Полезные поля:

- `version`
- `commit`
- `tree`
- `build_id`
- `binary`
- `latest_release`

## Посмотреть диагностический лог

```bash
agentd logs 200
```

или в TUI:

- `\логи 200`

В systemd-установке:

```bash
teamdctl logs 200
```

`agentd logs` читает `audit/runtime.jsonl`. Это diagnostic log процесса, не transcript агента.

## Список sessions, transcript и tool-call ledger

```bash
agentd session list
agentd sessions
agentd session list --raw
agentd session transcript <session_id>
agentd session tools <session_id> --limit 50 --offset 0
agentd session tools <session_id> --results --limit 50 --offset 0
agentd session tool-result <tool_call_id>
agentd session tools <session_id> --raw --limit 50 --offset 0
```

`session list` без `--raw` показывает человекочитаемый отчёт. `--raw` оставлен для grep, diff и внешних скриптов.

`session tools` без `--raw` показывает человекочитаемый отчёт. `--results` добавляет preview результатов tools, включая stdout/stderr для execution tools. `session tool-result <tool_call_id>` показывает полный результат одного вызова. `--raw` оставлен для grep, diff и внешних скриптов.

Для systemd-установки:

```bash
teamdctl session list
teamdctl session list --raw
teamdctl session transcript <session_id>
teamdctl session tools <session_id> --limit 50 --offset 0
teamdctl session tools <session_id> --results --limit 50 --offset 0
teamdctl session tool-result <tool_call_id>
teamdctl session tools <session_id> --raw --limit 50 --offset 0
```

## Открыть TUI

```bash
agentd tui
```

Базовые клавиши:

- `Enter` — открыть выбранную session
- `Д` — открыть debug-view выбранной session
- `N` — новая session
- `D` — удалить
- `Esc` — назад

## Основные команды в чате

- `\помощь`
- `\сессии`
- `\новая`
- `\переименовать`
- `\очистить`
- `\версия`
- `\логи [N]`
- `\настройки`
- `\система`
- `\контекст`
- `\план`
- `\статус`
- `\процессы`
- `\задачи`
- `\артефакты`
- `\дебаг`
- `\апрув [id]`
- `\автоапрув <вкл|выкл>`
- `\доводка <N|выкл>`
- `\модель <id>`
- `\размышления <вкл|выкл>`
- `\думай <уровень>`
- `\компакт`

## Быстрый inter-agent сценарий

Внутри session:

```text
\судья Кто ты?
```

Дальше:

1. дождитесь system line о queued child session;
2. вернитесь к списку сессий;
3. откройте `Agent: Judge`;
4. смотрите ответ там.

Если нужно дождаться ответа программно, канонический runtime tool — `session_wait`.

## Работа с агентами

- `\агенты`
- `\агент показать [id]`
- `\агент выбрать <id>`
- `\агент создать <имя> [из <template>]`
- `\агент открыть [id]`
- `\агент написать <id> <сообщение>`

## Работа с расписаниями

- `\расписания`
- `\расписание показать <id>`
- `\расписание создать <id> <секунды> [agent=<id>] :: <промпт>`
- `\расписание изменить <id> ...`
- `\расписание включить <id>`
- `\расписание выключить <id>`
- `\расписание удалить <id>`

## Работа с MCP

- `\mcp`
- `\mcp показать <id>`
- `\mcp создать <id> command=<cmd> ...`
- `\mcp изменить <id> ...`
- `\mcp включить <id>`
- `\mcp выключить <id>`
- `\mcp перезапустить <id>`
- `\mcp удалить <id>`

## Работа с памятью

- `\память сессии <запрос>`
- `\память сессия <id> [summary|timeline|transcript|artifacts]`
- `\память знания <запрос>`
- `\память файл <path> [excerpt|full]`

## Когда нужен `\апрув`

Если run ушёл в `waiting_approval`, используйте:

```text
\апрув
```

или явно:

```text
\апрув approval-...
```

Если `\автоапрув вкл`, TUI будет подтверждать такие паузы автоматически.

## Когда использовать `\стоп` и `\отмена`

- `\стоп` — остановить активный run.
- `\отмена` — погасить вообще всю текущую работу session: runs, jobs, wakeups и связанные локальные ветки.

## Обновить бинарь

```bash
agentd update
agentd update v1.0.3
```

После обновления daemon/TUI нужно перезапустить.

## Быстрый набор для отладки

```bash
agentd version
agentd status
agentd logs 200
```

Если проблема только в одной session, полезно также открыть:

- `\контекст`
- `\статус`
- `\процессы`
- `\артефакты`
- `\дебаг`

В TUI `\дебаг` открывает browser по текущей session: сообщения, ответы модели, системные записи, tool calls и artifacts в одном списке. `Ctrl+D` делает то же самое из chat, `Д` на экране sessions открывает debug-view выбранной session.

Если нужен файл для баг-репорта, используйте `\отладка`. Команда сохраняет daemon-side bundle в `DATA_DIR/audit/debug-bundles`; при systemd-установке путь обычно начинается с `/var/lib/teamd/state/audit/debug-bundles`.

Для быстрого сбора production-диагностики с удалённого узла есть операторский скрипт:

```bash
scripts/collect-teamd-diagnostics.sh --host teamd-ams1
scripts/collect-teamd-diagnostics.sh --host teamd-ams1 --session session-...
scripts/collect-teamd-diagnostics.sh --local
```

По умолчанию скрипт берёт последнюю session по `updated_at` из `teamdctl session list --raw` и складывает локальный bundle в `diagnostics/`: статусы сервисов, `journalctl`, tail `audit/runtime.jsonl`, список sessions, transcript, tool ledger с results, `run show` по всем run выбранной session, JSON debug-view и payload-файлы transcript/artifacts. В конце он создаёт архив всего bundle: `diagnostics/teamd-diagnostics-...tar.gz`.

Удалённый режим использует SSH host alias. Для такого сценария удобно завести SSH alias с ключом, чтобы не передавать пароль в диагностических командах. Локальный режим `--local` запускает те же команды на текущей машине. Он работает, если на этой машине доступны `teamdctl`, `journalctl`, `curl` и production state в ожидаемом `TEAMD_DIAG_STATE_DIR` (`/var/lib/teamd/state` по умолчанию).

## Если TUI подвисает

1. Не гадайте.
2. Снимите `agentd logs 200`.
3. Смотрите последние `request.start/request.finish/request.error` и `session_ops.*`.

Именно лог должен показать, на каком шаге тормозит runtime path.
