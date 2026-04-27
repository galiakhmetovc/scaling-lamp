# Справочник по tools

Этот документ отвечает на три практических вопроса:

1. как tools выглядят для модели в prompt contract;
2. какая у каждого tool сигнатура;
3. что именно tool делает и когда его использовать.

Источник истины:

- каталог tools: [`crates/agent-runtime/src/tool.rs`](../../crates/agent-runtime/src/tool.rs)
- provider-facing wiring: [`cmd/agentd/src/execution/provider_loop.rs`](../../cmd/agentd/src/execution/provider_loop.rs)

## Как tools выглядят в prompt

### Важный нюанс

Tools не попадают в prompt как большой текстовый README-блок вида “вот список команд”.

Они передаются provider’у структурированно как `ProviderToolDefinition`:

- `name`
- `description`
- `parameters`

Это делает [`automatic_provider_tools`](../../cmd/agentd/src/execution/provider_loop.rs) в `provider_loop`.

Логически модель видит tool примерно так:

```json
{
  "name": "fs_read_text",
  "description": "Read a UTF-8 text file from the workspace",
  "parameters": {
    "type": "object",
    "properties": {
      "path": {
        "type": "string",
        "description": "Relative path to the file"
      }
    },
    "required": ["path"],
    "additionalProperties": false
  }
}
```

### Что именно попадает модели

Важны три уровня:

1. `ToolCatalog::all_definitions()`
   Это полный каталог definitions, включая compatibility и внутренние ids.

2. `ToolCatalog::automatic_model_definitions()`
   Это канонический model-facing surface, который runtime вообще готов отдавать модели.

3. `automatic_provider_tools(...)`
   Это итоговый список для конкретного turn:
   - только если provider поддерживает tool calls;
   - только tools, разрешённые `AgentProfile.allowed_tools`;
   - `artifact_read` и `artifact_search` добавляются только если у session реально есть context offload;
   - dynamic MCP tools добавляются отдельно как самостоятельные provider tools с собственным `exposed_name` и `input_schema`.

### Что не стоит путать

- `mcp_call` определён в каталоге, но обычно не показывается модели как обычный tool id.
  Вместо него модель видит уже обнаруженные MCP tools по их `exposed_name`.
- legacy ids вроде `fs_read`, `fs_write`, `fs_patch`, `fs_search` живут в каталоге, но не входят в канонический automatic model-facing surface.
- `plan_read` и `plan_write` определены, но для обычного model-driven loop вместо них используются более узкие typed planning tools.

## Канонический model-facing tool surface

Ниже перечислены именно те ids, которые входят в `automatic_model_definitions()`.

Формат сигнатуры:

- это не Rust-тип и не verbatim JSON schema;
- это сжатая human-readable запись того же `parameters` schema;
- `?` означает optional поле;
- значения enum перечислены явно;
- если в description у schema есть важный default, он указан в комментарии.

## Filesystem

### `fs_read_text`

Сигнатура:

```text
fs_read_text({ path: string })
```

Что делает:

- читает UTF-8 текстовый файл целиком;
- возвращает текст и metadata о чтении.

Когда использовать:

- если нужен полный файл целиком;
- если не нужен точечный диапазон строк.

### `fs_read_lines`

Сигнатура:

```text
fs_read_lines({ path: string, start_line: integer, end_line: integer })
```

Что делает:

- читает включительно диапазон строк;
- возвращает bounds файла и сам диапазон.

Когда использовать:

- если нужен только кусок файла;
- если важно не тянуть весь большой файл в модель.

### `fs_search_text`

Сигнатура:

```text
fs_search_text({ path: string, query: string })
```

Что делает:

- ищет literal text внутри одного UTF-8 файла.

Когда использовать:

- если файл уже известен;
- если нужен поиск только внутри него.

### `fs_find_in_files`

Сигнатура:

```text
fs_find_in_files({ query: string, glob?: string, limit?: integer })
```

Что делает:

- ищет literal text по workspace;
- может ограничивать поиск glob-шаблоном.

Когда использовать:

- если сначала нужно найти файл по содержимому;
- если точный путь ещё неизвестен.

### `fs_list`

Сигнатура:

```text
fs_list({ path: string, recursive: boolean, limit?: integer, offset?: integer })
```

Что делает:

- листит файлы и директории;
- поддерживает пагинацию.

Когда использовать:

