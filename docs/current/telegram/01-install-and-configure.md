# Установка из репозитория и конфигурирование Telegram

Документ описывает базовый путь: скачать репозиторий, собрать `agentd`, включить Telegram-интеграцию, запустить worker и привязать Telegram-пользователя через pairing.

## Быстрый путь: deploy script

Из корня checkout:

```bash
./scripts/deploy-teamd.sh
```

Скрипт:

- проверяет native build dependencies;
- если не хватает `pkg-config`, OpenSSL dev headers или C toolchain, ставит системные пакеты через доступный package manager;
- проверяет `cargo`/`rustc`;
- если Rust отсутствует или слишком старый для edition 2024, ставит/обновляет stable Rust через `rustup`;
- собирает `agentd` в release mode;
- ставит binary в `/opt/teamd/bin/agentd`;
- регистрирует `agentd` в `PATH` через `/usr/local/bin/agentd`;
- ставит operator helper `/usr/local/bin/teamdctl`;
- создаёт пользователя `teamd`;
- пишет `/etc/teamd/config.toml` и `/etc/teamd/teamd.env`;
- спрашивает Telegram bot token и Z.ai/API key скрытым вводом;
- создаёт `teamd-daemon.service` и `teamd-telegram.service`;
- включает автозапуск и запускает оба service.

Проверить без изменений на машине:

```bash
TEAMD_TELEGRAM_BOT_TOKEN='123456789:test-token' \
  TEAMD_PROVIDER_API_KEY='zai-test-key' \
  ./scripts/deploy-teamd.sh --dry-run --non-interactive --no-build --no-start
```

Если Rust ставить автоматически нельзя, используйте:

```bash
./scripts/deploy-teamd.sh --no-install-rust
```

В этом режиме скрипт завершится ошибкой, если `cargo`/`rustc` отсутствуют или старее минимальной версии.

Если системные build dependencies ставить автоматически нельзя, используйте:

```bash
./scripts/deploy-teamd.sh --no-install-system-deps
```

Тогда заранее установите пакеты вручную. Для Ubuntu/Debian минимум:

```bash
sudo apt-get update
sudo apt-get install -y pkg-config libssl-dev build-essential ca-certificates curl
```

Если секреты уже есть в environment, можно запустить без интерактивного ввода:

```bash
TEAMD_TELEGRAM_BOT_TOKEN='123456789:real-token' \
  TEAMD_PROVIDER_API_KEY='real-provider-key' \
  ./scripts/deploy-teamd.sh --non-interactive
```

После `/start` в Telegram pairing key активируется так:

```bash
teamdctl telegram pair <key>
```

Ручные шаги ниже описывают то же самое подробно и полезны для отладки.

## 1. Требования

Нужны:

- Linux/WSL/server с доступом в интернет;
- package manager для установки `pkg-config`, OpenSSL dev headers и C toolchain, либо заранее установленные build dependencies;
- Rust toolchain с `cargo`, либо разрешение скрипту поставить Rust через `rustup`;
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
kind = "zai_chat_completions"

