# MIMO agent mesh

Этот документ фиксирует целевую архитектуру MIMO-взаимодействий в `teamD`.

Статус: design baseline + первый реализованный vertical slice. Уже есть PostgreSQL event tables, Telegram webhook ingress, NATS JetStream adapter, rule router, session worker, delivery worker и task worker для `message_agent`/`agent_task`. Production может оставаться на polling, пока webhook runtime не включён конфигом.

## Зачем это нужно

Текущий runtime уже умеет sessions, jobs, schedules, inter-agent messages, delegated jobs, Telegram bindings и delivery. Следующий шаг — перестать думать в модели “один чат = одна session = один ответ туда же”.

Целевая модель:

```text
Inputs -> Session Core -> Outputs
             |
             v
        Async Tasks / Result Bus
```

Одна session хранит состояние агента. Входов и выходов у неё может быть много. Вход и выход не обязаны совпадать.

## Внешние ориентиры

Мы не копируем один стандарт напрямую, но используем проверенные идеи:

- A2A: agent discovery, stateful tasks, messages, artifacts, push/stream updates, отсутствие доступа к внутреннему состоянию другого агента.
- OpenAI Agents: различать `agents as tools` и `handoffs`; если специалист не становится user-facing агентом, он должен быть callable worker/tool.
- Anthropic multi-agent pattern: orchestrator-worker, отдельные context windows, bounded delegation, итоговый compressed result вместо общего потока рассуждений.
- Claude Code subagents: subagent имеет отдельные instructions/tools/context и вызывается для специализированной работы, но не становится самостоятельным владельцем пользовательского диалога.

Ссылки:

- https://google-a2a.github.io/A2A/specification/
- https://google-a2a.github.io/A2A/topics/key-concepts/
- https://openai.github.io/openai-agents-js/guides/multi-agent/
- https://openai.github.io/openai-agents-js/guides/handoffs/
- https://openai.github.io/openai-agents-js/guides/tools/
- https://docs.anthropic.com/en/docs/claude-code/sub-agents
- https://www.anthropic.com/engineering/built-multi-agent-research-system

## Базовые инварианты

1. Контракт важнее диалога.

Агенты обмениваются структурированными task/result пакетами. Свободный текст допустим только как часть payload, но не как единственная форма координации.

2. Session не равна transport.

Session хранит контекст, память, transcript, jobs, plan, artifacts и workspace. Telegram, TUI, CLI, HTTP, webhook и event bus — это поверхности и каналы доставки.

3. MIMO по умолчанию.

У session может быть много inputs и много outputs. Задача из Telegram group A может породить результат в Telegram group B, archive channel и webhook.

4. Делегация не блокирует runtime.

Любая agent-agent или delegate работа возвращает `task_id` сразу. Runtime не держит provider loop в ожидании чужой работы. Если агенту логически нужен результат до следующего шага, это выражается зависимостью task graph, callback event или явным `session_wait`, но не блокировкой worker thread и не скрытым synchronous RPC.

5. Единое состояние session.

Несколько каналов могут писать в одну session queue. Это не создаёт несколько prompt histories. Порядок обработки определяется queue policy.

6. Контракт зависит от адресата.

Для человека диалог остаётся первичным интерфейсом. Для agent-agent/subagent взаимодействий первичным является structured contract. Свободный текст между агентами допустим только как человекочитаемая часть payload, а не как единственный источник состояния.

7. PostgreSQL source of truth, NATS event backbone.

Все durable состояния пишутся в PostgreSQL до publish/ack. NATS JetStream не заменяет store; он доставляет события между workers, даёт replay/backpressure и готовит mesh-эволюцию.

## Две плоскости взаимодействия

В архитектуре есть две основные плоскости:

1. `Delegation`: agent -> agent и agent -> subagent.
2. `Routing/Delivery`: session output -> delivery targets.

