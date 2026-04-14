# Mesh Runtime

## Назначение

`teamD` использует mesh runtime как внутренний слой оркестрации между агентами.

Снаружи пользователь видит одного `owner`-агента в Telegram. Внутри owner может:
- уточнить задачу;
- собрать proposals от peer-агентов;
- выбрать winner;
- дать winner право на реальное tool execution;
- при composite-задаче разложить задачу на шаги и собрать итог.

Главный принцип: только owner отвечает пользователю, даже если реальные проверки и инструменты выполнял другой агент.

## Основные сущности

### Owner

`owner`:
- принимает пользовательский запрос из Telegram;
- применяет `OrchestrationPolicy`;
- запускает clarification/proposal/execution/composite flow;
- собирает trace;
- отправляет финальный ответ пользователю.

### Peer

`peer`:
- регистрируется в mesh registry;
- принимает `Envelope` по HTTP;
- может отвечать proposal-ом;
- может выполнять execution round;
- не общается с пользователем напрямую.

### Registry

Registry хранит:
- `agent_id`
- `addr`
- `model`
- `status`
- `last_seen_at`
- score records по `task_class`

Текущая реализация:
- runtime registry: Postgres
- identity registry: in-memory MVP

### Policy

`OrchestrationPolicy` управляет mesh run.

Текущие профили:
- `fast`
- `balanced`
- `deep`
- `composite`

Ключевые поля:
- `clarification_mode`
- `max_clarification_rounds`
- `proposal_mode`
- `sample_k`
- `min_quorum_size`
- `proposal_timeout`
- `execution_mode`
- `allow_tool_execution`
- `composite_planning`
- `judge_mode`

В Telegram policy управляется командами:
- `/mesh`
- `/mesh help`
- `/mesh mode <profile>`
- `/mesh set clarification_mode=<...>`
- `/mesh set proposal_mode=<...>`
- `/mesh set sample_k=<n>`
- `/mesh set execution_mode=<owner|winner>`
- `/mesh set composite_planning=<off|auto|force>`

## Live topology

Текущий live runtime обычно поднимается так:
- `1 owner` с Telegram ingress;
- `1..N peers` без Telegram token;
- owner слушает mesh endpoint локально;
- peers слушают отдельные loopback-порты и регистрируются в registry.

Это важно для понимания trace:
- пользователь всегда говорит только с owner;
- owner может делегировать proposal/execution peer-ам;
- количество peer-ов зависит не от архитектурного лимита, а от текущего запуска coordinator/runtime.

## Runtime modes

### 1. Clarification

Clarification запускается до proposal round, если policy не выключает этот слой.

Вход:
- raw user prompt

Выход:
- `ClarifiedTask`

`ClarifiedTask` содержит:
- `goal`
- `deliverables`
- `constraints`
- `assumptions`
- `missing_info`
- `task_class`
- `task_shape`

Если missing info критичен:
- owner задаёт follow-up question пользователю;
- proposal round не стартует.

Если модель clarifier нарушила формат и вернула не JSON:
- run не падает;
- clarifier деградирует в low-confidence fallback;
- owner продолжает с исходным prompt.

### 2. Proposal round

Все sampled кандидаты получают задачу в режиме `proposal`.

Rules:
- proposal round не должен выполнять инструменты;
- proposal responses используются для сравнения подходов;
- owner сохраняет proposals в trace.

Текущая модель proposal:
- owner строит local proposal;
- peers получают `Envelope{Kind: "proposal"}`;
- owner сравнивает предложения детерминированно;
- затем строит `ExecutionBrief`.

### 3. Winner-only execution

Только один winner получает execution round.

Rules:
- proposal agents не должны вызывать tools;
- winner получает `ExecutionBrief`;
- только execution round имеет доступ к MCP/tools;
- owner остаётся final responder.

Текущий execution path:
- если winner = owner, owner исполняет locally;
- если winner = peer, owner отправляет `Envelope{Kind: "execute"}` peer-у;
- итог возвращается owner-у;
- owner отвечает пользователю.

### 4. Composite flow

Для составных задач owner может перейти в composite mode.

Текущая MVP-модель:
- planner строит линейный `TaskPlan`;
- plan состоит из `[]PlannedStep`;
- каждый шаг маршрутизируется отдельно;
- owner собирает step outputs в один финальный ответ.

Текущие ограничения:
- нет DAG scheduler;
- `Dependencies` существуют как forward-compatible поле, но не используются;
- integration пока простая и owner-centric.

## Data contracts

### Envelope

`Envelope` используется для межагентного сообщения.

Ключевые поля:
- `version`
- `message_id`
- `trace_id`
- `session_id`
- `owner_agent`
- `from_agent`
- `to_agent`
- `task_class`
- `task_shape`
- `kind`
- `ttl`
- `prompt`
- `execution_brief`
- `metadata`