[permissions]
mode = "default"
```

`telegram.bot_token` лучше не писать в `config.toml`; храните его в `.env` или в environment.

## 5. Настроить Z.ai provider

Для Z.ai используйте provider kind:

```toml
[provider]
kind = "zai_chat_completions"
```

Если `api_base` и `default_model` не заданы, `agentd` выставит значения по умолчанию:

```toml
[provider]
kind = "zai_chat_completions"
api_base = "https://api.z.ai/api/coding/paas/v4"
default_model = "glm-5-turbo"
```

Код добавляет `/chat/completions` сам, поэтому в `api_base` не надо дописывать этот suffix.

Минимальный практический вариант:

```toml
[provider]
kind = "zai_chat_completions"
```

Ключ задайте через environment или `.env`:

```bash
export TEAMD_PROVIDER_API_KEY='replace-with-zai-key'
```

Если нужно временно переопределить provider без редактирования TOML:

```bash
export TEAMD_PROVIDER_KIND='zai_chat_completions'
export TEAMD_PROVIDER_API_KEY='replace-with-zai-key'
export TEAMD_PROVIDER_MODEL='glm-5-turbo'
```

Проверка provider:

```bash
agentd provider smoke
```

Если запускаете из репозитория:

```bash
cargo run -p agentd -- provider smoke
```

## 6. Настроить секреты

`agentd` читает `.env` из текущей рабочей директории или из директории рядом с исполняемым бинарём.

Для запуска из корня репозитория:

```bash
cat > .env <<'EOF'
TEAMD_TELEGRAM_BOT_TOKEN=123456789:replace-with-real-token
TEAMD_PROVIDER_API_KEY=replace-with-zai-key
EOF
chmod 0600 .env
```

Для service/systemd или запуска из другой директории надёжнее задавать переменные окружения явно:

```bash
export TEAMD_TELEGRAM_BOT_TOKEN='123456789:replace-with-real-token'
export TEAMD_PROVIDER_API_KEY='replace-with-zai-key'
```

Если нужен нестандартный путь к конфигу:

```bash
export TEAMD_CONFIG=/absolute/path/to/config.toml
```

Если нужно явно зафиксировать state root:

```bash
export TEAMD_DATA_DIR=/absolute/path/to/teamd-state
```

## 7. Проверить конфиг

```bash
agentd version
agentd status
agentd provider smoke
```

Если запускаете из репозитория без установленного бинаря:

```bash
cargo run -p agentd -- version
cargo run -p agentd -- status
cargo run -p agentd -- provider smoke
```

Если `telegram.enabled = true`, но token не найден, команда завершится ошибкой вида:

```text
telegram.bot_token must be set when telegram.enabled is true
```

Проверьте `.env`, environment и рабочую директорию запуска.

## 8. Запустить Telegram worker вручную

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

## 9. Настроить systemd вручную

Для постоянного сервера лучше запускать два systemd unit’а:

- `teamd-daemon.service` — держит daemon process;
- `teamd-telegram.service` — держит Telegram long polling worker.

Так Telegram worker не зависит от unmanaged autospawn daemon, а systemd отдельно показывает статус и логи обоих процессов.

### 9.1. Подготовить пользователя и каталоги

```bash
if ! id -u teamd >/dev/null 2>&1; then
  sudo useradd --system --create-home --home-dir /var/lib/teamd --shell /usr/sbin/nologin teamd
fi
sudo mkdir -p /opt/teamd/bin /etc/teamd /var/lib/teamd/state
sudo install -m 0755 ./target/release/agentd /opt/teamd/bin/agentd
sudo ln -sf /opt/teamd/bin/agentd /usr/local/bin/agentd
sudo install -m 0755 ./scripts/teamdctl.sh /usr/local/bin/teamdctl
sudo test -f /etc/teamd/config.toml || sudo cp config.example.toml /etc/teamd/config.toml
sudo chown -R teamd:teamd /var/lib/teamd
```

В `/etc/teamd/config.toml` включите Telegram и Z.ai:

```toml
data_dir = "/var/lib/teamd/state"

[daemon]
bind_host = "127.0.0.1"
bind_port = 5140
skills_dir = "skills"

[telegram]
enabled = true

[provider]
kind = "zai_chat_completions"

[permissions]
mode = "default"
```

### 9.2. Создать environment file

Для systemd не полагайтесь на `.env` из текущей директории. Заведите один явный `EnvironmentFile`, который будут использовать оба unit’а и операторские CLI-команды.

```bash
sudo tee /etc/teamd/teamd.env >/dev/null <<'EOF'
TEAMD_CONFIG=/etc/teamd/config.toml
TEAMD_DATA_DIR=/var/lib/teamd/state
TEAMD_TELEGRAM_BOT_TOKEN=123456789:replace-with-real-token
TEAMD_PROVIDER_API_KEY=replace-with-zai-key
EOF
sudo chown root:teamd /etc/teamd/teamd.env
sudo chmod 0640 /etc/teamd/teamd.env
```

`root:teamd 0640` нужен не systemd, а операторским командам вроде `telegram pair`: их надо запускать от пользователя `teamd`, чтобы они писали в тот же `TEAMD_DATA_DIR`, что и сервисы.

### 9.3. Создать daemon unit

```bash
sudo tee /etc/systemd/system/teamd-daemon.service >/dev/null <<'EOF'
[Unit]
Description=teamD daemon
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=teamd
Group=teamd
EnvironmentFile=/etc/teamd/teamd.env
WorkingDirectory=/var/lib/teamd
ExecStart=/opt/teamd/bin/agentd daemon
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF
```

### 9.4. Создать Telegram unit

```bash
sudo tee /etc/systemd/system/teamd-telegram.service >/dev/null <<'EOF'
[Unit]
Description=teamD Telegram worker
After=network-online.target teamd-daemon.service
Wants=network-online.target
Requires=teamd-daemon.service

