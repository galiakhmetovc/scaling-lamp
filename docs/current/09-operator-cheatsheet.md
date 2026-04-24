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

Скрипт проверит native build dependencies, `cargo` и `rustc`; при необходимости поставит системные build-пакеты и stable Rust через `rustup`; интерактивно спросит Telegram bot token и Z.ai/API key, соберёт release binary, установит `/opt/teamd/bin/agentd`, создаст `/etc/teamd/config.toml`, `/etc/teamd/teamd.env` и два systemd service.

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
2. выполнить на сервере `./target/release/agentd telegram pair <key>` или `agentd telegram pair <key>`, если бинарь установлен в `PATH`;
3. проверить `./target/release/agentd telegram pairings` или `agentd telegram pairings`.

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
sudo -u teamd sh -lc 'set -a; . /etc/teamd/teamd.env; set +a; /opt/teamd/bin/agentd telegram pair tg...'
```

Рестарт:

```bash
sudo systemctl restart teamd-daemon.service
sudo systemctl restart teamd-telegram.service
```

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

## Открыть TUI

```bash
agentd tui
```

Базовые клавиши:

- `Enter` — открыть выбранную session
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

## Если TUI подвисает

1. Не гадайте.
2. Снимите `agentd logs 200`.
3. Смотрите последние `request.start/request.finish/request.error` и `session_ops.*`.

Именно лог должен показать, на каком шаге тормозит runtime path.
