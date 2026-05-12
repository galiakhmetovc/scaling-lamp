# Web Console

`apps/web` — собственный web-интерфейс teamD. Импортированный Hermes control-plane удалён: он был слишком тяжёлым, содержал чужие доменные сущности и создавал риск второго runtime.

## Назначение

Web Console — операторская панель поверх уже существующего `agentd`.

Он нужен для:

- просмотра состояния runtime, Postgres, NATS и сборки;
- основной работы с агентом через отдельный экран `Чат`;
- работы с сессиями: список, transcript, debug, task registry, active run;
- просмотра агентов, tool calls, delivery routes, Telegram bindings и traces;
- базового создания сессий и agent profiles через существующие HTTP endpoints;
- дальнейшего управления роем агентов без дублирования chat/runtime logic.

## Главный инвариант

Web Console не является вторым агентным runtime.

Все действия идут через canonical `agentd` HTTP API:

- snapshot: `GET /v1/web/snapshot`;
- список сессий: `GET /v1/sessions?limit=25&offset=0`;
- transcript: `GET /v1/sessions/{id}/transcript-tail/{limit}`;
- debug: `GET /v1/sessions/{id}/debug`;
- task registry сессии: `GET /v1/sessions/{id}/tasks`;
- workspace files: `GET /v1/sessions/{id}/workspace/list|read|download`, `POST /v1/sessions/{id}/workspace/write|mkdir|trash`;
- artifact files: `GET /v1/sessions/{id}/artifact-files`;
- session skills: `GET /v1/sessions/{id}/skills`, `POST /v1/sessions/{id}/skills/enable|disable`;
- отправка сообщения: `POST /v1/chat/turn`;
- создание сессии: `POST /v1/sessions`;
- создание агента: `POST /v1/agents`.

Frontend ходит не напрямую в демон, а через proxy path:

```text
/api/agentd/v1/* -> agentd /v1/*
```

Это позволяет держать token и внутренний адрес `agentd` на server side.

## UI-правила

Базовый UI-подход — Google Material Design через MUI.

Приоритеты интерфейса:

- таблицы, фильтры, списки, формы;
- явные статусы, loading/error/empty states;
- плотная операторская компоновка;
- русские UI-тексты по умолчанию;
- английские code identifiers, filenames, API fields, classes и functions.

Не делаем:

- marketing landing;
- декоративный hero;
- визуальный шум;
- отдельный tool loop или отдельный prompt/chat path.

## Запуск для разработки

```sh
cd apps/web
corepack pnpm install
TEAMD_AGENTD_BASE_URL=http://127.0.0.1:5140 corepack pnpm dev
```

Vite dev server слушает `0.0.0.0:5173`.

Если у `agentd` включён bearer token:

```sh
TEAMD_AGENTD_TOKEN=... corepack pnpm dev
```

## Production run

```sh
cd apps/web
corepack pnpm build
TEAMD_AGENTD_BASE_URL=http://127.0.0.1:5140 node server.mjs
```

Для публикации под single-domain Caddy route `https://<domain>/web/` сборка должна использовать base path:

```sh
corepack pnpm exec vite build --base=/web/
```

Caddy должен проксировать:

```text
/web/ -> teamd-web upstream
/api/agentd/* -> teamd-web upstream
/v1/web/* -> agentd upstream
```

`/api/agentd/*` идёт через `teamd-web`, потому что web server держит внутренний `TEAMD_AGENTD_BASE_URL` и при необходимости `TEAMD_AGENTD_TOKEN`.

Переменные:

- `TEAMD_WEB_HOST` или `HOST` — host bind, по умолчанию `0.0.0.0`;
- `TEAMD_WEB_PORT` или `PORT` — порт, по умолчанию `5173`;
- `TEAMD_AGENTD_BASE_URL` — URL демона, по умолчанию `http://127.0.0.1:5140`;
- `TEAMD_AGENTD_TOKEN` — bearer token для `agentd`, если включена авторизация;
- `TEAMD_AGENTD_TIMEOUT_MS` — timeout proxy-запросов, по умолчанию `120000`.
- `TEAMD_WEB_AUTH_USER` и `TEAMD_WEB_AUTH_PASSWORD` — включают HTTP Basic Auth для всего web console, включая `/web/` и `/api/agentd/*`;
- `TEAMD_WEB_AUTH_REALM` — optional realm для Basic Auth prompt, по умолчанию `teamD Web Console`.

Если `TEAMD_WEB_AUTH_USER` или `TEAMD_WEB_AUTH_PASSWORD` не заданы, Basic Auth отключён. Production deployment должен задавать оба значения через root-owned env file или systemd drop-in, а не через frontend bundle.

## Текущий статус

Реализовано:

- native React/Vite/MUI приложение;
- Node static server + reverse proxy к `agentd`;
- обзор runtime;
- отдельный экран `Чат` для основной работы с выбранной сессией;
- в `Чате` боковые панели `Сессии` и `Статус` по умолчанию свернуты в узкие рейлы, чтобы основной режим был именно chat-only;
- composer поддерживает command mode через `/`: web-команды открывают сессии, статус, refresh, stop/cancel или создают новую сессию без второго runtime path;
- нормальное отображение Markdown-ответов агента: GFM, таблицы, списки, ссылки, inline code и code blocks;
- список сессий грузится страницами через `limit/offset`, а не целиком;
- сессии: трёхпанельный операторский экран `список -> timeline -> inspector`;
- session timeline: единая лента сообщений, tool calls и артефактов из canonical debug/transcript data;
- session inspector: выбранная сессия, оперативные счётчики, выбранное событие, active run;
- отправка сообщения через `/v1/chat/turn`;
- создание сессии;
- агенты: список и создание profile через `/v1/agents`;
- tool calls: таблица с фильтром и ошибками;
- файлы: просмотр workspace выбранной сессии, preview/download файлов, создание текстовых файлов и папок, редактирование текстовых файлов, перемещение в `.trash`;
- artifact files: просмотр runtime artifacts, metadata, preview и download;
- skills: просмотр навыков выбранной сессии и ручное enable/disable;
- routes: delivery targets и Telegram bindings;
- traces: таблица trace links.

## Frontend decomposition

Web UI не должен превращаться в один God file.

Текущая структура:

```text
apps/web/src/
├── App.tsx                         # orchestration: загрузка данных, выбранная секция, dialogs state
├── api.ts                          # thin HTTP client к /api/agentd/*
├── components/                     # общие UI элементы
│   ├── ConsoleShell.tsx
│   ├── CreateAgentDialog.tsx
│   ├── CreateSessionDialog.tsx
│   ├── MarkdownMessage.tsx
│   └── common.tsx
├── features/
│   ├── chat/                       # основной рабочий чат
│   ├── files/                      # workspace + artifacts
│   ├── skills/                     # session skill activation
│   ├── sessions/                   # timeline/transcript/debug/tasks/inspector
│   ├── overview/
│   ├── agents/
│   ├── tools/
│   ├── routes/
│   ├── traces/
│   ├── runs/
│   └── settings/
├── ui/                             # theme/navigation
└── utils/                          # форматирование и мелкие pure helpers
```

Правило для будущих изменений: новый экран или крупный блок добавляется в `features/<domain>/`, а не в `App.tsx`. `App.tsx` может знать о состоянии приложения и выборе экрана, но не должен содержать таблицы, markdown renderer, карточки inspector или бизнес-разметку экранов.

Ограничения текущей версии:

- редактирование `SYSTEM.md`, `AGENTS.md` и `SKILL.md` пока не реализовано в web, потому что нужен отдельный безопасный `agentd` API для agent profile files;
- workspace editor редактирует только текстовые файлы; для бинарных файлов доступен download;
- если текстовый файл показан не полностью из-за лимита preview, сохранение отключено, чтобы не перезаписать файл обрезанным содержимым;
- удаление/архивация агентов и сессий не вынесены в UI;
- web пока не заменяет TUI, а закрывает read/review/operator-flow поверх тех же данных.

## План Hermes-parity

P1 — управляемость агентом:

1. Workspace file editor: read/download/create/edit/trash для файлов сессии.
2. Skills screen: видимость automatic/manual/disabled и ручной enable/disable.
3. Agent profile files: read/write `SYSTEM.md`, `AGENTS.md`, `skills/*/SKILL.md`.
4. Tools policy UI: показать доступные tools профиля и будущие overrides.

P2 — память и знания:

1. Memory UI: semantic memory search/list/update/delete.
2. KV UI: browse/edit/delete по scope (`operator`, `agent`, `workspace`, `session`).
3. Recall preview: показать, что будет подмешано в prompt выбранной сессии.
4. SilverBullet/knowledge boundary: явно показать, чем отличаются заметки, semantic memory и KV.

P3 — control-plane роя:

1. MCP UI: connectors, состояние, tools/resources/prompts.
2. Operations dashboard: очереди NATS, active runs, task registry, delivery routes.
3. Mission/conductor builder: создать задачу, выбрать агента, delivery target, follow/cancel.
4. Reports/inbox: checkpoints, blockers, handoff, ready-for-human.

P4 — polish:

1. Terminal/PTY только после явной policy-модели и audit.
2. PWA/mobile polish.
3. Capability gates: аккуратные placeholders вместо пустых/сломанных экранов.
4. Auth/roles перед широким внешним доступом.