`Subagent` не является отдельной шиной или третьим видом транспорта. Это bounded-режим делегации: отдельный контекст, ограниченный scope, явный return contract. `Delivery Target` тоже не является взаимодействием агентов; это fan-out результатов.

### Delegation: Agent -> Agent

Используется для асинхронной делегации между равноправными agent profiles.

Контракт:

```text
agent_task {
  task_id,
  source_session_id,
  source_agent_id,
  target_agent_id,
  goal,
  context_refs,
  expected_output,
  constraints,
  reply_policy,
  priority,
  timeout,
  retry_policy,
  fallback_policy,
  trace_id
}
```

Поведение:

- runtime создаёт task/job и возвращает `task_id` сразу;
- target agent работает в отдельной session или продолжает заданную session по политике;
- результат публикуется в result bus;
- task registry обновляется независимо от того, читает ли родитель результат прямо сейчас;
- родитель получает `task_completed` event в свою input queue или видит результат по `/tasks`.
- зависимости между задачами хранятся в task registry/task graph, а не в call stack.

Текущий `message_agent` уже создаёт `task_registry` запись вида `agent_task` и публикует событие `agent_task.created` в `teamd.task.<task_id>`. Для совместимости tool output пока всё ещё отдаёт `recipient_session_id`/`recipient_job_id`; единый user-facing `task_id`/result package contract остаётся следующим API-слоем.

### Delegation: Agent -> Subagent

Используется для рабочей подзадачи под контролем parent run. Это специализация `agent_task`, а не отдельный механизм.

Контракт:

```text
delegate {
  task_id,
  parent_session_id,
  parent_run_id,
  label,
  goal,
  bounded_context,
  write_scope,
  allowed_tools,
  timeout,
  retry_policy,
  return_format
}
```

Поведение:

- subagent не является самостоятельным user-facing агентом;
- subagent не пишет напрямую в operator channels;
- parent остаётся владельцем итогового ответа;
- результат возвращается как package: `summary`, `changed_paths`, `artifact_refs`, `errors`, `next_actions`;
- write scope обязателен для code/file tasks.
- subagent видит только `bounded_context`, `context_refs` и разрешённые artifacts; полный transcript родителя не передаётся по умолчанию.

Текущие `Delegate` jobs и `parent_session_id` уже дают основу, но нужны единый task registry и явное отображение результата.

### Routing/Delivery: Session -> Delivery Target

Используется для маршрутизации результата во внешние каналы.

Контракт:

```text
delivery_target {
  target_id,
  kind,
  address,
  scope,
  owner_user_id,
  allowed_agent_ids,
  allowed_session_ids,
  send_policy,
  format_policy,
  quiet_hours,
  created_at,
  updated_at
}
```

Для Telegram `address` — это `chat_id`, но core не должен знать детали Telegram.

Поведение:

- target регистрируется оператором;
- session может иметь несколько output routes;
- output route имеет свой delivery cursor;
- доставка не меняет selected input session;
- ошибка доставки не должна ломать run, она должна попадать в delivery/task status.

## Зависимости и ожидание результатов

Неблокирующая делегация не означает, что у задач нет зависимостей.

Поддерживаются три модели:

- `callback`: результат подзадачи публикуется как `task_completed` event в очередь родительской session;
- `follow`: оператор подписывает delivery target на конкретную задачу через Telegram `/follow <task_id>` или CLI `agentd task follow <task_id> <target_id>`; при `agent_task.completed|failed|blocked` результат уходит в этот target и фиксируется в `event_deliveries`;
- `query`: оператор или агент читает task registry через Telegram `/tasks`, TUI `\задачи`, CLI `agentd session tasks <session_id>`, CLI `agentd task show <task_id>`, HTTP `GET /v1/sessions/{id}/tasks` или будущий tool;
- `wait`: агент явно вызывает bounded wait (`session_wait`/future task wait) с timeout, когда без результата нельзя продолжать.