- для обзора каталога;
- перед чтением или поиском.

### `fs_glob`

Сигнатура:

```text
fs_glob({ path: string, pattern: string, limit?: integer, offset?: integer })
```

Что делает:

- матчинг путей по glob-шаблону.

Когда использовать:

- если нужны `*.rs`, `**/Cargo.toml`, `docs/**/*.md` и подобные выборки.

### `fs_write_text`

Сигнатура:

```text
fs_write_text({
  path: string,
  content: string,
  mode: "create" | "overwrite" | "upsert"
})
```

Что делает:

- пишет полный текст файла;
- semantics записи задаются явно через `mode`.

Когда использовать:

- для создания нового файла;
- для полной перезаписи файла;
- когда intent именно “заменить весь файл”.

### `fs_patch_text`

Сигнатура:

```text
fs_patch_text({ path: string, search: string, replace: string })
```

Что делает:

- заменяет один точный текстовый фрагмент в UTF-8 файле.

Когда использовать:

- для маленьких точечных правок;
- когда известен exact fragment.

### `fs_replace_lines`

Сигнатура:

```text
fs_replace_lines({
  path: string,
  start_line: integer,
  end_line: integer,
  content: string
})
```

Что делает:

- заменяет явный диапазон строк.

Когда использовать:

- когда известны номера строк;
- когда search/replace слишком хрупок.

### `fs_insert_text`

Сигнатура:

```text
fs_insert_text({
  path: string,
  line?: integer,
  position: "before" | "after" | "prepend" | "append",
  content: string
})
```

Что делает:

- вставляет текст до/после строки или в начало/конец файла.

Когда использовать:

- для controlled insertions без полной замены блока.

### `fs_mkdir`

Сигнатура:

```text
fs_mkdir({ path: string })
```

Что делает:

- создаёт директорию внутри workspace.

### `fs_move`

Сигнатура:

```text
fs_move({ src: string, dest: string })
```

Что делает:

- перемещает или переименовывает файл/директорию.

### `fs_trash`

Сигнатура:

```text
fs_trash({ path: string })
```

Что делает:

- перемещает файл/директорию в workspace trash вместо перманентного удаления.

Когда использовать:

- когда нужно убрать файл безопаснее, чем через hard delete.

## Web

### `web_fetch`

Сигнатура:

```text
web_fetch({ url: string })
```

Что делает:

- делает прямой HTTP fetch указанного URL;
- для `text/html`/`xhtml` конвертирует страницу в markdown-подобный readable text;
- большие результаты могут уходить в artifact/offload path.

Когда использовать:

- если уже есть конкретный URL;
- если нужен текст страницы, а не search results.

### `web_search`

Сигнатура:

```text
web_search({ query: string, limit: integer })
```

Что делает:

- выполняет запрос к configured search backend;
- backend задаётся через `[web]` config.

Когда использовать:

- как первый шаг веб-исследования;
- чтобы найти candidate URLs перед `web_fetch`.

## Exec

### `exec_start`

Сигнатура:

```text
exec_start({
  executable: string,
  args: string[],
  cwd?: string | null
})
```

Что делает:

- запускает structured process `executable + args`;
- не опирается на shell snippets как на основной API;
- возвращает `process_id`.

Когда использовать:

- для запуска тестов, сборки, git, rg, ls и прочих CLI;
- если нужен дальнейший контроль процесса.

### `exec_read_output`

Сигнатура:

```text
exec_read_output({
  process_id: string,
  stream?: "merged" | "stdout" | "stderr" | null,
  cursor?: integer | null,
  max_bytes?: integer | null,
  max_lines?: integer | null
})
```

Что делает:

- читает bounded live output уже запущенного процесса;
- поддерживает cursor-based дочитывание.

Когда использовать:

- если процесс долгий;
- если нужен incremental polling вывода.

### `exec_wait`

Сигнатура:

```text
exec_wait({ process_id: string })
```

Что делает:

- ждёт завершения процесса;
- возвращает итоговый статус и bounded output summary.

Когда использовать:

- когда процесс уже запущен и нужно дождаться результата.

### `exec_kill`

Сигнатура:

```text
exec_kill({ process_id: string })
```

Что делает:

- завершает запущенный process.

## Planning

### `init_plan`

Сигнатура:

```text
init_plan({ goal: string })
```

Что делает:

- создаёт верхнеуровневую цель structured plan.

