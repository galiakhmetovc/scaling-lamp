# Compaction

## Зачем она нужна

История растёт быстрее, чем влезает в prompt. Compaction сжимает старый префикс, но старается не потерять смысл.

## Текущая политика

Смотри:

- [internal/compaction/budget.go](/home/admin/AI-AGENT/data/projects/teamD/internal/compaction/budget.go)
- [internal/compaction/assembler.go](/home/admin/AI-AGENT/data/projects/teamD/internal/compaction/assembler.go)
- [internal/compaction/service.go](/home/admin/AI-AGENT/data/projects/teamD/internal/compaction/service.go)
- [internal/runtime/prompt_context.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/prompt_context.go)
- [internal/runtime/prompt_context_assembler.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/prompt_context_assembler.go)
- [internal/transport/telegram/prompt_context.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/prompt_context.go)
- [internal/transport/telegram/memory_runtime.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/memory_runtime.go)

## Какие env это регулируют

- `TEAMD_CONTEXT_WINDOW_TOKENS`
- `TEAMD_PROMPT_BUDGET_TOKENS`
- `TEAMD_COMPACTION_TRIGGER_TOKENS`
- `TEAMD_MAX_TOOL_CONTEXT_CHARS`
- `TEAMD_LLM_COMPACTION_ENABLED`
- `TEAMD_LLM_COMPACTION_TIMEOUT`

## Что происходит при compaction

1. `internal/runtime/prompt_context.go` оценивает projected final prompt, а не только сырую историю.
2. Перед сборкой prompt старый residency path проходит pruning.
3. Если trigger достигнут, старый префикс уходит в compaction service.
4. `service.go` пытается сделать:
   - `llm-v1` synthesis
   - если не удалось, fallback на `heuristic-v1`
5. Получается checkpoint.
6. При следующем prompt build этот checkpoint добавляется как summary.
7. После base assembly runtime-owned prompt context assembler добавляет workspace/SessionHead/recall/skills fragments.

Теперь checkpoint и continuity могут нести не только summary text, но и recoverable references:

- `archive_refs`
- `artifact_refs`

## Что защищено от потери

Самое важное правило: нельзя отрезать активный хвост текущего user turn.

Именно поэтому assembler старается сохранить целиком:

- последний `user`
- следующие `assistant/tool` сообщения этого хода

## Почему compaction может быть плохой

Если в transcript протек шумный tool output, summary становится мусорной.

Поэтому рядом с compaction есть фильтры и отдельный pruning layer:

- tool output reducer
- artifact offload
- pruning
- noisy content detection
- memory promotion gate

Если говорить жёстко:

- reducer уменьшает output
- pruning режет старое prompt residency без переписывания transcript
- offload выносит output из prompt path совсем

Для длинных задач это сильнее, чем просто тримминг.

## Как это выглядит снаружи

Compaction теперь не только внутренняя эвристика Telegram.

Для оператора следы видны через:

- `checkpoint` как часть следующего prompt path
- `memory` events в `teamd-agent chat`
- `artifact.offloaded` events в `events list/watch`
- `artifacts show/cat`, если длинный tool output был вынесен из transcript
- `runs replay`, если нужно понять, на какие archive/artifact refs опирался runtime

То есть практический путь длинной задачи выглядит так:

1. transcript растёт
2. compaction сворачивает старый префикс в checkpoint
3. trimmed prefix получает `archive_ref`
4. noisy outputs уходят в artifacts
5. recall и текущий active tail остаются короткими

## Как теперь читать код

- `internal/runtime/prompt_context.go` — решает, когда compact запускать и как собрать base prompt
- `internal/runtime/pruning.go` — снижает prompt residency старых noisy blocks без изменения durable transcript
- `internal/runtime/prompt_context_assembler.go` — добавляет runtime-owned prompt fragments после compaction
- `service.go` — строит checkpoint
- `internal/transport/telegram/prompt_context.go` — Telegram implementation for workspace/memory/skills fragments
- `memory_runtime.go` — решает, превращать ли checkpoint в searchable memory doc
- [artifact-offload.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/artifact-offload.md) — объясняет, как большие tool outputs выводятся из transcript path
- [context-budget.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/context-budget.md) — объясняет projected trigger и budget breakdown