Правило: ожидание должно быть явным, ограниченным по timeout и видимым в task registry. Нельзя прятать ожидание внутри транспорта или webhook handler.

## Операторская видимость task registry

Task registry — это не внутренняя таблица “для разработчика”, а пользовательский слой наблюдаемости MIMO.

Минимальный реализованный контракт:

- `Telegram /tasks` показывает делегированные задачи текущей session: `task_id`, `status`, `kind`, owner/executor, попытки, последний update, chain/error/result ref.
- `Telegram /task <task_id>` показывает полную карточку задачи и followers.
- `Telegram /follow <task_id>` регистрирует текущий чат как delivery target и включает доставку результата этой task в этот чат.
- `Telegram /unfollow <task_id>` отключает такую доставку для текущего чата.
- `Telegram /cancel <task_id>` отменяет конкретную task, переводит её в `cancelled` и создаёт task-result event для подписчиков.
- `CLI agentd session tasks <session_id>` показывает тот же список локально; поддерживает `--limit`, `--offset`, `--raw`.
- `CLI agentd task show <task_id>` показывает полную карточку задачи: followers, dependency/context/result/retry JSON, trace, chain/hops, timestamps и ошибку.
- `CLI agentd task follow <task_id> <target_id>`, `agentd task unfollow <task_id> <target_id>`, `agentd task cancel <task_id>` управляют подпиской и отменой локально.
- `TUI \задачи` открывает task browser текущей или выбранной session; `\задача <task_id>` показывает карточку, `\задача отменить <task_id>` отменяет task.
- `HTTP GET /v1/sessions/{session_id}/tasks` отдаёт структурированный список для web UI/TUI.
- `HTTP GET /v1/tasks/{task_id}` и `POST /v1/tasks/{task_id}/cancel` дают daemon-backed TUI тот же task detail/cancel path.

Граница ответственности:

- `/jobs` показывает активные runtime/background jobs внутри session.
- `/tasks` показывает MIMO/delegation tasks из `task_registry`, включая завершённые и упавшие задачи.
- `session_wait` остаётся bounded wait, но оператор не обязан ждать: состояние задачи можно посмотреть отдельно.

## Fan-in layer

Input adapter принимает внешний сигнал и нормализует его в одно из:

```text
input_event {
  event_id,
  dedupe_key,
  source_id,
  source_kind,
  session_id,
  operator_id,
  priority,
  queue_policy,
  payload,
  metadata,
  created_at
}
```

Источники:

- Telegram private/group/chat topic;
- CLI/TUI/HTTP;
- webhook;
- schedule;
- task_completed event;
- NATS topic.

Все inputs пишутся в session queue, но не обязательно исполняются в порядке поступления. Очередь сортируется по:

1. priority;
2. created_at;
3. deterministic event id.

Queue policy задаёт поведение при активном run:

- `fifo`: поставить в очередь;
- `priority`: поставить в очередь с приоритетом;
- `coalesce`: объединить сообщения в окне;
- `restart`: прервать текущий run и начать новый;
- `reject`: отказать с операторским статусом.

Critical events, например monitoring alert severity=critical, должны иметь priority выше routine-запросов.

## Session Core

Session Core не должен знать, откуда пришёл input и куда уйдёт output. Его ответственность:

- загрузить session state;
- собрать prompt по canonical prompt contract;
- выполнить provider loop;
- вызвать tools;
- создать artifacts;
- создать async tasks;
- записать result events;
- вернуть canonical output package.

Session Core не должен:

- отправлять Telegram-сообщения напрямую;
- знать Telegram chat id;
- блокироваться на долгой agent-agent/delegate работе;
- создавать отдельную историю под каждый transport.

## Fan-out layer

Output adapter доставляет result package в один или несколько delivery targets.

Output route:

```text
output_route {
  route_id,
  session_id,
  target_id,
  filter,
  format_policy,
  enabled,
  cursor,
  created_at,
  updated_at
}
```

Политики:

- `full_text`;
- `summary`;
- `artifact_only`;
- `json`;
- `status_only`;
- `errors_only`.

Каждый route имеет собственный cursor. Это важно: один и тот же assistant transcript может быть уже доставлен в admin chat, но ещё не доставлен в alerts chat.

## Routing Engine

Routing Engine применяет декларативные правила:

```text
route_rule {
  rule_id,
  source_filter,
  condition,
  outputs,
  priority,
  enabled
}
```

Примеры:

```text
IF source=telegram_admin THEN output=telegram_admin
IF task_type=server_status THEN output=telegram_ops
IF severity=high THEN output=telegram_alerts + webhook_pager
IF result.kind=artifact AND artifact.kind=report THEN output=original_chat + archive_channel
```

Phase 1 может стартовать без полноценного expression language: достаточно явных session output routes и named delivery targets. Expression rules нужны после стабилизации target registry.

## Task Registry

Task Registry — единая таблица/модель для async работы.

```text
task_registry_entry {
  task_id,
  kind,
  source_session_id,
  owner_agent_id,
  executor_agent_id,
  parent_task_id,
  status,
  created_at,
  updated_at,
  started_at,
  finished_at,
  result_ref,
  error,
  delivery_state,
  dependency_json,
  retry_policy_json,
  attempt_count,
  max_attempts,
  timeout_at,
  chain_id,
  hop_count,
  max_hops,
  trace_id
}
```

Kinds:

- `agent_task`;
- `delegate`;
- `schedule_fire`;
- `delivery`;
- `tool_background`;
- `webhook`.

Status:

- `queued`;
- `running`;
- `waiting_input`;
- `completed`;
- `failed`;
- `cancelled`;
- `blocked`;
- `timed_out`;
- `dead_lettered`.

Ошибки:

- timeout переводит task в `timed_out`;
- превышение retry policy переводит task в `failed` или `dead_lettered`;
- invalid result contract сохраняется как `failed` с `result_validation_error`;
- delivery failure не откатывает completed run, а создаёт failed delivery task/event.

Chains:

- `chain_id`, `hop_count`, `max_hops` остаются обязательными для agent-agent цепочек;
- `grant_agent_chain_continuation` должен работать как явное увеличение budget по chain;
- result events должны сохранять chain metadata, чтобы audit показывал путь задачи.

Операторские команды:

- `/tasks` — список активных/последних tasks текущей session;
- `/task <id>` — детали task;
- `/cancel <id>` — отменить task;
- `/follow <id>` — подписаться на updates.

## Event/Result Bus

Event bus состоит из двух слоёв:

- PostgreSQL — durable source of truth;
- NATS JetStream — delivery backbone для workers.

PostgreSQL-таблицы текущего vertical slice:

- `event_sources` — зарегистрированные входы;
- `router_rules` — декларативные правила маршрутизации;
- `inbound_events` — нормализованные входящие события с `dedupe_key`;
- `routed_events` — привязка input event к session/agent;
- `event_outbox` — durable publish queue;
- `event_deliveries` — результат доставки output в target;
- `task_registry` — состояние async/background work.

Существующие runtime records также остаются частью bus contract:

- terminal job result;
- `session_inbox_events`;
- task registry updates;
- delivery requests;
- trace links.

Событие:

```text
task_completed {
  event_id,
  task_id,
  source_session_id,
  target_session_id,
  result_ref,
  summary,
  status,
  created_at
}
```

Result bus обязан доставить событие:

- в queue родительской session;
- в task registry;
- в настроенные output routes;
- в observability trace.

Webhook/runtime flow сейчас:

```text
Telegram webhook
  -> inbound_events + event_outbox(teamd.input.telegram)
  -> rule router
  -> routed_events + event_outbox(teamd.session.<session_id>.input)
  -> session worker
  -> canonical App::execute_chat_turn(...)
  -> runs/transcripts + task_registry + event_outbox(teamd.session.<session_id>.output)
  -> delivery worker
  -> event_deliveries + delivery target cursor
```