### `add_task`

Сигнатура:

```text
add_task({
  description: string,
  depends_on?: string[],
  parent_task_id?: string | null
})
```

Что делает:

- добавляет задачу в план;
- может сразу связать зависимости и parent.

### `set_task_status`

Сигнатура:

```text
set_task_status({
  task_id: string,
  new_status: "pending" | "in_progress" | "completed" | "blocked" | "cancelled",
  blocked_reason?: string | null
})
```

Что делает:

- меняет статус задачи;
- для `blocked` может зафиксировать причину.

### `add_task_note`

Сигнатура:

```text
add_task_note({ task_id: string, note: string })
```

Что делает:

- добавляет заметку к задаче.

### `edit_task`

Сигнатура:

```text
edit_task({
  task_id: string,
  description?: string | null,
  depends_on?: string[] | null,
  parent_task_id?: string | null,
  clear_parent_task?: boolean
})
```

Что делает:

- редактирует описание, зависимости и parent relation существующей задачи.

### `plan_snapshot`

Сигнатура:

```text
plan_snapshot({})
```

Что делает:

- возвращает текущее состояние structured plan: goal, tasks, statuses, notes, dependencies.

### `plan_lint`

Сигнатура:

```text
plan_lint({})
```

Что делает:

- валидирует план;
- возвращает найденные structural issues.

### `prompt_budget_read`

Сигнатура:

```text
prompt_budget_read({})
```

Что делает:

- читает текущую session-scoped prompt budget policy;
- возвращает `context_window_tokens`, `auto_compaction_trigger_basis_points`, `usable_context_tokens`;
- возвращает проценты и target tokens для слоёв `SYSTEM`, `AGENTS`, active skills, `SessionHead`, `AutonomyState`, plan, summary, offload refs, recent tool activity и transcript tail.

Когда использовать:

- если нужно понять, сколько usable context доступно;
- перед изменением prompt allocation через `prompt_budget_update`.

### `prompt_budget_update`

Сигнатура:

```text
prompt_budget_update({
  reset?: boolean,
  percentages?: {
    system?: integer | null,
    agents?: integer | null,
    active_skills?: integer | null,
    session_head?: integer | null,
    autonomy_state?: integer | null,
    plan?: integer | null,
    context_summary?: integer | null,
    offload_refs?: integer | null,
    recent_tool_activity?: integer | null,
    transcript_tail?: integer | null
  } | null,
  reason?: string | null
})
```

Что делает:

- меняет session-scoped prompt budget policy;
- если `reset=true`, сначала возвращает policy к default;
- затем merge-ит переданные проценты;
- отклоняет изменение, если итоговая сумма процентов не равна `100`;
- влияет на физическое ограничение prompt layers при следующих provider requests.

Когда использовать:

- когда текущей задаче реально нужен другой context allocation;
- например временно увеличить `transcript_tail` или `recent_tool_activity`, чтобы модель лучше видела свежую историю или недавние tool ошибки.

Важно:

- если layer превышает target, prompt содержит `Prompt Budget Truncation` notice с количеством скрытых approximate tokens/messages;
- `transcript_tail` сохраняет новые сообщения, а старые uncovered messages скрываются первыми;
- скрытое содержимое не теряется: его можно читать через transcript/debug/session/artifact surfaces.

## Offload

### `artifact_read`

Сигнатура:

```text
artifact_read({ artifact_id: string })
```

Что делает:

- читает полный payload offloaded artifact по `artifact_id`.

Когда использовать:

- если transcript/tool result дал только artifact reference;
- если нужен большой output целиком.

### `artifact_search`

Сигнатура:

```text
artifact_search({ query: string, limit: integer })
```

Что делает:

- ищет по labels, summaries и payloads текущих offloaded artifacts.

## Memory

### `knowledge_search`

Сигнатура:

```text
knowledge_search({
  query: string,
  limit?: integer | null,
  offset?: integer | null,
  kinds?: ("root_doc" | "project_doc" | "project_note" | "extra_doc")[] | null,
  roots?: ("root_docs" | "docs" | "projects" | "notes" | "extra")[] | null
})
```

Что делает:

- ищет по canonical knowledge roots проекта;
- возвращает source metadata и bounded results.

### `knowledge_read`

Сигнатура:

```text
knowledge_read({
  path: string,
  mode?: "excerpt" | "full" | null,
  cursor?: integer | null,
  max_bytes?: integer | null,
  max_lines?: integer | null
})
```

Что делает:

- читает один knowledge source в bounded excerpt/full режиме.

Важно:

- enum-like аргументы должны быть quoted JSON strings;
- то есть `"mode": "full"`, а не `"mode": full`.

### `session_search`

Сигнатура:

```text
session_search({
  query: string,
  limit?: integer | null,
  offset?: integer | null,
  tiers?: ("active" | "warm" | "cold")[] | null,
  agent_identifier?: string | null,
  updated_after?: integer | null,
  updated_before?: integer | null
})
```

Что делает:

- ищет по историческим sessions;
- удобен, чтобы сначала найти точный `session_id`.

### `session_read`

Сигнатура:

```text
session_read({
  session_id: string,
  mode?: "summary" | "timeline" | "transcript" | "artifacts" | null,
  cursor?: integer | null,
  max_items?: integer | null,
  max_bytes?: integer | null,
  include_tools?: boolean | null
})
```

Что делает:

- читает bounded snapshot session;
- не ждёт новой работы;
- нужен для пассивного просмотра текущего состояния session.

### `session_wait`

Сигнатура:

```text
session_wait({
  session_id: string,
  wait_timeout_ms?: integer | null,
  mode?: "summary" | "timeline" | "transcript" | "artifacts" | null,
  cursor?: integer | null,
  max_items?: integer | null,
  max_bytes?: integer | null,
  include_tools?: boolean | null
})
```

Что делает:

- ждёт, пока queued/running work в session устаканится;
- потом возвращает bounded snapshot.

Когда использовать:

- после `message_agent`, если нужен реальный ответ другого агента до завершения текущего хода.

## MCP

### `mcp_search_resources`

Сигнатура:

```text
mcp_search_resources({
  connector_id?: string | null,
  query?: string | null,
  limit?: integer | null,
  offset?: integer | null
})
```

Что делает:

- ищет по обнаруженным MCP resources.

### `mcp_read_resource`

Сигнатура:

```text
mcp_read_resource({ connector_id: string, uri: string })
```

Что делает:

- читает один MCP resource.

### `mcp_search_prompts`

Сигнатура:

```text
mcp_search_prompts({
  connector_id?: string | null,
  query?: string | null,
  limit?: integer | null,
  offset?: integer | null
})
```

Что делает:

- ищет по обнаруженным MCP prompts.

### `mcp_get_prompt`

Сигнатура:

```text
mcp_get_prompt({
  connector_id: string,
  name: string,
  arguments?: object | null
})
```

Что делает:

- получает один MCP prompt по имени.

### Отдельно про dynamic MCP tools

Модель обычно видит не `mcp_call`, а уже конкретные discovered MCP tools:

- их `name` берётся из `tool.exposed_name`;
- `description` берётся из description MCP tool;
- `parameters` берётся из `tool.input_schema`.

То есть provider получает их как обычные полноценные tools, а runtime потом маршрутизирует вызов во внутренний `mcp_call`.

## Agent

### `agent_list`

Сигнатура:

```text
agent_list({ limit?: integer | null, offset?: integer | null })
```

Что делает:

- листит доступные agent profiles.

### `agent_read`

Сигнатура:

```text
agent_read({ identifier: string })
```

Что делает:

- читает agent profile по id или имени.

### `agent_create`

Сигнатура:

```text
agent_create({
  name: string,
  template_identifier?: string | null
})
```

Что делает:

- создаёт новый agent profile из встроенного или существующего template.

### `continue_later`

Сигнатура:

```text
continue_later({
  delay_seconds: integer,
  handoff_payload: string,
  delivery_mode?: "fresh_session" | "existing_session" | null
})
```

Что делает:

- создаёт self-addressed one-shot timer;
- будит ту же или новую session позже.

Когда использовать:

- для простого “напомни через N минут/часов”.

Важно:

- для простого one-shot reminder это preferred tool;
- enum-like `delivery_mode` должен быть quoted JSON string.

### `schedule_list`

Сигнатура:

```text
schedule_list({
  limit?: integer | null,
  offset?: integer | null,
  agent_identifier?: string | null
})
```

Что делает:

- показывает schedules текущего workspace.

### `schedule_read`

Сигнатура:

```text
schedule_read({ id: string })
```

Что делает:

- читает один schedule по id.

### `schedule_create`

Сигнатура:

```text
schedule_create({
  id: string,
  agent_identifier?: string | null,
  prompt: string,
  mode?: "interval" | "after_completion" | "once" | null,
  delivery_mode?: "fresh_session" | "existing_session" | null,
  target_session_id?: string | null,
  interval_seconds: integer,
  enabled?: boolean | null
})
```

Что делает:

- создаёт advanced или recurring schedule.

Когда использовать:

- для интервалов, recurring запуска, delivery в другую session;
- не для простого one-shot reminder, если хватает `continue_later`.

Важно:

- `mode` и `delivery_mode` должны быть quoted JSON strings.

### `schedule_update`

Сигнатура:

```text
schedule_update({
  id: string,
  agent_identifier?: string | null,
  prompt?: string | null,
  mode?: "interval" | "after_completion" | "once" | null,
  delivery_mode?: "fresh_session" | "existing_session" | null,
  target_session_id?: string | null,
  interval_seconds?: integer | null,
  enabled?: boolean | null
})
```

Что делает:

- обновляет существующий schedule.

Важно:

- enum-like поля тоже должны быть quoted JSON strings.

### `schedule_delete`

Сигнатура:

```text
schedule_delete({ id: string })
```

Что делает:

- удаляет schedule.

### `message_agent`

Сигнатура:

```text
message_agent({
  target_agent_id: string,
  message: string
})
```

Что делает:

- ставит асинхронное сообщение другому агенту;
- создаёт fresh recipient session и background job;
- не ждёт ответ автоматически.

Когда использовать:

- если нужно поручить работу другому агенту.

Важно:

- если нужен ответ до завершения текущего хода, затем используйте `session_wait` по возвращённому `recipient_session_id`.

### `grant_agent_chain_continuation`

Сигнатура:

```text
grant_agent_chain_continuation({
  chain_id: string,
  reason: string
})
```

Что делает:

- разрешает ровно один дополнительный hop для уже заблокированной inter-agent chain.

Когда использовать:

- только после подтверждённого `max_hops` block.

## Tools, которые определены, но обычно не показываются модели напрямую

### Legacy filesystem ids

- `fs_read`
- `fs_write`
- `fs_patch`
- `fs_search`

Это compatibility layer. Канонический model-facing surface использует typed variants:

- `fs_read_text`
- `fs_read_lines`
- `fs_search_text`
- `fs_find_in_files`
- `fs_write_text`
- `fs_patch_text`
- `fs_replace_lines`
- `fs_insert_text`

### Planning low-level compatibility ids

- `plan_read`
- `plan_write`

Они определены в каталоге, но обычной модели выдаётся более узкий набор:

- `init_plan`
- `add_task`
- `set_task_status`
- `add_task_note`
- `edit_task`
- `plan_snapshot`
- `plan_lint`

### `mcp_call`

Это внутренний dispatch tool runtime.

Обычная модель вместо него видит:

- `mcp_search_resources`
- `mcp_read_resource`
- `mcp_search_prompts`
- `mcp_get_prompt`
- и отдельно discovered MCP tools по их `exposed_name`

## Policy: что ещё знает runtime про tool

У каждого `ToolDefinition` кроме имени и описания есть policy:

- `read_only`
- `destructive`
- `requires_approval`

Это влияет не на сигнатуру, а на permission resolution и operator flow.

Пример:

- `fs_read_text` — read-only, без approval;
- `fs_write_text` — destructive и требует approval;
- `exec_wait` — не destructive и не требует approval;
- `exec_kill` — destructive и требует approval.

Подробнее про approval-модель: [04-tools-and-approvals.md](04-tools-and-approvals.md).

## Как оператору увидеть реальные tool calls

После выполнения хода оператор может посмотреть:

```bash
agentd session tools <session_id> --limit 50 --offset 0
agentd session tools <session_id> --results --limit 50 --offset 0
agentd session tool-result <tool_call_id>
```

Или в production/systemd-установке:

```bash
teamdctl session tools <session_id> --limit 50 --offset 0
teamdctl session tools <session_id> --results --limit 50 --offset 0
teamdctl session tool-result <tool_call_id>
```

Это уже не schema, а фактический ledger:

- какой tool вызвали;
- с какими arguments;
- какой был status;
- была ли ошибка;
- какой preview/result вернул runtime.
