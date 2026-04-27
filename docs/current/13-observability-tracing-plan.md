# Observability tracing: план и текущий v1

Статус: v1 частично реализован, полный OTel/exporter ещё не принят.

Дата фиксации: 2026-04-25.

Этот документ фиксирует идею “пронизывать” пользовательский запрос, run модели, tool calls, результаты, artifacts, delivery и межагентские переходы единым trace, а поверх traces собирать аналитику качества, задержек и использования.

Текущий v1:

- `DiagnosticEvent` получил first-class поля `trace_id`, `span_id`, `parent_span_id`, `surface`, `entrypoint`;
- Telegram delivery пишет audit events `component=telegram`, `op=delivery.request`/`delivery.retry`, `surface=telegram`;
- появился локальный CLI-срез `agentd analytics [N]` / `teamdctl analytics [N]`;
- analytics читает tail `audit/runtime.jsonl` и tool ledger из SQLite;
- OTLP exporter, Jaeger integration, span tree в store и trace propagation между всеми runtime сущностями пока не реализованы.

## Проблема

Сейчас `teamD` уже хранит много связей:

- `session_id`;
- `run_id`;
- `job_id`;
- `tool_call_id`;
- `artifact_id`;
- `chain_id` для inter-agent;
- Telegram bindings/cursors;
- structured audit events.

Но эти связи не образуют единую причинную картину. Оператор часто может увидеть “что произошло”, но не всегда быстро видит “почему это произошло” и “какое событие породило это действие”.

Пример:

```text
Telegram message -> run -> provider round -> web_search -> empty result -> assistant answer -> Telegram delivery
```

Сейчас такую цепочку можно восстановить через debug-view, logs и tool ledger, но это реконструкция по времени и идентификаторам. Trace сделал бы эту цепочку first-class сущностью.

Отдельная проблема — аналитика. Сейчас можно вручную расследовать отдельный случай, но сложно ответить на агрегированные вопросы:

- какие сценарии чаще всего успешны;
- какие tools чаще дают пустой или ошибочный результат;
- где latency: provider, tool, queue, approval или delivery;
- какие surfaces дают больше неудачных диалогов;
- какие запросы приводят к повторным исправлениям пользователя;
- сколько стоит turn по токенам и времени;
- какие изменения tool definitions реально улучшили поведение.

## Идея

Ввести внутреннюю trace-модель, совместимую с OpenTelemetry:

- `trace_id`;
- `span_id`;
- `parent_span_id`;
- `links`;
- `span_kind`;
- `started_at`;
- `finished_at`;
- `status`;
- `attributes`;
- `events`.

Поверх неё хранить доменные attributes `teamD`:

- `session_id`;
- `run_id`;
- `job_id`;
- `agent_id`;
- `tool_name`;
- `tool_call_id`;
- `artifact_id`;
- `chain_id`;
- `hop_count`;
- `node_id`;
- `surface`;
- `entrypoint`.

Для async/workflow сценариев дополнительно рассмотреть:

- `correlation_id` — “это всё одна пользовательская история”;
- `causation_id` — “какое конкретное событие породило это событие”.

`correlation_id` и `causation_id` не являются core-полями OpenTelemetry, но часто используются в event-driven/CQRS/saga системах. В OTel их можно хранить как attributes.

## Что это даст

### Оператору

Trace-view может показать причинное дерево, а не плоский timeline:

```text
trace telegram.message
  span session.run
    span provider.request round=1
      span tool.web_search
        span artifact.write
    span provider.request round=2
  span telegram.send
```

Это ускоряет ответы на вопросы:

- откуда пришёл turn: Telegram, TUI, CLI, HTTP, schedule, inter-agent;
- какой run породил tool call;
- какой tool call породил artifact;
- был ли ответ реально доставлен в Telegram;
- где случилась задержка;
- какой async wakeup связан с исходной просьбой пользователя.

### Разработчику

Trace позволит быстрее разбирать дефекты:

- `web_search` вернул пусто, потому что parser не нашёл результаты;
- `continue_later` создал wakeup, но delivery не дошла;
- Judge ответил, но origin session не подхватила reply;
- A2A peer принял delegate job, но callback не вернулся;
- TUI упал на operator command, хотя ошибка должна была остаться внутри timeline.