Agent-agent task flow сейчас:

```text
message_agent
  -> recipient session + interagent_message job
  -> task_registry(kind=agent_task, status=queued)
  -> event_outbox(teamd.task.<task_id>, agent_task.created)
  -> task worker
  -> canonical background job executor
  -> target session run/transcript
  -> task_registry(status=completed|failed|blocked, result_ref)
  -> event_outbox(teamd.task.<task_id>, agent_task.completed|failed|blocked)
  -> parent session inbox event
```

Если `event_bus.required = true`, обычный background worker не исполняет task-backed `interagent_message`/`delegate` jobs напрямую. Это защищает от гонки: task исполняется через `teamd.task.*` consumer.

Webhook handler не запускает модель и не пишет transcript. Он только валидирует secret, нормализует update, дедуплицирует `telegram:update:<update_id>`, сохраняет inbound event и outbox envelope.

## Idempotency

Каждый внешний input имеет `dedupe_key`.

Примеры:

- Telegram webhook: `telegram:update:<update_id>`;
- schedule fire: `schedule:<schedule_id>:<planned_fire_at>`;
- A2A/task callback: `task:<task_id>:completed:<attempt>`;
- delivery: `delivery:<route_id>:<output_event_id>`.

Повторный input с тем же `dedupe_key` не создаёт новый run. Он возвращает существующий event/task status.

JetStream ack не является source of truth. Ack выполняется только после durable write в PostgreSQL.

## Context Isolation

Agent-agent и subagent work не получают полный контекст родителя по умолчанию.

Передаются только:

- `goal`;
- `constraints`;
- `bounded_context`;
- `context_refs`;
- `allowed_tools`;
- `write_scope`;
- `return_format`.

Доступ к секретам, workspace и artifacts должен быть явно разрешён. Для subagent code/file tasks `write_scope` обязателен.

NATS subjects текущего backbone:

- `teamd.input.<source_kind>`;
- `teamd.session.<id>.input`;
- `teamd.session.<id>.output`;
- `teamd.task.<task_id>`;
- `teamd.delivery.<target_id>`;
- `teamd.dlq`.

Контракты при этом не меняются.

## Telegram сценарий: статус сервера в другой чат

Целевой flow:

1. Оператор добавляет bot в группу `ops`.
2. В группе вызывает `/target register ops-status`.
3. В основном чате создаёт monitor agent/session.
4. Оператор или агент настраивает output route:

```text
/attach_output ops-status
```

5. Агент создаёт schedule:

```text
schedule_create {
  id: "server-status-15m",
  agent_identifier: "monitor",
  prompt: "Check server status and produce short operator summary.",
  interval_seconds: 900,
  mode: "interval",
  delivery_mode: "existing_session",
  target_session_id: "<monitor_session_id>"
}
```

6. Background worker запускает scheduled job.
7. Session Core создаёт assistant output.
8. Fan-out доставляет output в `ops-status`, а не обязательно в исходный чат.
9. `/tasks` показывает последний schedule_fire и delivery status.

## Фазы реализации

### Phase 1: Modular Monolith

Цель: MIMO внутри одного `agentd`.

Базовый vertical slice:

1. Ввести delivery target registry.
2. Ввести session output routes с отдельными cursors.
3. Добавить Telegram commands для target/route management.
4. Расширить pending assistant delivery: selected chat binding остаётся, но дополнительно работают output routes.
5. Добавить task registry view поверх существующих jobs/schedules/inter-agent/delegate.
6. Обновить tool definitions и prompts: agent-agent/delegate возвращают `task_id`, не требуют ожидания.

### Phase 2: Event-driven backbone

Цель: вынести result bus/input bus на шину без переписывания core.

Шина: NATS JetStream.

Причины:

