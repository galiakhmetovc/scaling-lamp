# Установка из репозитория и конфигурирование Telegram

Документ описывает базовый путь: скачать репозиторий, собрать `agentd`, включить Telegram-интеграцию, запустить worker и привязать Telegram-пользователя через pairing.

## 1. Требования

Нужны:

- Linux/WSL/server с доступом в интернет;
- Rust toolchain с `cargo`;
- Telegram bot token от `@BotFather`;
- LLM provider key;
- доступ к GitHub-репозиторию `galiakhmetovc/scaling-lamp`.

## 2. Скачать репозиторий

```bash
git clone https://github.com/galiakhmetovc/scaling-lamp.git
cd scaling-lamp
```

Если репозиторий уже скачан:

```bash
git pull --ff-only
```

## 3. Собрать `agentd`

Для быстрой проверки:

```bash
cargo build -p agentd
cargo run -p agentd -- version
```

Для нормального запуска:

```bash
cargo build --release -p agentd
./target/release/agentd version
```

Опционально положить бинарь в пользовательский `PATH`:

```bash
mkdir -p ~/.local/bin
install -m 0755 ./target/release/agentd ~/.local/bin/agentd
agentd version
```

Если `agentd` не находится, проверьте, что `~/.local/bin` есть в `PATH`.

## 4. Создать `config.toml`

По умолчанию `agentd` читает конфиг из:

```text
~/.config/teamd/config.toml
```

Создайте его из примера:

```bash
mkdir -p ~/.config/teamd
cp config.example.toml ~/.config/teamd/config.toml
```

Минимальные секции для Telegram:

```toml
[telegram]
enabled = true
poll_interval_ms = 1000
poll_request_timeout_seconds = 50
progress_update_min_interval_ms = 1250
pairing_token_ttl_seconds = 900
max_upload_bytes = 16777216
max_download_bytes = 41943040
private_chat_auto_create_session = true
group_require_mention = true
default_autoapprove = true

[provider]
kind = "openai_responses"
default_model = "gpt-5.4"

[permissions]
mode = "default"
```

`telegram.bot_token` лучше не писать в `config.toml`; храните его в `.env` или в environment.

## 5. Настроить секреты

`agentd` читает `.env` из текущей рабочей директории или из директории рядом с исполняемым бинарём.

Для запуска из корня репозитория:

```bash
cat > .env <<'EOF'
TEAMD_TELEGRAM_BOT_TOKEN=123456789:replace-with-real-token
TEAMD_PROVIDER_API_KEY=replace-with-provider-key
EOF
chmod 0600 .env
```

Для service/systemd или запуска из другой директории надёжнее задавать переменные окружения явно:

```bash
export TEAMD_TELEGRAM_BOT_TOKEN='123456789:replace-with-real-token'
export TEAMD_PROVIDER_API_KEY='replace-with-provider-key'
```

Если нужен нестандартный путь к конфигу:

```bash
export TEAMD_CONFIG=/absolute/path/to/config.toml
```

Если нужно явно зафиксировать state root:

```bash
export TEAMD_DATA_DIR=/absolute/path/to/teamd-state
```

## 6. Проверить конфиг

```bash
agentd version
agentd status
```

Если запускаете из репозитория без установленного бинаря:

```bash
cargo run -p agentd -- version
cargo run -p agentd -- status
```

Если `telegram.enabled = true`, но token не найден, команда завершится ошибкой вида:

```text
telegram.bot_token must be set when telegram.enabled is true
```

Проверьте `.env`, environment и рабочую директорию запуска.

## 7. Запустить Telegram worker

Из установленного бинаря:

```bash
agentd telegram run
```

Из репозитория:

```bash
cargo run -p agentd -- telegram run
```

Что делает worker:

- подключается к локальному daemon;
- если daemon не запущен, autospawn-ит локальный daemon;
- регистрирует slash-команды в Telegram Bot API;
- запускает long polling;
- доставляет входящие сообщения в canonical runtime path.