### Улучшению tools и prompt contract

Trace может стать источником статистики:

- какие tool calls чаще приводят к ошибкам;
- какие arguments модель выбирает неудачно;
- какие tool definitions надо уточнить;
- где модель делает ложные утверждения о delivery/tool results;
- сколько provider rounds нужно до успешного ответа.

### Модели

Модели не нужен полный raw trace в prompt. Это было бы дорого и шумно.

Польза для модели возможна через bounded `TraceSummary` / `TurnContext` block:

```text
Current turn trace:
- origin: telegram message from activated user
- current run: retry after tool web_search returned 0 results
- relevant tool result: web_search query="..." results=0
- pending delivery: none
- expected next action: answer user or retry with improved query
```

То есть trace остаётся audit/debug слоем, а в prompt попадает только компактная выжимка текущего causal context.

### Аналитике

Поверх trace-модели можно строить отдельный analytics слой:

- latency по компонентам: входящий surface, очередь, provider, tool, delivery;
- tool usage: частота, error rate, empty result rate, retries;
- provider usage: tokens, model, rounds per answer, transient failures;
- quality signals: explicit feedback, повторный follow-up “не работает”, manual correction, operator mark;
- funnel: входящий запрос, ответ, delivery, follow-up, satisfaction;
- популярные intent categories, если они будут классифицироваться отдельным bounded процессом.

Важно: analytics не должен быть просто “Jaeger с текстами сообщений”. Для качества нужны агрегаты и безопасные ссылки на локальные сущности, а не raw payload в observability backend.

## Как это связано с OpenTelemetry и Jaeger

Идея совместима с OpenTelemetry:

- `trace_id`, `span_id`, `parent_span_id`, `links`, `attributes`, `events`, `status` — стандартная модель OTel;
- `traceparent` можно использовать для A2A/HTTP propagation;
- OTLP exporter позволит отправлять traces в Jaeger, Tempo или OpenTelemetry Collector.

Jaeger сможет показать:

- flamegraph/timeline;
- задержки по spans;
- ошибки;
- attributes вроде `session_id`, `run_id`, `tool_name`, `surface`.

Metrics backend сможет показать:

- `teamd_turn_duration_ms`;
- `teamd_provider_tokens_total`;
- `teamd_tool_calls_total`;
- `teamd_tool_errors_total`;
- `teamd_tool_empty_results_total`;
- `teamd_delivery_failures_total`;
- `teamd_user_feedback_total`;
- p50/p95/p99 latency по surface/agent/model/tool.

Но Jaeger не заменяет `teamD` debug-view:

- Jaeger не должен хранить большие transcript/tool outputs;
- sensitive payload надо маскировать до export;
- доменные сущности `Session`, `Run`, `ToolCall`, `Artifact` удобнее читать через локальный debug-view;
- локальный runtime должен оставаться полезным без внешнего observability stack.
- analytics backend не должен быть единственным местом, где хранится качество диалогов: локальный store остаётся источником истины.

Рекомендуемая траектория:

1. Добавить trace fields в diagnostic audit events. Статус: сделано для `DiagnosticEvent`.
2. Добавить локальные агрегаты поверх audit/tool ledger. Статус: сделано минимально через `agentd analytics [N]`.
3. Ввести внутреннюю trace-модель в store для runs/tool calls/artifacts/transcripts. Статус: не сделано.
4. Обновить debug-view, чтобы строить trace tree локально. Статус: не сделано.
5. Добавить propagation для schedules, subagents, inter-agent и A2A. Статус: не сделано.
6. Только потом добавить OTLP exporter и metrics exporter. Статус: не сделано.

## Surfaces и entrypoints

Каждый root span должен явно отвечать на вопрос “откуда пришёл turn”.

Примеры attributes:

```text
surface=telegram
entrypoint=telegram.message
telegram.chat_id=...
telegram.user_id=...
telegram.message_id=...
session_id=session-...
agent_id=default
```

```text
surface=tui
entrypoint=tui.command
command=\дебаг
operator_id=local:<uid>
session_id=session-...
```

```text
surface=scheduler
entrypoint=schedule.fire
schedule_id=...
delivery_mode=existing_session
session_id=session-...
```