- простая операционная модель;
- lightweight pub/sub и queue consumers;
- request/reply;
- JetStream для durability, ack/replay/backpressure;
- хорошая fit-модель для локального mesh и нескольких узлов.

На этом этапе PostgreSQL остаётся source of truth, а NATS — обязательный event backbone. Telegram long polling заменяется на webhook ingress внутри `agentd`; webhook только сохраняет/publish input event и не запускает модель напрямую.

Реализованный vertical slice:

1. обязательная конфигурация `event_bus`/`telegram.mode = "webhook"` для event runtime;
2. NATS JetStream client и stream setup;
3. Telegram webhook ingress;
4. rule router;
5. session input worker поверх canonical `App::execute_chat_turn`;
6. delivery worker поверх `delivery_targets`/`session_output_routes`;
7. task worker поверх `task_registry`/`teamd.task.*` для `message_agent`;
8. e2e smoke `cmd/agentd/tests/event_runtime_smoke.rs`.

Phase 2 data flow:

```text
Telegram webhook
  -> inbound_events
  -> NATS teamd.input.telegram
  -> rule router
  -> routed_events
  -> NATS teamd.session.<session_id>.input
  -> session worker
  -> NATS teamd.session.<session_id>.output
  -> delivery worker

message_agent inside session worker
  -> task_registry + NATS teamd.task.<task_id>
  -> task worker
  -> target agent session
  -> task result + parent inbox event
```

### Phase 3: Autonomous Mesh

Цель: несколько узлов и dynamic agent discovery.

Нужно добавить:

- agent capability registry;
- task ownership leasing;
- duplicate suppression;
- leader election или deterministic ownership для recurring tasks;
- remote A2A adapters;
- mesh health/status UI.

## Что не делаем

- Не переписываем provider loop.
- Не создаём отдельный Telegram chat loop.
- Не делаем route expression language до registry/output routes.
- Не даём subagent прямой доступ писать пользователю.
- Не смешиваем session state и delivery target state.
- Не считаем NATS source of truth: durable state сначала PostgreSQL, потом publish/ack.

## Миграция от Telegram-centric runtime

Переход staged, но без второго chat path:

1. Ввести event/router таблицы и NATS health без изменения текущего polling-поведения.
2. Включить webhook ingress в тестовом окружении и проверить dedupe по Telegram `update_id`.
3. Подключить rule router, который воспроизводит текущую привязку private chat -> selected session/default agent.
4. Подключить session worker к canonical App/runtime chat path.
5. Подключить delivery worker к существующим `delivery_targets`/`session_output_routes`.
6. Подключить task worker к `task_registry`/`teamd.task.*` для agent-agent/delegate работы.
7. Перевести production Telegram bot с long polling на webhook.
8. Удалить polling worker как legacy после стабилизации webhook runtime.

На каждом шаге invariant: один session transcript, один provider loop, один prompt assembly path.

## Открытые решения

1. Должен ли `message_agent` быть переименован/дополнен новым `agent_task_create`, или оставить старый tool как compatibility alias?
2. Нужно ли agent_task разрешать писать в delivery target напрямую, или только через route родительской/session?
3. Должен ли `/attach_output` быть session-scoped или agent-profile-scoped?
4. Какие route policies нужны первыми: `full_text`, `summary`, `status_only`, `errors_only`?
5. Нужно ли выводить task registry в prompt как `AutonomyState`, и в каком бюджете?

## Решение на сейчас

Текущий production-safe путь:

```text
Telegram delivery target registry
  -> session output routes
  -> route-aware assistant transcript delivery
  -> operator commands /target, /outputs
  -> docs/tests
```

Это закрывает основной пользовательский кейс: агент работает в одной session, а результаты может отправлять в другой Telegram chat/group по явной регистрации и route policy.

Следующий experimental путь:

```text
Telegram webhook
  -> NATS JetStream
  -> router/session/delivery workers
```

Его включают только через `telegram.mode = "webhook"` и `event_bus.required = true`.