Процесс должен оставаться запущенным, пока нужен Telegram-доступ.

## 8. Pairing пользователя

1. В Telegram откройте бота и отправьте:

```text
/start
```

2. Бот вернёт pairing key и подсказку:

```text
Pairing key: tg...

Activate it on the server:
agentd telegram pair tg...
```

3. На сервере выполните команду из подсказки:

```bash
agentd telegram pair tg...
```

4. Проверить список pairing records:

```bash
agentd telegram pairings
```

После активации пользователь может писать боту обычные сообщения.

## 9. Минимальная smoke-проверка

В Telegram:

```text
/help
```

Затем:

```text
Привет, кто ты?
```

Ожидаемое поведение:

- бот отвечает в тот же Telegram chat;
- при первом normal message в private chat создаётся или выбирается session;
- ответ проходит через LLM provider;
- state сохраняется в `data_dir`.

## 10. Команды Telegram

Текущий набор slash-команд:

| Команда | Назначение |
| --- | --- |
| `/start` | Получить pairing key. |
| `/help` | Показать помощь. |
| `/new [title]` | Создать и выбрать session. |
| `/sessions` | Показать sessions. |
| `/use <session_id>` | Выбрать session. |
| `/judge <message>` | Отправить сообщение Judge. |
| `/agent <agent_id> <message>` | Отправить сообщение другому agent. |

Обычный текст без slash-команды отправляется в выбранную session как chat turn.

## 11. Важные параметры Telegram

| Параметр | Значение по умолчанию | Смысл |
| --- | --- | --- |
| `telegram.enabled` | `false` | Включает Telegram-интеграцию. |
| `telegram.poll_interval_ms` | `1000` | Пауза между polling-итерациями. |
| `telegram.poll_request_timeout_seconds` | `50` | Timeout long polling request к Bot API. |
| `telegram.progress_update_min_interval_ms` | `1250` | Минимальный интервал progress updates. |
| `telegram.pairing_token_ttl_seconds` | `900` | TTL pairing key. |
| `telegram.max_upload_bytes` | `16777216` | Soft cap для upload. |
| `telegram.max_download_bytes` | `41943040` | Soft cap для download. |
| `telegram.private_chat_auto_create_session` | `true` | Автоматически создавать session в private chat. |
| `telegram.group_require_mention` | `true` | В группах реагировать только на mention/targeted command. |
| `telegram.default_autoapprove` | `true` | Включать auto-approve по умолчанию для Telegram-created sessions. |

## 12. Диагностика

Посмотреть последние runtime logs:

```bash
agentd logs 200
```

Типовые проблемы:

| Симптом | Что проверить |
| --- | --- |
| `telegram is disabled in config` | В `config.toml` должно быть `telegram.enabled = true`. |
| `telegram.bot_token is not configured` | `TEAMD_TELEGRAM_BOT_TOKEN` должен быть в environment или `.env`, который видит процесс. |
| `/start` не отвечает | Запущен ли `agentd telegram run`; нет ли другого процесса, который уже polling-ит того же бота. |
| Pairing key истёк | Повторить `/start` и активировать новый key. |
| Сообщения не доходят | Проверить `agentd logs 200`, provider key, daemon connectivity и selected session. |
| Ответы идут не в тот state | Проверить `TEAMD_DATA_DIR`, пользователя запуска и `data_dir` в `agentd version`. |

## 13. Что не делать

- Не коммитьте реальные tokens в Git.
- Не запускайте два `agentd telegram run` на один и тот же Bot API token: long polling будет конфликтовать.
- Не запускайте часть команд под обычным пользователем, а часть под `root`, если не понимаете, какой `data_dir` используется.
- Не добавляйте отдельный Telegram runtime path: Telegram должен оставаться thin surface поверх daemon/app/runtime.