```text
surface=interagent
entrypoint=agent.message
source_session_id=...
source_agent_id=default
target_agent_id=judge
chain_id=...
hop_count=1
```

## Inter-agent, schedules и A2A

Для прямой синхронной работы достаточно parent/child.

Для async и distributed переходов нужны links:

```text
span tool.continue_later
  schedule_id=schedule-...

span schedule.fire
  link -> tool.continue_later
```

```text
span tool.message_agent target=judge
  creates recipient session/job

span interagent.receive
  link -> tool.message_agent
  chain_id=...
```

```text
span a2a.outbound.delegate
  peer_node=ams-2
  traceparent=00-<trace_id>-<span_id>-01

span a2a.inbound.delegate
  remote_parent_span=<span_id>
```

В локальном debug-view это можно показывать лучше, чем в Jaeger: как дерево пользовательского intent, где видны default agent, judge, child sessions, A2A peer и callbacks.

## Analytics слой

Trace отвечает на вопрос “что породило что”.

Analytics отвечает на вопросы:

- насколько хорошо система работает;
- где она медленная;
- какие tools и surfaces проблемные;
- какие изменения улучшили или ухудшили качество.

Эти задачи связаны, но не одинаковы. Поэтому plan должен разделять:

- `trace_spans` — причинная структура выполнения;
- `metrics` — числовые агрегаты;
- `quality_events` — явные или эвристические сигналы качества;
- `local payloads` — transcripts, tool outputs, artifacts, которые остаются в `teamD` store.

### Что можно отправлять в OpenTelemetry attributes

Безопасные/полезные поля:

```text
surface
entrypoint
session_id
run_id
job_id
agent_id
model
tool_name
tool_call_id
artifact_id
status
error_kind
duration_ms
tokens_input
tokens_output
tokens_total
provider_round_count
tool_result_count
tool_empty_result=true|false
delivery_status
feedback_kind
```

Поля с высокой чувствительностью или cardinality:

```text
user_message
assistant_answer
full_prompt
tool_output
telegram username/full name
API keys/secrets
raw external URL query strings with private data
```

Их нельзя по умолчанию класть в span attributes. Вместо этого:

- хранить payload локально в `transcripts`/`artifacts`;
- экспортировать `transcript_entry_id`, `artifact_id`, `tool_call_id`;
- опционально экспортировать `redacted_preview`;
- опционально экспортировать `content_hash`.

### Quality signals

Качество нельзя честно вывести только из `status=ok`: модель может успешно ответить технически, но плохо по смыслу.

Возможные сигналы:

- `explicit_positive_feedback` — пользователь явно отметил ответ как хороший;
- `explicit_negative_feedback` — пользователь явно отметил ответ как плохой;
- `operator_mark` — оператор вручную пометил trace;
- `followup_correction` — следующий user message похож на “не работает”, “не то”, “почему”, “ты ошибся”;
- `tool_empty_result` — tool вернул пустой результат, но модель продолжила отвечать;
- `delivery_failed` — ответ был сгенерирован, но не доставлен surface;
- `approval_denied` — оператор отказал tool/action;
- `run_failed` — run завершился ошибкой.

Эти события должны быть отдельными facts, а не неявными комментариями в transcript.

Возможная таблица:

```text
quality_events
  id TEXT
  trace_id TEXT
  session_id TEXT
  run_id TEXT NULL
  kind TEXT
  source TEXT        -- user/operator/runtime/heuristic/model_judge
  confidence REAL
  attributes_json TEXT
  created_at INTEGER
```

Открытый вопрос: разрешать ли `model_judge` как источник качества. Это может быть полезно для offline анализа, но нельзя подменять им пользовательскую оценку.

### Metrics

Минимальные counters/histograms:

```text
turns_total{surface,agent_id,status}
turn_duration_ms{surface,agent_id,model}
provider_rounds_total{model,status}
provider_tokens_total{model,kind=input|output|total}
tool_calls_total{tool_name,status}
tool_duration_ms{tool_name,status}
tool_empty_results_total{tool_name}
deliveries_total{surface,status}
feedback_total{surface,kind}
```

