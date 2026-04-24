# Structured tools и approvals

## Почему tools здесь важны

Для `teamD` tools — это не “дополнительная фича модели”, а основной способ делать управляемые side effects.

Вместо того чтобы позволять модели писать shell-магией что угодно, runtime даёт ей строгое меню capabilities:

- файлы;
- процессы;
- планы;
- память;
- артефакты;
- MCP;
- межагентные действия.

Определения живут в [`crates/agent-runtime/src/tool.rs`](../../crates/agent-runtime/src/tool.rs).

## Семейства tool surface

### Filesystem

Основные инструменты:

- `fs_read_text`
- `fs_read_lines`
- `fs_search_text`
- `fs_find_in_files`
- `fs_write_text`
- `fs_patch_text`
- `fs_replace_lines`
- `fs_insert_text`
- `fs_mkdir`
- `fs_move`
- `fs_trash`
- `fs_list`
- `fs_glob`

Есть и legacy-имена (`fs_read`, `fs_write`, `fs_patch`, `fs_search`), но канонический surface ориентирован на более точные typed variants.

### Exec

- `exec_start`
- `exec_read_output`
- `exec_wait`
- `exec_kill`

Главный принцип: structured executable + args. Не полагаться на shell snippets как базовый инструмент.

### Planning

- `init_plan`
- `add_task`
- `set_task_status`
- `add_task_note`
- `edit_task`
- `plan_snapshot`
- `plan_lint`

План должен быть каноническим источником прогресса, а не память модели.

### Offload и память

- `artifact_read`
- `artifact_search`
- `session_search`
- `session_read`
- `session_wait`
- `knowledge_search`
- `knowledge_read`

### MCP

- `mcp_call`
- `mcp_search_resources`
- `mcp_read_resource`
- `mcp_search_prompts`
- `mcp_get_prompt`

### Agent

- `agent_list`
- `agent_read`
- `agent_create`
- `schedule_list`
- `schedule_read`
- `schedule_create`
- `schedule_update`
- `schedule_delete`
- `message_agent`
- `grant_agent_chain_continuation`

## Tool policy

Каждый tool definition содержит policy:

- `read_only`
- `destructive`
- `requires_approval`

Идея простая: модель должна заранее знать, какой инструмент безопасный, а какой может остановить ход до operator approval.

## Жизненный цикл tool call

В transcript и event stream tool step проходит статусы:

- `requested`
- `waiting_approval`
- `approved`
- `running`
- `completed`
- `failed`

Это важно для TUI и REPL: интерфейс показывает не “сырой шум”, а компактный статус каждого tool step.

Кроме event stream, runtime пишет persistent tool-call ledger в таблицу `tool_calls`. Там фиксируется сам факт вызова: `session_id`, `run_id`, provider call id, tool name, arguments JSON, summary, status, error и timestamps. Полный большой результат tool’а туда не кладётся; он остаётся в transcript/model continuation или уходит в artifacts/offloads.

Операторская команда:

```bash
agentd session tools <session_id> --limit 50 --offset 0
```

По умолчанию команда печатает человекочитаемый отчёт: группирует вызовы по `run`, нумерует tool calls, отдельно показывает ISO-время вызова, `summary`, pretty-printed `args`, `status` и `error`. Команда постраничная: заголовок показывает `total`, текущий диапазон `showing` и `next_offset` для следующей страницы.

Если нужен старый однострочный формат для `grep`, diff или внешнего парсинга, используйте:

```bash
agentd session tools <session_id> --raw --limit 50 --offset 0
```

Эта команда нужна для аудита и для улучшения инструкций агентам: можно увидеть, какие tools модель выбирала, какие аргументы передавала и где ошибалась.

## Approval model

Approval — это не отдельная мини-сессия. Это состояние того же run.

Когда tool требует operator confirmation:

1. runtime создаёт approval record;
2. run сохраняет pending approval state;
3. provider loop останавливается;
4. оператор подтверждает;
5. тот же run продолжается дальше.

Следствие: approval continuation должна чиниться в runtime/persistence/provider loop, а не в UI.

## `message_agent` и `session_wait`

Это самая частая точка путаницы.

### `message_agent`

`message_agent`:

- **асинхронный**;
- создаёт дочернюю session;
- ставит inter-agent job в очередь;
- возвращает `recipient_session_id`, `recipient_job_id`, `chain_id`, `hop_count`;
- **не означает**, что другой агент уже ответил.

### `session_read`

`session_read`:

- читает bounded snapshot session;
- не ждёт завершения active jobs;
- нужен для пассивного просмотра.

### `session_wait`

`session_wait`:

- явный follow-up после `message_agent`;
- ждёт, пока child session “устаканится”;
- при ожидании может pump’ить background worker для этой session;
- возвращает bounded snapshot и флаг `settled`.

Это правильный способ избежать фантазий модели вроде “судья молчит”, когда дочерняя session ещё просто не успела завершиться.

## `grant_agent_chain_continuation`

У межагентных цепочек есть hop limit. Если цепочка упёрлась в `max_hops`, обычный следующий hop блокируется.

`grant_agent_chain_continuation` нужен только для явного разового разрешения ещё одного hop после подтверждённого block.

Неправильный usage:

- выдавать grant “на всякий случай”;
- выдавать grant до того, как цепочка реально упёрлась.

## Offload и artifacts

Если tool output большой:

- runtime пишет payload в artifact;
- в model-facing output оставляет summary/reference;
- оператор потом читает это через `artifact_read` или browser в TUI.

Это защищает prompt от silent bloat.

## Частые ошибки агентов

Система уже пытается объяснить это в tool definitions и agent prompts, но типовые промахи такие:

- считать `message_agent` синхронным;
- использовать `session_read` там, где нужен `session_wait`;
- пытаться обойти structured exec shell-трюками;
- завершать ход без реальной проверки результата;
- просить `grant_agent_chain_continuation`, не проверив max-hops block;
- тащить большие outputs обратно в prompt вместо artifacts.

## Где смотреть

- Tool catalog и schemas: [`crates/agent-runtime/src/tool.rs`](../../crates/agent-runtime/src/tool.rs)
- Tool tests: [`crates/agent-runtime/src/tool/tests.rs`](../../crates/agent-runtime/src/tool/tests.rs)
- Provider loop integration: [`cmd/agentd/src/execution/provider_loop.rs`](../../cmd/agentd/src/execution/provider_loop.rs)
- Inter-agent semantics: [`cmd/agentd/src/execution/interagent.rs`](../../cmd/agentd/src/execution/interagent.rs)
- Operator help: [`cmd/agentd/src/help.rs`](../../cmd/agentd/src/help.rs)
- Agent prompt guidance: [`cmd/agentd/src/agents.rs`](../../cmd/agentd/src/agents.rs)
