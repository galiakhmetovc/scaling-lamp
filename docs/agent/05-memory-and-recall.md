# Memory And Recall

## У памяти тут три уровня

### 1. Session history

Это история сообщений Telegram-сессии.

Она нужна для:

- прямого продолжения разговора
- compaction input
- аудита

### 2. Working state

Это краткое состояние текущей сессии.

В этот слой входят:

- `SessionHead`
- `checkpoint`
- `continuity`

Их удобнее понимать не как две отдельные памяти, а как две формы рабочего состояния.

`SessionHead` — это канонический recent-context слой между runs.

Там живут:

- last completed run
- current goal
- last result summary
- recent artifacts

Это не long-term memory и не searchable memory.  
Это рабочая "голова" текущей сессии.

`Checkpoint` — это сжатый снимок старой части сессии.

Он нужен, чтобы:

- не тащить всю старую историю в prompt
- сохранить смысл старого префикса

`Continuity` — более устойчивое представление о том, что сейчас происходит в сессии.

Там обычно живут:

- user goal
- current state
- resolved facts
- unresolved items

### 3. Searchable memory

Это отдельные документы для recall.

Они уже идут в поиск и могут переживать конкретный run.

## Где это реализовано

- [internal/memory/postgres_store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/memory/postgres_store.go)
- [internal/memory/recall.go](/home/admin/AI-AGENT/data/projects/teamD/internal/memory/recall.go)
- [internal/memory/store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/memory/store.go)
- [internal/runtime/memory_documents.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/memory_documents.go)
- [internal/runtime/prompt_context_assembler.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/prompt_context_assembler.go)
- [internal/runtime/postgres_store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/postgres_store.go)
- [internal/transport/telegram/memory_runtime.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/memory_runtime.go)
- [internal/transport/telegram/prompt_context.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/prompt_context.go)

## Как recall попадает в модель

Есть два пути:

### Automatic pre-recall

Runtime сам ищет релевантную память перед prompt build и подмешивает её как system block.

Структура теперь такая:

- `internal/runtime/prompt_context_assembler.go`
  - даёт runtime-owned hook для recall injection
- `internal/transport/telegram/prompt_context.go`
  - реализует Telegram-side recall lookup и formatting inputs
- `internal/memory/recall.go`
  - форматирует найденные recall items в prompt block

### Tool-based recall

Модель может сама вызвать:

- `memory_search`
- `memory_read`

Это полезно, когда ей нужно точечно поднять старый контекст.

## Semantic search

Если включены embeddings:

- документ получает embedding через Ollama
- embedding хранится в Postgres `pgvector`
- поиск идёт сначала по vector similarity
- потом fallback на FTS/ILIKE

Ключевые настройки:

- `TEAMD_MEMORY_EMBEDDINGS_ENABLED`
- `TEAMD_OLLAMA_BASE_URL`
- `TEAMD_MEMORY_EMBED_MODEL`
- `TEAMD_MEMORY_EMBED_DIMS`

## Что нельзя писать в память

Нельзя продвигать туда:

- шумный tool output
- web-search snippets
- binary-derived strings
- transient machine dumps

Именно поэтому в коде есть noisy-content filters и quality gate перед `checkpoint -> memory_document`.

Эта логика теперь живёт отдельно:

- `internal/runtime/memory_documents.go`
  - строит continuity/checkpoint memory docs
  - режет noisy summaries
  - выделяет resolved facts

## Явная memory policy

Теперь policy задаётся явно через конфиг, а не только через скрытые эвристики.

Главные настройки:

- `TEAMD_MEMORY_POLICY_PROFILE`
- `TEAMD_MEMORY_PROMOTE_CHECKPOINT`
- `TEAMD_MEMORY_PROMOTE_CONTINUITY`
- `TEAMD_MEMORY_RECALL_KINDS`
- `TEAMD_MEMORY_MAX_BODY_CHARS`
- `TEAMD_MEMORY_MAX_RESOLVED_FACTS`

Практический смысл:

- можно отдельно включать и выключать promotion для `checkpoint` и `continuity`
- automatic recall можно ограничить whitelist kinds
- можно ограничить размер searchable memory docs
- можно ограничить число facts, которые continuity вытаскивает в память

Консервативный дефолт сейчас такой:

- `checkpoint` не promoted в searchable memory
- `continuity` promoted
- automatic recall preload'ит только `continuity`
- body memory docs режется по лимиту

## Самая полезная mental model

Если совсем коротко:

1. агент хранит сырую историю сессии
2. runtime делает из неё working state
3. `SessionHead` держит recent truth между runs
4. только малая часть working state продвигается в searchable memory

То есть память здесь не равна transcript.

## Как это видно оператору

Память теперь можно наблюдать не только через код и DB.

Через control plane доступны:

- `teamd-agent memory search <chat_id> <session_id> <query>`
- `teamd-agent memory read <doc_key>`
- `teamd-agent chat ...`
  - показывает `memory:` events, например `artifact offloaded`
- `GET /api/memory/search`
- `GET /api/memory/{key}`

Это важно, потому что длинные tool outputs теперь часто не остаются inline в transcript, а уходят в artifacts. Для оператора это выглядит как связка:

1. run/tool loop
2. artifact offload
3. searchable memory and recall
4. on-demand artifact read

## Как теперь читать код

- `memory_runtime.go` — persistence в run store и запись memory docs
- `internal/runtime/memory_documents.go` — policy: что именно promoted в searchable memory
- `internal/memory/recall.go` — как recall превращается в prompt block
- `internal/runtime/prompt_context_assembler.go` — runtime-owned recall/skills/workspace injection point
- `internal/transport/telegram/prompt_context.go` — Telegram implementation of recall/workspace fragments
- [context-budget.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/context-budget.md) — как recall конкурирует за prompt residency