Для локального режима можно начать с materialized SQL queries или отдельной `analytics_events` таблицы. Для внешнего режима можно экспортировать в OpenTelemetry metrics/Prometheus-compatible backend.

### Dataset для улучшения качества

Для улучшения prompts/tools нужна не только telemetry, но и dataset:

```text
case_id
trace_id
session_id
run_id
surface
user_message_ref
assistant_answer_ref
tool_call_refs
artifact_refs
quality_labels
operator_notes
created_at
```

Payload refs указывают на локальные transcripts/artifacts. Это позволяет строить curated набор “плохих” и “хороших” кейсов без копирования чувствительных данных в observability backend.

## Возможная схема хранения

Минимальная таблица:

```text
trace_spans
  trace_id TEXT
  span_id TEXT
  parent_span_id TEXT NULL
  name TEXT
  kind TEXT
  status TEXT
  started_at INTEGER
  finished_at INTEGER NULL
  attributes_json TEXT
  events_json TEXT
  links_json TEXT
```

Дополнительные analytics таблицы, если идея будет принята:

```text
analytics_events
  id TEXT
  trace_id TEXT
  span_id TEXT NULL
  name TEXT
  attributes_json TEXT
  value_json TEXT
  created_at INTEGER

quality_events
  id TEXT
  trace_id TEXT
  session_id TEXT
  run_id TEXT NULL
  kind TEXT
  source TEXT
  confidence REAL
  attributes_json TEXT
  created_at INTEGER
```

Связи в существующих таблицах:

```text
transcripts.trace_id, transcripts.span_id
runs.trace_id, runs.span_id
jobs.trace_id, jobs.span_id
tool_calls.trace_id, tool_calls.span_id
artifacts.trace_id, artifacts.span_id
```

Открытый вопрос: хранить trace fields прямо в каждой таблице или держать отдельную таблицу связей `trace_entity_links`.

## Риски

- Слишком ранняя интеграция OTel может протащить внешнюю терминологию внутрь доменной модели.
- Trace может начать дублировать `runtime.jsonl`, `runs`, `jobs` и `tool_calls`.
- Analytics может начать дублировать trace и превратиться во вторую базу истины.
- Большие payload нельзя экспортировать в Jaeger/OTLP.
- Нужно продумать redaction секретов, prompt text, Telegram IDs и user content.
- Raw user/assistant text в span attributes создаёт риск приватности, высокую cardinality и тяжёлые индексы.
- Есть риск раздуть prompt, если raw trace начнут вставлять модели.
- Нужно сохранить один canonical runtime path, а не сделать отдельный observability path.

## Открытые вопросы

1. Должен ли `trace_id` соответствовать одному user intent, одному run или одной session?
2. Нужны ли `correlation_id`/`causation_id`, если есть OTel links?
3. Какой минимальный trace полезен без Jaeger?
4. Что именно модель должна видеть: `TraceSummary`, `TurnContext` или ничего?
5. Какой уровень redaction нужен для Telegram/operator данных?
6. Нужно ли делать exporter в Jaeger сразу или сначала локальный trace debug-view?
7. Где граница между `runtime.jsonl` diagnostic events и trace spans?
8. Какие quality signals считаются достаточными для аналитики качества?
9. Нужен ли отдельный curated dataset для успешных/неуспешных диалогов?
10. Какие поля можно экспортировать наружу, а какие должны оставаться только локально?
11. Как не допустить survivorship bias: логировать только успешные диалоги нельзя, нужны и успешные, и неуспешные traces.

## Предварительная позиция

Если принимать эту идею, начинать стоит не с Jaeger, а с локальной domain-compatible trace-модели. Она должна быть OTel-compatible, но не OTel-driven.

Первый полезный результат:

- debug-view умеет открыть “trace текущего turn”;
- видно root surface/entrypoint;
- видно provider rounds;
- видно tool calls и artifacts;
- видно async links для schedule/inter-agent/A2A.

OTLP/Jaeger exporter стоит делать только после того, как локальная модель доказала пользу.

Analytics стоит развивать параллельно, но как отдельный слой:

- trace/debug отвечает за causality;
- metrics отвечает за агрегаты;
- quality dataset отвечает за улучшение prompts/tools;
- raw payload остаётся локальным и экспортируется только явно, после redaction.
