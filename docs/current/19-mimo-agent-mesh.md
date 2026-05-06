# MIMO agent mesh

Этот документ фиксирует целевую архитектуру MIMO-взаимодействий в `teamD`.

Статус: design baseline для следующей волны работ. Это не описание полностью реализованного состояния.

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

Session хранит контекст, память, transcript, jobs, plan, artifacts и workspace. Telegram, TUI, CLI, HTTP, webhook или будущая event bus — это поверхности и каналы доставки.

3. MIMO по умолчанию.

У session может быть много inputs и много outputs. Задача из Telegram group A может породить результат в Telegram group B, archive channel и webhook.

4. Родитель не блокируется на делегации.

Любая agent-agent или delegate работа возвращает `task_id` сразу. Родитель продолжает обрабатывать входы. Результат возвращается через result bus и task registry.

5. Единое состояние session.

Несколько каналов могут писать в одну session queue. Это не создаёт несколько prompt histories. Порядок обработки определяется queue policy.

6. Modular monolith first.

Phase 1 реализуется внутри текущего `agentd`: PostgreSQL + background worker + typed repositories. NATS или другая шина появляется только в Phase 2.

## Три независимых механизма

### Agent -> Agent

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
  trace_id
}
```

Поведение:

- runtime создаёт task/job и возвращает `task_id` сразу;
- target agent работает в отдельной session или продолжает заданную session по политике;
- результат публикуется в result bus;
- task registry обновляется независимо от того, читает ли родитель результат прямо сейчас;
- родитель получает `task_completed` event в свою input queue или видит результат по `/tasks`.

Текущий `message_agent` близок к этому, но пока отдаёт `recipient_session_id`/`recipient_job_id`, а не единый `task_id`/result package contract.

### Agent -> Subagent

Используется для рабочей подзадачи под контролем parent run.

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
  return_format
}
```

Поведение:

- subagent не является самостоятельным user-facing агентом;
- subagent не пишет напрямую в operator channels;
- parent остаётся владельцем итогового ответа;
- результат возвращается как package: `summary`, `changed_paths`, `artifact_refs`, `errors`, `next_actions`;
- write scope обязателен для code/file tasks.

Текущие `Delegate` jobs и `parent_session_id` уже дают основу, но нужны единый task registry и явное отображение результата.

### Session -> Delivery Target

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

## Fan-in layer

Input adapter принимает внешний сигнал и нормализует его в одно из:

```text
input_event {
  event_id,
  source_id,
  source_kind,
  session_id,
  operator_id,
  priority,
  payload,
  metadata,
  dedupe_key,
  created_at
}
```

Источники:

- Telegram private/group/chat topic;
- CLI/TUI/HTTP;
- webhook;
- schedule;
- task_completed event;
- будущий NATS topic.

Все inputs пишутся в session queue. Очередь имеет policy: `fifo`, `priority`, `coalesce`, `rate_limited`, `restart`.

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
- `blocked`.

Операторские команды:

- `/tasks` — список активных/последних tasks текущей session;
- `/task <id>` — детали task;
- `/cancel <id>` — отменить task;
- `/follow <id>` — подписаться на updates.

## Result Bus

В Phase 1 result bus — это не NATS. Это contract поверх PostgreSQL:

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

В Phase 2 result bus может стать NATS:

- `teamd.session.<id>.input`;
- `teamd.task.<id>.events`;
- `teamd.agent.<id>.tasks`;
- `teamd.delivery.<target_id>.requests`.

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

Минимальный vertical slice:

1. Ввести delivery target registry.
2. Ввести session output routes с отдельными cursors.
3. Добавить Telegram commands для target/route management.
4. Расширить pending assistant delivery: selected chat binding остаётся, но дополнительно работают output routes.
5. Добавить task registry view поверх существующих jobs/schedules/inter-agent/delegate.
6. Обновить tool definitions и prompts: agent-agent/delegate возвращают `task_id`, не требуют ожидания.

### Phase 2: Event-driven backbone

Цель: вынести result bus/input bus на шину без переписывания core.

Рекомендуемая шина: NATS.

Причины:

- простая операционная модель;
- lightweight pub/sub;
- request/reply;
- JetStream для durability;
- хорошая fit-модель для локального mesh и нескольких узлов.

На этом этапе PostgreSQL остаётся source of truth, а NATS — delivery/notification backbone.

### Phase 3: Autonomous Mesh

Цель: несколько узлов и dynamic agent discovery.

Нужно добавить:

- agent capability registry;
- task ownership leasing;
- duplicate suppression;
- leader election или deterministic ownership для recurring tasks;
- remote A2A adapters;
- mesh health/status UI.

## Что не делаем в Phase 1

- Не внедряем NATS сразу.
- Не переписываем provider loop.
- Не создаём отдельный Telegram chat loop.
- Не делаем route expression language до registry/output routes.
- Не даём subagent прямой доступ писать пользователю.
- Не смешиваем session state и delivery target state.

## Открытые решения

1. Должен ли `message_agent` быть переименован/дополнен новым `agent_task_create`, или оставить старый tool как compatibility alias?
2. Нужно ли agent_task разрешать писать в delivery target напрямую, или только через route родительской/session?
3. Должен ли `/attach_output` быть session-scoped или agent-profile-scoped?
4. Какие route policies нужны первыми: `full_text`, `summary`, `status_only`, `errors_only`?
5. Нужно ли выводить task registry в prompt как `AutonomyState`, и в каком бюджете?

## Решение на сейчас

Начинаем с Phase 1.

Первый production-worthy slice:

```text
Telegram delivery target registry
  -> session output routes
  -> route-aware assistant transcript delivery
  -> operator commands /target, /outputs
  -> docs/tests
```

Это закрывает основной пользовательский кейс: агент работает в одной session, а результаты может отправлять в другой Telegram chat/group по явной регистрации и route policy.
