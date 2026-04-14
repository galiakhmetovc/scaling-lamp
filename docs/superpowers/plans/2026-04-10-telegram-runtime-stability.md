# Telegram Runtime Stability Plan

## Goal

Убрать зависания и сделать single-agent Telegram runtime управляемым в реальном использовании.

План покрывает только фактически проявившиеся проблемы:
- зависший `provider.Generate`
- бесполезный `/cancel` при уже зависшем run
- блокирующий poll loop
- немые зависания status card
- runaway tool/model loops без внятного stop condition

## Scope

In scope:
- async execution model для Telegram updates
- per-run provider timeout
- реальная отмена активного run
- защита от параллельных run в одном chat
- улучшение status-card observability
- trace/log visibility для stuck provider rounds

Out of scope:
- mesh orchestration
- skill protocol evolution
- redesign prompts/skills semantics
- multi-agent routing

## Current Failures

### 1. Blocking Poll Loop

Сейчас `cmd/coordinator/main.go` делает:
- `Poll()`
- `Reply()`
- ждёт завершения `Reply()`
- только потом читает следующий Telegram update

Следствие:
- если один run завис на `provider.Generate`
- бот перестаёт обрабатывать новые сообщения
- `/cancel` и callback `run:cancel` не доходят до runtime

### 2. No Hard Provider Round Timeout

Сейчас длинный или зависший `z.ai` запрос:
- может висеть слишком долго
- не даёт явной ошибки пользователю
- выглядит как вечное `Думаю над ответом`

### 3. Runaway Validation Loops

После снятия лимита tool rounds модель может:
- продолжать проверять один и тот же сценарий
- запускать новые tool rounds без сильного прогресса
- держать пользователя в бесконечном run

Проблема не в самом отсутствии лимита, а в отсутствии:
- stop policy
- cancel path
- timeout policy

### 4. Status Card Desync

Даже когда run ещё жив:
- карточка может не обновляться
- пользователь не понимает, stuck это или идёт progress

Часть проблем уже смягчена:
- live card режется до последних шагов

Но всё ещё нет:
- явного stuck-state
- явной причины последнего ожидания
- отдельного статуса для provider timeout / provider wait

### 5. Advisory Answer Drift

Модель может уже выйти на полезный advisory answer, но вместо финализации:
- запрашивает ещё один tool round
- уходит в дополнительный research
- не возвращает итог пользователю вовремя

Типичный сценарий:
- user asks for recommendation/opinion
- assistant уже формулирует вывод
- затем делает ещё одну “проверку для уверенности”
- run затягивается, отменяется или рвётся до финального user-facing ответа

Проблема не в самих tools, а в отсутствии stop policy для advisory queries.

### 6. Stale Checkpoint Without Intent

Checkpoint может пережить исходный user intent дольше, чем сам текст задачи в live history.

Сценарий:
- старая тема уже compacted в checkpoint
- потом идёт длинный tool-heavy run на другой теме
- оригинальный user prompt по старой теме выпадает из живой истории
- checkpoint при этом остаётся
- позже пользователь пишет короткое `Ну?` / `Ау?`
- модель видит summary старой темы, но не видит исходную формулировку запроса

Следствие:
- модель честно говорит `потерял контекст`
- но для пользователя это выглядит как бессмысленный провал continuity

Проблема в том, что checkpoint хранит факт темы, но не хранит безопасно её исходный intent в форме, достаточной для продолжения.

## Target Behavior

### Runtime Model

Telegram adapter должен работать так:
1. poll loop только читает updates
2. каждый новый normal message запускается в отдельной goroutine/task
3. `/cancel` и callback `run:cancel` обрабатываются независимо от активного run
4. на один chat одновременно допускается только один normal run
5. во время активного run:
   - новые обычные сообщения отклоняются
   - `/status` и `/cancel` работают

### Provider Safety

Каждый `provider.Generate` должен иметь:
- hard timeout
- явную ошибку `provider round timed out`
- запись этого факта в trace/status/log

### Cancellation

Отмена должна:
- доходить, даже если основной run висит
- отменять `runCtx`
- прерывать активный provider request
- менять status card на `Отменено`
- возвращать пользователю отдельное сообщение об отмене

## Implementation Slices

### Slice 1. Async Telegram Runs

#### Goal

Развязать poll loop и выполнение run.

#### Changes

- в `cmd/coordinator/main.go`
  - перестать выполнять `adapter.Reply()` inline
  - normal updates обрабатывать в отдельной goroutine
  - command updates `\/status`, `\/cancel`, callbacks обрабатывать сразу

- в `internal/transport/telegram/adapter.go`
  - добавить lightweight gate:
    - если в chat уже активен normal run
    - новый normal text -> ответ `уже выполняю запрос, используй /status или /cancel`