Используемые значения `kind`:
- `proposal`
- `execute`
- `reply`

### CandidateReply

`CandidateReply` возвращается executor-ами и transport handler-ом.

Содержит:
- `agent_id`
- `stage`
- `text`
- `proposal`
- `proposal_metadata`
- `deterministic_score`
- `judge_score`
- `passed_checks`
- `trace`
- `rejection_reason`

### ExecutionBrief

`ExecutionBrief` синтезируется owner-ом перед real execution.

Содержит:
- `goal`
- `required_steps`
- `constraints`
- `adopted_ideas`
- `conflicts_to_resolve`
- `required_checks`

Validation rules:
- `goal` обязателен;
- `required_steps` не может быть пустым;
- unresolved conflicts блокируют execution.

## Transport

Текущий transport:
- HTTP
- endpoint: `POST /mesh/message`

Transport behavior:
- `TTL` декрементируется на отправке;
- dedupe по `message_id`;
- peer отвечает `Envelope` с `candidate_reply` в `metadata`.

Это временный MVP transport. Архитектурные boundaries сохранены transport-agnostic для будущего перехода на gRPC.

## Tool execution

Текущие доступные инструменты:
- `filesystem.read_file`
- `filesystem.write_file`
- `filesystem.list_dir`
- `shell.exec`

`ToolExecutor`:
- вызывает provider;
- обрабатывает `tool_calls`;
- исполняет MCP tools через runtime;
- возвращает финальный ответ agent-а.

Critical rule:
- если `Envelope.Kind == "proposal"`, `ToolExecutor` возвращает ошибку stage и не вызывает tools.

## Telegram integration

Telegram adapter умеет:
- обычный LLM/tool chat path;
- mesh owner flow;
- session management;
- mesh policy commands.

### Status card

Во время run Telegram показывает status card:
- текущая стадия;
- elapsed time;
- current tool;
- context estimate;
- tool steps;
- completion/error state.

### Mesh trace

Кнопка `Статус` теперь показывает:
1. numeric breakdown:
   - prompt tokens
   - completion tokens
   - tool calls
   - tool output chars
   - tool duration
   - context estimate
2. mesh trace:
   - `Ingress`
   - `Policy`
   - `Clarification`
   - `Classification`
   - `Proposal`
   - `Winner`
   - `ExecutionBrief`
   - `Execution`
   - `Planner`
   - `CompositeStep`
   - `Integration`

Что именно видно в trace:
- какой policy profile применился к run;
- был ли clarification и почему;
- какие peer-ы были выбраны в proposal round;
- какой proposal победил и по какой причине;
- кто стал реальным executor;
- какие tools реально запускались в execution round;
- как owner собрал финальный ответ.

Если trace не помещается в одно Telegram message:
- он режется на несколько страниц;
- pages отправляются последовательно.

## Config

Текущие mesh env:
- `TEAMD_AGENT_ID`
- `TEAMD_MESH_LISTEN_ADDR`
- `TEAMD_MESH_REGISTRY_DSN`
- `TEAMD_MESH_COLD_START_FANOUT`
- `TEAMD_MESH_EXPLORATION_RATE`
- `TEAMD_MESH_PEER_TIMEOUT`
- `TEAMD_MESH_HEARTBEAT_INTERVAL`
- `TEAMD_MESH_STALE_THRESHOLD`
- `TEAMD_MESH_CLASSIFIER_MODEL`
- `TEAMD_MESH_PROPOSAL_MODEL`
- `TEAMD_MESH_EXECUTION_MODEL`

Замечание:
- model override поля уже есть в config;
- текущий runtime wiring пока всё ещё использует один shared provider client/model, если отдельно не доработать bootstrap.

## Текущие ограничения

- live mesh сейчас обычно запускается как `1 owner + N peers`;
- score-based specialization есть как база, но routing ещё простой;
- `IdentityRegistry` пока in-memory, не Postgres-backed;
- `Spawner` и lease lifecycle пока не wired в live coordinator;
- planner и composite integration пока MVP-уровня;
- trace прозрачный, но ещё не показывает абсолютно все raw payloads;
- Telegram rich text и table rendering всё ещё компромиссные.

## Как проверить

### Policy

В Telegram:
```text
/mesh
/mesh mode deep
/mesh set sample_k=3
```

### Clarification

В Telegram:
```text
напиши скрипт
```

Ожидаемо owner задаст follow-up question.

### Winner execution

В Telegram:
```text
проверь память на сервере через shell и кратко напиши результат
```

Затем нажать:
```text
Статус
```

Ожидаемо в trace будет видно:
- policy
- clarification result
- proposals
- winner
- execution brief
- execution result

### Composite

В Telegram:
```text
напиши скрипт резервного копирования и задокументируй его
```

Затем:
```text
Статус
```

Ожидаемо trace покажет planner/composite steps/integration.