[Service]
Type=simple
User=teamd
Group=teamd
EnvironmentFile=/etc/teamd/teamd.env
WorkingDirectory=/var/lib/teamd
ExecStart=/opt/teamd/bin/agentd telegram run
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF
```

### 9.5. Включить автозапуск и запустить

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now teamd-daemon.service
sudo systemctl enable --now teamd-telegram.service
```

Проверить:

```bash
systemctl status teamd-daemon.service
systemctl status teamd-telegram.service
journalctl -u teamd-daemon.service -n 100 --no-pager
journalctl -u teamd-telegram.service -n 100 --no-pager
```

### 9.6. Перезапуск и остановка

Перезапустить Telegram worker:

```bash
sudo systemctl restart teamd-telegram.service
```

Перезапустить daemon и Telegram worker:

```bash
sudo systemctl restart teamd-daemon.service
sudo systemctl restart teamd-telegram.service
```

Остановить:

```bash
sudo systemctl stop teamd-telegram.service
sudo systemctl stop teamd-daemon.service
```

Отключить автозапуск:

```bash
sudo systemctl disable teamd-telegram.service teamd-daemon.service
```

Перезагрузить сервер целиком:

```bash
sudo reboot
```

После reboot оба сервиса должны подняться сами, если они были включены через `systemctl enable`.

## 10. Обновление установленного systemd-сервиса

Из checkout репозитория:

```bash
git pull --ff-only
cargo build --release -p agentd
sudo systemctl stop teamd-telegram.service
sudo systemctl stop teamd-daemon.service
sudo install -m 0755 ./target/release/agentd /opt/teamd/bin/agentd
sudo ln -sf /opt/teamd/bin/agentd /usr/local/bin/agentd
sudo install -m 0755 ./scripts/teamdctl.sh /usr/local/bin/teamdctl
sudo systemctl start teamd-daemon.service
sudo systemctl start teamd-telegram.service
```

Проверить версию после обновления:

```bash
agentd version
journalctl -u teamd-telegram.service -n 100 --no-pager
```

Если установленный binary лежит в `~/.local/bin/agentd`, обновление аналогичное: пересобрать, заменить binary и перезапустить процессы, которые его используют.

## 11. Pairing пользователя

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

Если используется systemd-установка из примера, pairing key не передаётся в service unit и не требует рестарта сервиса. Его надо один раз активировать локальной CLI-командой, запущенной от того же unix-пользователя и с тем же env, что и `teamd-telegram.service`:

```bash
teamdctl telegram pair tg...
```

Эта команда находит pending pairing record в `TEAMD_DATA_DIR`, помечает его как activated и привязывает Telegram account к runtime state. Уже запущенный Telegram worker увидит эту запись без перезапуска.

4. Проверить список pairing records:

```bash
agentd telegram pairings
```

Для systemd-установки:

```bash
teamdctl telegram pairings
```

После активации пользователь может писать боту обычные сообщения.

## 12. Минимальная smoke-проверка

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

### Как выглядит живой turn в Telegram

Во время выполнения turn Telegram worker ведёт себя так:

- сразу отправляет временное status-message;
- если turn не закончился мгновенно, бот начинает показывать `typing`;
- status-message обновляется компактным HTML-блоком, а не сырым Markdown;
- в статусе показываются стадия, текущий tool и точные счётчики:
  - `Вызовы` — сколько уникальных tool call уже стартовало в этом turn;
  - `Ошибки` — сколько из них завершилось `failed`;
- финальный ответ приходит отдельным новым сообщением;
- status-message после этого не редактируется в финальный ответ, а помечается как временный/stale;
- stale status удаляется:
  - при следующем сообщении пользователя в этот chat;
  - или автоматически через `30 минут`, если пользователь больше не писал.

Это сделано специально, чтобы:

- финальный ответ оставался чистым и читабельным;
- progress-message не копились в chat;
- Telegram surface оставался thin UI над тем же canonical runtime path.

## 13. Команды Telegram

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

## 14. Важные параметры Telegram

Официальные страницы Telegram, на которые завязаны лимиты и API-поля:

- Bot FAQ про rate limits: <https://core.telegram.org/bots/faq#my-bot-is-hitting-limits-how-do-i-avoid-this>
- `getUpdates`: <https://core.telegram.org/bots/api#getupdates>
- `sendMessage`: <https://core.telegram.org/bots/api#sendmessage>
- `sendDocument`: <https://core.telegram.org/bots/api#senddocument>

В коде мы целимся не в 100% лимита, а примерно в 80% безопасной скорости:

- один chat: официальный ориентир — не больше 1 сообщения/секунду; в конфиге `progress_update_min_interval_ms = 1250`, то есть 0.8 сообщения/секунду;
- group chat: официальный ориентир — не больше 20 сообщений/минуту; для групповых broadcast-сценариев нужен cap около 16 сообщений/минуту на группу;
- bulk notifications: официальный ориентир — около 30 сообщений/секунду; для массовой рассылки нужен global cap около 24 сообщений/секунду;
- `getUpdates.limit`: Bot API принимает 1-100, текущая реализация использует верхнюю границу 100;
- `sendMessage.text`: Bot API принимает 1-4096 characters after entities parsing, поэтому длинные ответы надо дробить до лимита;
- captions: Bot API для media captions обычно ограничивает 0-1024 characters after entities parsing, поэтому длинный текст лучше отправлять отдельными text messages или document.

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

## 15. Диагностика

Посмотреть последние runtime logs:

```bash
agentd logs 200
```

Если binary установлен deploy script’ом, он лежит в `/opt/teamd/bin/agentd` и дополнительно доступен как `/usr/local/bin/agentd`:

```bash
agentd logs 200
```

Для systemd-установки запускайте CLI через `teamdctl`, чтобы использовать того же пользователя `teamd`, тот же `/etc/teamd/teamd.env` и тот же `TEAMD_DATA_DIR`:

```bash
teamdctl logs 200
```

`agentd logs` читает `data_dir/audit/runtime.jsonl`. Это structured diagnostic log runtime/daemon/Telegram/provider-loop, а не transcript конкретного агента.

Для stdout/stderr systemd unit’ов используйте `teamdctl` shortcuts над `journalctl`:

```bash
teamdctl daemon logs
teamdctl telegram logs
```

Получить список sessions, прочитать transcript и вызовы tools:

```bash
teamdctl session list
teamdctl session list --raw
teamdctl session transcript <session_id>
teamdctl session tools <session_id> --limit 50 --offset 0
teamdctl session tools <session_id> --results --limit 50 --offset 0
teamdctl session tool-result <tool_call_id>
teamdctl session tools <session_id> --raw --limit 50 --offset 0
```

Обычный `session list` и `session tools` выводят читаемые отчёты. `session tools --results` добавляет preview результатов вызовов, а `session tool-result <tool_call_id>` показывает полный output конкретного вызова из preview или artifact. `--raw` нужен только для однострочного машинного формата.

Типовые проблемы:

| Симптом | Что проверить |
| --- | --- |
| `telegram is disabled in config` | В `config.toml` должно быть `telegram.enabled = true`. |
| `telegram.bot_token is not configured` | `TEAMD_TELEGRAM_BOT_TOKEN` должен быть в environment или `.env`, который видит процесс. |
| `/start` не отвечает | Запущен ли `agentd telegram run`; нет ли другого процесса, который уже polling-ит того же бота. |
| Pairing key истёк | Повторить `/start` и активировать новый key. |
| Provider не отвечает | Проверить `TEAMD_PROVIDER_API_KEY`, `provider.kind = "zai_chat_completions"` и `agentd provider smoke`. |
| Сообщения не доходят | Проверить `agentd logs 200`, provider key, daemon connectivity и selected session. |
| Ответы идут не в тот state | Проверить `TEAMD_DATA_DIR`, пользователя запуска и `data_dir` в `agentd version`. |
| systemd service падает | Проверить `journalctl -u teamd-daemon.service` и `journalctl -u teamd-telegram.service`. |

## 16. Что не делать

- Не коммитьте реальные tokens в Git.
- Не запускайте два `agentd telegram run` на один и тот же Bot API token: long polling будет конфликтовать.
- Не запускайте часть команд под обычным пользователем, а часть под `root`, если не понимаете, какой `data_dir` используется.
- Не добавляйте отдельный Telegram runtime path: Telegram должен оставаться thin surface поверх daemon/app/runtime.