#### Acceptance

- зависший run не блокирует приём `/cancel`
- зависший run не блокирует `/status`
- второй обычный запрос не стартует новый run поверх текущего

### Slice 2. Provider Round Timeout

#### Goal

Каждый LLM round должен иметь верхнюю границу ожидания.

#### Changes

- в `internal/transport/telegram/adapter.go`
  - оборачивать каждый `provider.Generate` в `context.WithTimeout`

- в `internal/config/config.go`
  - добавить env:
    - `TEAMD_PROVIDER_ROUND_TIMEOUT`

- в trace
  - фиксировать timeout как отдельную ошибку round-а

#### Acceptance

- если `z.ai` не ответил за timeout
  - run падает явно
  - пользователь видит нормальную ошибку
  - trace содержит timeout, а не просто обрыв

### Slice 3. Explicit Stuck-State Telemetry

#### Goal

Показать пользователю, где именно run ждёт.

#### Changes

- в `RunState`
  - добавить поля:
    - `WaitingOn`
    - `LastProgressAt`
    - `RoundIndex`

- status card должна уметь показывать:
  - `Ожидаю ответ модели`
  - `Ожидаю инструмент`
  - `Отмена запрошена`
  - `Provider timeout`

#### Acceptance

- в зависшем run видно, что бот ждёт именно provider, а не tool

### Slice 4. Cancel Path Verification

#### Goal

Проверить end-to-end, что cancel реально прерывает долгий run.

#### Changes

- добавить integration-style test с fake provider, который блокируется до cancel
- проверить:
  - `/cancel` доходит
  - context отменяется
  - run завершается
- следующий запрос после cancel снова проходит

### Slice 5. Advisory Stop Policy

#### Goal

Не давать модели превращать уже достаточный advisory answer в бесконечный research loop.

#### Changes

- в Telegram runtime ввести lightweight policy для advisory prompts:
  - если запрос выглядит как `что посоветуешь`, `что лучше`, `как бы ты сделал`, `что думаешь`
  - и модель уже вернула содержательный answer draft
  - следующий tool round должен быть разрешён только если он реально закрывает явный пробел

- добавить heuristic/runtime guard:
  - если предыдущий assistant text уже содержит recommendation/advice
  - а новый tool call выглядит как “ещё одна общая проверка”
  - завершать run текущим draft, а не уходить в новый round

- добавить trace marker:
  - `advisory_stop_applied`
  - `advisory_extra_round_allowed`

#### Acceptance

- advisory questions не уходят в лишние дополнительные проверки без сильной причины
- если у модели уже есть годный ответ, пользователь получает его как финальный
- trace показывает, почему extra round был остановлен или разрешён

### Slice 6. Intent-Preserving Checkpoints

#### Goal

Не допускать ситуации, когда checkpoint помнит тему, но уже не хватает исходного user intent для осмысленного продолжения.

#### Changes

- при compaction/checkpointing сохранять краткий `originating_intent`
  - что именно пользователь просил сделать
  - не только `what happened`, но и `what was requested`

- при сборке prompt:
  - если в истории нет полного исходного user prompt для темы
  - но есть checkpoint
  - system summary должен содержать explicit `original user intent`

- если intent восстановить нельзя надёжно:
  - не делать вид, что тема продолжается
  - помечать run как needing clarification

#### Acceptance

- после длинных tool-heavy runs бот не отвечает туманно про “вижу checkpoint, но не вижу запрос”
- checkpoint даёт модели достаточно информации, чтобы продолжить тему осмысленно
- если информации всё же не хватает, это помечается как explicit clarification state, а не как случайный провал памяти

## Tests

Нужны обязательные тесты:

1. `Reply` does not block command handling for another update in same chat
2. `/cancel` interrupts blocked provider call
3. second normal message while run active gets deterministic rejection
4. provider timeout returns explicit failure
5. status card shows provider-wait state
6. advisory question with already-sufficient draft does not trigger unnecessary extra tool round
7. advisory question with explicit missing fact can still do one additional justified tool round
8. after compaction, checkpoint retains enough originating intent for a follow-up like `Ну?`

## Recommended Order

1. async poll/run split
2. single-chat active run gate
3. provider round timeout
4. stuck-state telemetry
5. cancel integration tests
6. advisory stop policy
7. intent-preserving checkpoints

## Success Criteria

План считается выполненным, когда:
- `/cancel` реально работает во время зависшего run
- `z.ai` hang не держит бота бесконечно
- пользователь всегда понимает, stuck ли бот и на чём именно
- один зависший run не ломает весь Telegram control flow
- advisory/recommendation runs не расползаются в лишний research после уже достаточного ответа
- checkpoint не переживает тему в “поломанном” виде, где summary есть, а исходный запрос уже потерян
