# teamD vs Hermes/OpenClaw

Этот документ фиксирует разницу между текущим `teamD` single-agent core и более зрелыми агентными системами вроде `Hermes` и `OpenClaw`.

Цель не в том, чтобы “догнать всё подряд”, а в том, чтобы ясно видеть:

- что у `teamD` уже есть
- чего ещё нет
- что реально стоит делать следующим

## Сводная таблица

| Область | teamD сейчас | У Hermes / OpenClaw сильнее | Вывод |
|---|---|---|---|
| Single-agent runtime | Есть рабочее ядро, cancel, status, traces, tool loop | Обычно ещё строже отделён core API от transport | У нас уже хорошо, но transport independence ещё не доведена |
| Transport separation | Telegram сильно похудел, runtime стал центральнее | У зрелых систем transport ещё тоньше | Нужен ещё более явный core API |
| Run lifecycle control | Есть `RunManager`, `ActiveRegistry`, persistent run records | Часто богаче: pause/resume/replay/watchdogs | Следующий шаг уже не “фикс”, а richer control plane |
| Memory | Есть session history, checkpoint, continuity, searchable memory, embeddings | Обычно ещё лучше разведены working/durable/user memory | У нас memory уже рабочая, но модель ещё проще, чем у зрелых систем |
| Recall | Есть automatic recall и tool-based recall | Чаще есть более богатый retrieval policy и stronger separation of recall classes | Нужно развить recall policy, а не просто добавлять ещё поиск |
| Compaction | Есть trigger, checkpoint synthesis, prompt assembly, LLM fallback | У зрелых систем часто богаче lineage/replay semantics | Нормально для текущего этапа, но не конечное состояние |
| Tool loop | Есть provider tools, memory tools, guards, status surfacing | Часто есть единый policy/approval слой поверх всех tools | Главный gap здесь — policy, не сам tool loop |
| Approval / safety | Частично есть подсистема approvals, но не центральна в single-agent path | У зрелых систем approval часто часть основного runtime FSM | Это один из самых заметных архитектурных gaps |
| Replay / debugging | Есть хорошие traces | У зрелых систем бывает step-by-step replay и richer state inspection | Следующий шаг после traces — replay/debug mode |
| Examples / onboarding | Есть docs и minimal-agent skeleton | У зрелых систем обычно больше reference agents/templates | Нам не хватает 2-3 опорных reference examples |

## Что у нас уже есть

Это важно зафиксировать отдельно, чтобы не занижать реальное состояние проекта.

- Понятный single-agent path без mesh в hot path
- Рабочий `RunManager`
- Асинхронный Telegram runtime
- `/cancel`, `/status`, LLM traces
- Tool loop с guardrails
- Compaction
- Memory recall
- Semantic retrieval через embeddings
- Launcher через `systemd --user`
- Beginner docs и minimal skeleton

То есть `teamD` уже не выглядит как экспериментальный прототип.

## Чего у нас нет

### 1. Полноценного core API, независимого от Telegram

Сейчас ядро уже сильно лучше, но transport всё ещё остаётся главным клиентом и главным mental model.

У более зрелой системы хочется видеть интерфейс уровня:

- `StartRun(...)`
- `CancelRun(...)`
- `GetRun(...)`
- `ContinueRun(...)`
- `ReplayRun(...)`

И чтобы Telegram был просто одним из клиентов этого API.

### 2. Единого policy / approval слоя

Сейчас policy есть кусками:

- memory policy
- tool guards
- runtime defaults

Но нет одного понятного policy-layer, который отвечает на вопросы:

- можно ли этот tool вообще
- нужен ли approval
- можно ли сеть
- можно ли писать в память
- что является risky action

### 3. Более богатой memory hierarchy

Сейчас у нас уже есть хорошая основа:

- session history
- working state
- searchable memory

Но у зрелых систем обычно отдельно существуют:

- user profile memory
- workspace durable facts
- operator memory
- retrieval-only memory

У нас это пока более компактная модель.

### 4. Replay/debug mode поверх traces

Traces уже полезные.

Но следующий зрелый шаг — возможность:

- воспроизвести старый run
- посмотреть state transitions
- понять, где runtime или provider ушёл не туда

### 5. Больше reference examples

Один `minimal-agent` уже полезен.

Но для настоящего onboarding не хватает ещё хотя бы:

- `minimal-tool-agent`
- `minimal-memory-agent`
- `minimal-transport-adapter`

## Что стоит брать у Hermes

У Hermes сильная сторона — runtime discipline.

Что полезно перенять:

- более строгий control plane
- richer run lifecycle
- stronger approval/interrupt model
- более богатую session/memory model

Короче:

- у Hermes стоит брать **runtime rigor**

## Что стоит брать у OpenClaw

У OpenClaw сильная сторона — memory/retrieval и separation of concerns around multi-layer context.

Что полезно перенять:

- лучшее разделение memory classes
- retrieval-first мышление
- более transport-agnostic core shape

Короче:

- у OpenClaw стоит брать **memory/retrieval architecture**

## Что не надо делать прямо сейчас

- Не надо снова расползаться в mesh раньше времени
- Не надо строить огромную agent platform сразу
- Не надо добавлять новые сложные subsystems, пока не добит policy/core API

Следующий правильный рост должен быть не в ширину, а в глубину.

## Что делать следующим

### P1

1. Сделать единый `core runtime API`
2. Сделать явный `policy/approval layer`
3. Добавить session-scoped overrides для runtime/memory policy

### P2

1. Добавить replay/debug mode поверх traces
2. Расширить memory hierarchy
3. Добавить ещё 2-3 reference agent examples

### P3

1. Возвращаться к mesh
2. Делать richer orchestration between agents

## Рекомендация

Если выбирать один следующий большой этап, я бы выбрал:

**`policy + approval + core runtime API`**

Причина простая:

- это даст больше инженерной зрелости, чем следующий memory tweak
- это подготовит почву и для mesh, и для risky tools, и для richer operator UX
- это тот слой, который у более зрелых систем уже обычно есть

Если коротко:

- single-agent core у нас уже хороший
- следующий реальный скачок — не “ещё один рефакторинг”, а **runtime governance layer**
