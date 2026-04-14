# Artifact Offload

`Artifact offload` в `teamD` нужен для одной простой вещи:

- большие tool outputs не должны раздувать prompt и transcript

Вместо этого runtime:

1. сохраняет полный payload как artifact
2. пишет в transcript короткий preview
3. добавляет `artifact_ref`
4. даёт оператору и агенту дочитать полный результат позже

Теперь это не только read-by-ref surface.

Artifacts также можно искать scoped-first:

- по `owner_type`
- по `owner_id`
- с явным global fallback только по запросу

## Почему это лучше blunt truncation

Если просто обрезать tool output:

- модель теряет детали
- compaction summary получает шумный мусор
- оператор не может понять, что именно видел runtime

Если offload сделан правильно:

- prompt остаётся компактным
- полный output не теряется
- event plane фиксирует сам факт offload через `artifact.offloaded`

## Где это происходит в коде

- [artifact_offload.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/artifact_offload.go)
- [conversation_engine.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/conversation_engine.go)
- [tool_helpers.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/tool_helpers.go)
- [store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/artifacts/store.go)

## Runtime path

Путь такой:

1. tool возвращает raw output
2. runtime вызывает offload policy
3. если output маленький:
   - он остаётся inline
4. если output большой:
   - payload уходит в artifact store
   - transcript получает краткий текст с `artifact_ref`
   - runtime пишет event `artifact.offloaded`
   - `RunView` и `WorkerView` получают `artifact_refs`

## Как это связано с compaction

Compaction не должна переваривать гигантские tool dumps.

Поэтому artifact offload улучшает compaction дважды:

- уменьшает шум в raw transcript
- даёт checkpoint ссылаться на результат через artifact/event path, а не через длинный text blob

## Как это связано с workers

Worker handoff использует ту же идею:

- родителю не нужен весь raw transcript worker'а
- родителю нужен summary + artifact refs

Поэтому `artifact offload` и `worker handoff` — это одна архитектурная линия:

- меньше inline шума
- больше recoverable structured state

## Операторские примеры

Посмотреть событие:

```bash
teamd-agent events list run run-1
```

Посмотреть metadata артефакта:

```bash
teamd-agent artifacts show artifact://tool-output-1
```

Дочитать payload:

```bash
teamd-agent artifacts cat artifact://tool-output-1
```

Поиск по owner scope:

```bash
teamd-agent artifacts search run run-1 error
```

Глобальный поиск только явно:

```bash
teamd-agent artifacts search --global error
```

Через HTTP API:

```bash
curl 'http://127.0.0.1:18081/api/events?entity_type=run&entity_id=run-1'
curl 'http://127.0.0.1:18081/api/artifacts/artifact:%2F%2Ftool-output-1'
curl 'http://127.0.0.1:18081/api/artifacts/artifact:%2F%2Ftool-output-1/content'
curl 'http://127.0.0.1:18081/api/artifacts/search?owner_type=run&owner_id=run-1&query=error'
```
