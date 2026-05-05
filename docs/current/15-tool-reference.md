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
   - `artifact_read`, `artifact_search`, `artifact_pin` и `artifact_unpin` добавляются только если у session реально есть context offload;
   - `deliver_file` доступен как generic delivery tool: он не привязан к Telegram, но Telegram surface умеет доставлять queued requests как documents;
   - dynamic MCP tools добавляются отдельно как самостоятельные provider tools с собственным `exposed_name` и `input_schema`, но только для enabled MCP connectors.

### Что не стоит путать

- `mcp_call` определён в каталоге, но обычно не показывается модели как обычный tool id.
  Вместо него модель видит уже обнаруженные MCP tools по их `exposed_name`.
- legacy ids вроде `fs_read`, `fs_write`, `fs_patch`, `fs_search` живут в каталоге, но не входят в канонический automatic model-facing surface.
- `plan_read` и `plan_write` определены, но для обычного model-driven loop вместо них используются более узкие typed planning tools.
- legacy Obsidian/Lightpanda MCP tools не должны появляться в production surface, если соответствующие connectors не включены явно. Контейнерный deploy отключает `[daemon.mcp_connectors.obsidian]` и `[daemon.mcp_connectors.lightpanda]`, когда operator не просит их через legacy flags.

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

Web tools делятся на две разные capability:

- `web_search`/`web_fetch` — канонические built-in tools для поиска и прямого HTTP fetch;
- `browser_*` — канонические built-in tools для real browser automation через `agent-browser`; production backend обычно Browserless.

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

- если пользователь дал точный URL;
- если URL был найден через `web_search`;
- если это известный canonical URL документации или первоисточника.

Когда не использовать:

- не угадывать URL поисковых/погодных/новостных endpoint-ов вручную;
- не начинать веб-исследование с `web_fetch`, если источник не выбран.

### `web_search`

Сигнатура:

```text
web_search({ query: string, limit: integer })
```

Что делает:

- выполняет запрос к configured search backend;
- backend задаётся через `[web]` config;
- в production-развёртывании backend обычно переключается на локальный SearXNG (`searxng_json`).

Когда использовать:

- как первый шаг веб-исследования;
- для текущей/внешней информации: новости, погода, законы, цены, продукты, документация;
- чтобы найти candidate URLs перед `web_fetch`.

Если результатов нет:

- переформулировать запрос один раз;
- если снова пусто, явно сказать, что search backend не нашёл источников, а не выдумывать результат.

## Browser

Browser tools идут через тот же provider/tool loop, tool ledger, artifacts/offload и debug UI, что и остальные tools. Отдельного browser-agent loop нет.

Runtime вызывает `agent-browser` CLI, а конфигурация задаётся через `[browser]` или `TEAMD_BROWSER_*`. В production рекомендуется Browserless backend:

```toml
[browser]
enabled = true
command = "/opt/teamd/bin/agent-browser"
provider = "cdp"
session_prefix = "teamd"
default_timeout_ms = 30000
max_output_chars = 20000

[browser.browserless]
api_url = "http://127.0.0.1:3000"
cdp_url = "ws://127.0.0.1:3000/chromium?token=<token>"
api_key = "..."
browser_type = "chromium"
ttl_ms = 300000
stealth = true
```

### `browser_open`

```text
browser_open({ url: string, wait_until?: string | null })
```

- открывает URL в isolated browser session текущей teamD session;
- `wait_until`: `load`, `domcontentloaded` или `networkidle`.

### `browser_snapshot`

```text
browser_snapshot({
  interactive?: boolean | null,
  compact?: boolean | null,
  depth?: integer | null,
  selector?: string | null,
  max_chars?: integer | null
})
```

- возвращает accessibility snapshot;
- interactive refs вида `@e1` действуют только до следующего page-changing action;
- большие snapshots уходят в artifact/offload, их нужно читать через `artifact_read`.

### `browser_text`

```text
browser_text({ selector?: string | null, max_chars?: integer | null })
```

- возвращает text content указанного selector или `body`.

### `browser_click`

```text
browser_click({ selector: string, wait_until?: string | null })
```

- кликает CSS selector или ref из последнего snapshot;
- после клика нужно заново вызвать `browser_snapshot`, если дальше используются refs.

### `browser_fill`

```text
browser_fill({ selector: string, text: string })
```

- очищает и заполняет input/textarea/contenteditable.

### `browser_press`

```text
browser_press({ key: string })
```

- отправляет клавишу: `Enter`, `Tab`, `Control+a` и т.п.

### `browser_wait`

```text
browser_wait({ kind: string, value: string, state?: string | null })
```

- ждёт selector, milliseconds, url pattern, load state, JS expression или text;
- `kind`: `selector`, `ms`, `url`, `load`, `fn`, `text`;
- `state` применим к selector waits: `visible`, `hidden`, `attached`, `detached`.

### `browser_scroll`

```text
browser_scroll({ direction: string, pixels?: integer | null })
```

- скроллит страницу: `up`, `down`, `left`, `right`.

### `browser_eval`

```text
browser_eval({ script: string, max_chars?: integer | null })
```

- выполняет JavaScript в странице;
- использовать для inspection/extraction, а не как замену нормальным browser actions.

### `browser_screenshot`

```text
browser_screenshot({
  path?: string | null,
  full?: boolean | null,
  annotate?: boolean | null
})
```

- сохраняет screenshot в workspace-relative path;
- default path: `scratch/browser/screenshot-<timestamp>.png`.

### `browser_pdf`

```text
browser_pdf({ path: string })
```

- сохраняет PDF текущей страницы в workspace-relative path.

### `browser_status`

```text
browser_status({})
```

- показывает browser session, текущий URL и title.

### `browser_close`

```text
browser_close({ all?: boolean | null })
```

- закрывает текущую browser session или все sessions при `all=true`.

### Когда нужны browser tools

Используйте `browser_*`, если:

- страница требует JavaScript rendering;
- нужно нажимать кнопки, заполнять формы, скроллить или ждать DOM selector;
- `web_fetch` вернул shell HTML, пустой текст или контент, который явно не соответствует странице в браузере;
- нужны screenshot/PDF или проверка интерактивного flow.

Не используйте `browser_*`, если:

- достаточно `web_search` или `web_fetch`;
- нужно просто найти текущий источник;
- задача похожа на высокочастотный scraping, bypass access controls или игнорирование правил сайта.

### Legacy browser MCP add-ons

Dynamic MCP browser tools допустимы только для явно включённых legacy/экспериментов. В обычном production deploy старый Lightpanda connector выключается, чтобы модель использовала built-in `browser_*`. Пример старого Lightpanda connector:

```text
mcp__lightpanda__goto({ url: string })
mcp__lightpanda__markdown({})
mcp__lightpanda__semantic_tree({})
mcp__lightpanda__links({})
mcp__lightpanda__interactiveElements({})
mcp__lightpanda__click({ ... })
mcp__lightpanda__fill({ ... })
mcp__lightpanda__waitForSelector({ ... })
```

Точные имена и schemas приходят от MCP connector во время discovery. Архитектурное правило сохраняется: MCP connector расширяет canonical provider tool surface, но не создаёт отдельный prompt path, chat loop или debug ledger.

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
exec_wait({ process_id: string, timeout_ms?: integer | null })
```

Что делает:

- ждёт завершения процесса;
- имеет hard timeout: по умолчанию 10 минут, максимум 60 минут;
- при timeout завершает process group и возвращает `status=timed_out`;
- возвращает итоговый статус и bounded output summary.

Когда использовать:

- когда процесс уже запущен и нужно дождаться результата.
- для долгих процессов сначала используйте `exec_read_output`, а не blocking wait.

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

- читает effective prompt budget policy для следующей полной сборки prompt;
- возвращает `context_window_tokens`, `auto_compaction_trigger_basis_points`, `usable_context_tokens`;
- возвращает `source`: `runtime_default`, `session_override` или `next_turn_override`;
- возвращает `pending_next_turn_override`;
- возвращает проценты и target tokens для слоёв `SYSTEM`, `AGENTS`, active skills, `SessionHead`, `AutonomyState`, plan, summary, offload refs, recent tool activity и transcript tail.

Когда использовать:

- если нужно понять, сколько usable context доступно;
- перед изменением prompt allocation через `prompt_budget_update`.

### `prompt_budget_update`

Сигнатура:

```text
prompt_budget_update({
  scope?: "session" | "next_turn",
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

- `scope="session"` меняет durable session-scoped prompt budget policy;
- `scope="next_turn"` ставит одноразовый override на следующий полный prompt assembly, не меняя durable session policy;
- если `reset=true`, сначала возвращает выбранный scope к default;
- `scope="next_turn", reset=true` без `percentages` очищает queued one-shot override;
- затем merge-ит переданные проценты;
- отклоняет изменение, если итоговая сумма процентов не равна `100`;
- влияет на физическое ограничение prompt layers при следующих provider requests.

Когда использовать:

- `scope="session"` — когда текущей задаче реально нужен durable context allocation;
- `scope="next_turn"` — когда нужно разово изменить allocation для следующего user/scheduled/inter-agent turn;
- например временно увеличить `transcript_tail` или `recent_tool_activity`, чтобы модель лучше видела свежую историю или недавние tool ошибки.

Важно:

- provider continuation rounds не пересобирают base prompt, поэтому `scope="next_turn"` не влияет на текущий tool loop после вызова tool;
- если layer превышает target, prompt содержит `Prompt Budget Truncation` notice с количеством скрытых approximate tokens/messages;
- `transcript_tail` сохраняет новые сообщения, а старые uncovered messages скрываются первыми;
- скрытое содержимое не теряется: его можно читать через transcript/debug/session/artifact surfaces.

## Autonomy

### `autonomy_state_read`

Сигнатура:

```text
autonomy_state_read({
  max_items?: integer | null,
  include_inactive_schedules?: boolean | null
})
```

Что делает:

- возвращает bounded aggregate state текущей session;
- показывает `turn_source`, parent session/job, `delegation_label`;
- показывает связанные schedules текущего workspace и agent/session;
- показывает active jobs, child sessions, inbox events;
- показывает inter-agent chain metadata, если session является частью A2A chain;
- показывает configured A2A/mesh peers из daemon config.

Когда использовать:

- перед автономной работой по расписанию, wakeup, delegation или agent-to-agent flow;
- если нужно понять, почему session проснулась или какие фоновые работы сейчас связаны с ней;
- если нужно одно компактное представление вместо отдельных `schedule_list`, `session_read`, `agent_list` и debug views.

Важно:

- это read-only aggregate, а не отдельный runtime path;
- полные детали по-прежнему читаются каноническими tools: `schedule_read`, `session_read`, `session_wait`, debug/TUI views.

## Skills

### `skill_list`

Сигнатура:

```text
skill_list({
  include_inactive?: boolean | null,
  limit?: integer | null,
  offset?: integer | null
})
```

Что делает:

- показывает merged skill catalog для текущей session: global skills плюс agent-local overrides;
- возвращает `name`, `description`, activation `mode`, `skill_dir`, `skill_md_path`;
- по умолчанию `include_inactive=true`, чтобы модель могла обнаружить полный каталог до активации;
- поддерживает пагинацию.

Когда использовать:

- если задача похожа на специализированный workflow, но нужный skill ещё не активен;
- перед `skill_read`, `skill_enable` или `skill_disable`, если точное имя skill неизвестно.

### `skill_read`

Сигнатура:

```text
skill_read({ name: string, max_bytes?: integer | null })
```

Что делает:

- читает body `SKILL.md` для skill из текущего merged catalog;
- возвращает description, activation mode, paths, body, исходный `body_byte_len` и `body_truncated`;
- `max_bytes` bounded и режется по UTF-8 границе.

Когда использовать:

- перед тем как полагаться на детальные правила skill;
- если active skill prompt был усечён budget policy;
- если нужно понять, как правильно пользоваться внешним workflow или MCP connector.

### `skill_enable`

Сигнатура:

```text
skill_enable({ name: string })
```

Что делает:

- вручную включает skill для текущей session через `SessionSettings.enabled_skills`;
- снимает конфликтующий disabled override;
- не редактирует `SKILL.md` и не меняет global/agent template.

Когда использовать:

- если модель поняла, что skill нужен дальше в этой session;
- после `skill_list`/`skill_read`, когда activation должна стать устойчивой для следующих turns.

### `skill_disable`

Сигнатура:

```text
skill_disable({ name: string })
```

Что делает:

- вручную отключает skill для текущей session через `SessionSettings.disabled_skills`;
- снимает конфликтующий enabled override;
- не удаляет и не изменяет skill file.

Когда использовать:

- если auto-activation мешает текущей задаче;
- если пользователь явно просит не использовать конкретный skill в этой session.

## Offload

### `artifact_read`

Сигнатура:

```text
artifact_read({
  artifact_id: string,
  offset?: integer | null,
  max_bytes?: integer | null
})
```

Что делает:

- читает bounded page offloaded artifact по `artifact_id`;
- по умолчанию возвращает безопасную страницу, а не весь большой payload;
- `offset` продолжает чтение с byte offset из `next_offset`;
- `max_bytes` задаёт размер страницы, runtime режет по UTF-8 границе и ограничивает hard cap;
- в output возвращает `content`, `offset`, `content_byte_len`, `total_byte_len`, `content_truncated`, `next_offset`;
- увеличивает `explicit_read_count` у соответствующего `ContextOffloadRef`;
- после 3 явных чтений ref становится auto-pinned и получает приоритет в будущих `OffloadRefs`.

Когда использовать:

- если transcript/tool result дал только artifact reference;
- если нужен большой output частями, без раздувания следующего provider request.

### `artifact_search`

Сигнатура:

```text
artifact_search({ query: string, limit: integer })
```

Что делает:

- ищет по labels, summaries и payloads текущих offloaded artifacts.

### `artifact_pin`

Сигнатура:

```text
artifact_pin({ artifact_id: string })
```

Что делает:

- вручную закрепляет offloaded artifact ref;
- закреплённый ref получает приоритет в будущих prompt `OffloadRefs`;
- payload не читается автоматически, для этого нужен `artifact_read`.

Когда использовать:

- если artifact содержит важный результат, диагностику или решение, которое должно быть видно модели в следующих turn;
- если пользователь явно просит “запомнить/держать в контексте” крупный output.

### `artifact_unpin`

Сигнатура:

```text
artifact_unpin({ artifact_id: string })
```

Что делает:

- снимает manual pin с offloaded artifact ref;
- не удаляет artifact и не сбрасывает `explicit_read_count`;
- если ref уже auto-pinned из-за 3+ `artifact_read`, он всё равно может оставаться приоритетным.

Когда использовать:

- если закреплённый крупный context больше не нужен в ближайших turn;
- если prompt budget нужно освободить без удаления данных.

### `deliver_file`

Сигнатура:

```text
deliver_file({
  artifact_id?: string | null,
  workspace_path?: string | null,
  file_name?: string | null,
  caption?: string | null,
  target?: "current_chat" | null
})
```

Что делает:

- ставит файл в durable очередь доставки `file_delivery_requests`;
- принимает ровно один источник: либо существующий `artifact_id`, либо `workspace_path`;
- `artifact_id` должен принадлежать текущей session;
- `workspace_path` читается только из workspace текущей session, затем сохраняется как artifact `workspace_file`;
- `file_name` задаёт внешнее имя файла; если его нет, runtime берёт `file_name` из artifact metadata или имя workspace-файла;
- `caption` передаётся surface как короткая подпись;
- `target` сейчас поддерживает только `current_chat`.

Что возвращает:

- `request_id`;
- итоговый `artifact_id`;
- `target`;
- `file_name`;
- `caption`;
- `status = "queued"`.

`queued` означает успешную постановку в очередь, а не финальную доставку. Telegram отправляет queued files после текущего ответа модели через `sendDocument`. Если `sendDocument` падает, Telegram worker помечает request как `failed`, пишет ошибку в audit и отправляет в чат отдельное сообщение о неудачной доставке. Агент не должен объявлять `queued` ошибкой и не должен придумывать fallback вроде “сохранил в Obsidian vault”.

Когда использовать:

- когда пользователь просит “пришли файл/отчёт/экспорт”;
- после создания файла через filesystem tools;
- чтобы отправить назад файл, который пользователь ранее загрузил в Telegram;
- когда ответ текстом неудобен или слишком большой, но нужен именно документ.

Важные ограничения:

- это не Telegram-specific tool и не принимает host paths;
- если путь к файлу уже известен, агент не должен читать содержимое файла через `fs_read_text`/`artifact_read` перед отправкой; достаточно сразу вызвать `deliver_file` с `workspace_path`;
- для сгенерированного файла сначала создай файл внутри текущего workspace, затем вызови `deliver_file` с `workspace_path`;
- `artifact_id` в этом механизме является внутренним durable storage/runtime detail и не должен упоминаться пользователю как “альтернатива” отправке файла;
- unsupported surfaces могут только показать actionable queued/result state, а не выполнить `sendDocument`;
- Telegram worker доставляет queued requests после текущего chat turn;
- ошибки `missing artifact` и `artifact from another session` возвращаются модели как non-retryable tool error, чтобы она могла выбрать правильный файл.

## Memory

### Semantic memory через Mem0

`memory_*` tools появляются в model-facing списке только если включён config:

```toml
[mem0]
enabled = true
api_base = "http://127.0.0.1:18888"
```

Они работают через тот же canonical provider loop и tool ledger, а не через MCP и не через отдельный prompt path. Mem0 хранит только явно записанные durable memories. Он не заменяет `state.sqlite`, transcript, tool-call ledger, artifacts, `ContextSummary` или SilverBullet/docs.

Scope:

- `operator` — память оператора, только `user_id`;
- `agent` — память конкретного agent profile, только `agent_id`;
- `agent_shared` — общий пул semantic memories для всех агентов, `agent_id = teamd-agent-shared`;
- `workspace` — default scope, память текущего workspace, `agent_id = teamd-workspace-<sha256(workspace_root)[0..16]>`;
- `session` — память текущей session, только `run_id=session_id`.

Runtime добавляет в metadata provenance поля `teamd_scope`, `teamd_session_id`, `teamd_agent_profile_id`, `teamd_workspace_root` и `teamd_source`. Это не KV-хранилище: Mem0 используется для семантических durable facts/lessons, а точные ключи, очереди, locks, counters и runtime state должны оставаться во встроенном KV/runtime-store слое `state.sqlite`.

### Runtime KV через `state.sqlite`

`kv_*` tools всегда идут через тот же canonical provider loop, tool ledger и approval layer. Они не используют Mem0, MCP, Redis или отдельный сервис. Данные лежат в `state.sqlite` в таблице `kv_entries`.

KV хранит exact JSON values. Он не делает embeddings, ranking, inference или semantic search.

Scope mapping:

- `operator` — `namespace_id = mem0.default_user_id`;
- `agent` — `namespace_id = <agent_profile_id>`;
- `agent_shared` — `namespace_id = teamd-agent-shared`;
- `workspace` — default, `namespace_id = teamd-workspace-<sha256(workspace_root)[0..16]>`;
- `session` — `namespace_id = <session_id>`.

Когда использовать KV:

- точное состояние, которое нужно читать по ключу;
- counters, cursors, lightweight locks и feature flags;
- machine-readable state между turns/sessions;
- short structured state с optional TTL.

Когда не использовать KV:

- для семантических воспоминаний и предпочтений — используйте `memory_*`;
- для больших payload — используйте artifacts/offload;
- для знаний и заметок — используйте docs/SilverBullet/knowledge tools.

### `kv_get`

Сигнатура:

```text
kv_get({
  key: string,
  scope?: "operator" | "agent" | "agent_shared" | "workspace" | "session" | null
})
```

Что делает:

- читает одну exact запись из `(scope, namespace_id, key)`;
- default scope — `workspace`;
- не возвращает expired записи.

### `kv_put`

Сигнатура:

```text
kv_put({
  key: string,
  value: any_json,
  scope?: "operator" | "agent" | "agent_shared" | "workspace" | "session" | null,
  metadata?: object | null,
  expected_revision?: integer | null,
  ttl_seconds?: integer | null
})
```

Что делает:

- пишет JSON value по exact key;
- увеличивает `revision`;
- сохраняет `created_at`, `updated_at`, optional `expires_at`;
- `expected_revision` включает compare-and-set: `0` означает create-only, текущее число означает update-only на ожидаемой версии;
- `ttl_seconds` задаёт срок жизни записи.

Ограничения:

- key не пустой и не больше 512 bytes;
- value не больше 64 KiB serialized JSON;
- metadata — только object или null, не больше 16 KiB serialized JSON.

### `kv_list`

Сигнатура:

```text
kv_list({
  scope?: "operator" | "agent" | "agent_shared" | "workspace" | "session" | null,
  prefix?: string | null,
  limit?: integer | null,
  offset?: integer | null
})
```

Что делает:

- листит keys текущего namespace;
- поддерживает prefix filter;
- default `limit = 50`, hard max `500`;
- возвращает `next_offset`, если есть следующая page;
- не возвращает expired записи.

### `kv_delete`

Сигнатура:

```text
kv_delete({
  key: string,
  scope?: "operator" | "agent" | "agent_shared" | "workspace" | "session" | null,
  expected_revision?: integer | null
})
```

Что делает:

- удаляет exact key;
- destructive tool, проходит через обычный approval layer;
- поддерживает `expected_revision`, чтобы не удалить запись, которая изменилась после чтения.

### Post-turn memory curator

Кроме ручных `memory_*` tools, teamD может включить автоматический post-turn curator:

```toml
[memory_curator]
enabled = true
mode = "auto"
min_confidence = 0.8
max_candidates = 5
max_output_tokens = 512
```

Curator запускается после завершения обычного chat turn, делает отдельный короткий provider-вызов без tools и просит модель вернуть strict JSON candidates. Runtime сам применяет candidates через `memory_search` + `memory_add`; модель не получает новый tool loop и не может зависнуть на повторном `memory_add`.

Гарантии:

- основной ответ пользователю уже сохранён, поэтому ошибка curator не валит turn;
- секреты/токены/пароли/API keys/pairing keys дополнительно отбрасываются runtime-guard'ом;
- exact duplicates пропускаются после `memory_search`;
- provenance сохраняется в Mem0 metadata через `teamd_curator_run_id`, `teamd_curator_confidence`, `teamd_curator_reason`;
- summary pass пишется в `audit/runtime.jsonl` с component `memory_curator`.

### Pre-turn Memory Recall

Если включены `[mem0]` и `[memory_recall]`, runtime перед provider request сам вызывает Mem0 `POST /search` по последнему user-сообщению. Это не model tool call и не отдельная ветка чата: результат вставляется в canonical prompt как system-блок `Memory Recall` после `SessionHead`/`AutonomyState` и до `Plan`.

Default scopes:

- `operator` — предпочтения и устойчивые факты оператора;
- `workspace` — проектные решения и workspace-specific lessons;
- `agent_shared` — общие уроки и reusable operating knowledge для всех агентов.

Ограничения задаются `memory_recall.max_results`, `memory_recall.max_query_chars` и `memory_recall.max_memory_chars`. Ошибки recall не валят turn; они пишутся в `audit/runtime.jsonl` с component `memory_recall`.

### `memory_add`

Сигнатура:

```text
memory_add({
  text?: string,
  messages?: { role: "user" | "assistant" | "system" | "tool", content: string }[],
  scope?: "operator" | "agent" | "agent_shared" | "workspace" | "session" | null,
  infer?: boolean | null,
  metadata?: object
})
```

Что делает:

- явно записывает факт или фрагмент диалога в Mem0;
- принимает либо `text`, либо `messages`;
- отправляет `POST /memories` в self-hosted Mem0/OpenMemory REST API;
- добавляет ровно один entity id по выбранному scope: `user_id`, `agent_id` или `run_id`;
- если задан `api_key`, runtime отправляет его как `X-API-Key`.

Когда использовать:

- пользователь явно просит “запомни”;
- агент получил устойчивое предпочтение, правило, идентификатор проекта или долгоживущий факт;
- важно сохранить память между sessions.

Когда не использовать:

- для секретов, токенов, паролей;
- для временного состояния текущего turn;
- для tool outputs, которые уже сохранены в artifacts/tool ledger.

### `memory_search`

Сигнатура:

```text
memory_search({
  query: string,
  scope?: "operator" | "agent" | "agent_shared" | "workspace" | "session" | null,
  limit?: integer | null,
  filters?: object
})
```

Что делает:

- ищет релевантные memories через `POST /search`;
- автоматически добавляет entity filters по выбранному scope;
- отправляет Mem0 body с `filters` и `top_k`; поле tool input `limit` конвертируется в `top_k`;
- ограничивает `limit` через `mem0.default_limit` и `mem0.max_limit`.

Когда использовать:

- до ответа, если агенту нужен долгоживущий контекст о пользователе, проекте или workspace;
- если обычный `ContextSummary`/transcript tail не должен раздуваться историей.

### `memory_list`

Сигнатура:

```text
memory_list({
  scope?: "operator" | "agent" | "agent_shared" | "workspace" | "session" | null,
  limit?: integer | null,
  offset?: integer | null,
  filters?: object
})
```

Что делает:

- читает список memories через `GET /memories`;
- применяет bounded pagination на стороне runtime;
- useful для audit/debug и чистки.

### `memory_update`

Сигнатура:

```text
memory_update({ memory_id: string, text: string, metadata?: object })
```

Что делает:

- обновляет конкретную memory через `PUT /memories/{memory_id}`;
- не ищет memory сам, сначала используй `memory_search` или `memory_list`.

### `memory_delete`

Сигнатура:

```text
memory_delete({ memory_id: string })
```

Что делает:

- удаляет конкретную memory через `DELETE /memories/{memory_id}`;
- помечен destructive и требует approval по обычным правилам approval layer.

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

- ищет через SQLite FTS по configured canonical teamD docs/workspace knowledge index;
- источники строго задаются в `[knowledge].source_files` и `[knowledge].source_dirs` внутри agent workspace: например `README.md`, `SYSTEM.md`, `AGENTS.md`, `docs/`, `projects/`, `notes/`, extra roots;
- unreadable/stale/non-UTF8 файлы при обновлении индекса пропускаются и не должны валить весь tool;
- возвращает source metadata и bounded results.

Что это не делает:

- не делает произвольный поиск по файловой системе и не должен использоваться вместо `fs_find_in_files`/`fs_search_text`;
- не ищет в Mem0 semantic memory: для этого есть `memory_search`;
- не читает scoped exact state: для этого есть `kv_get`/`kv_list`;
- не читает artifacts, transcripts или tool-call ledger: для этого есть session/debug/artifact surfaces;
- не является SilverBullet-specific API. Если нужен именно SilverBullet Space, используйте active skill `silverbullet-space` и available MCP/filesystem path для этого space.

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

- читает один source из canonical teamD docs/workspace knowledge index в bounded excerpt/full режиме.
- это не замена `fs_read_text`/`fs_read_lines`: `knowledge_read` принимает только indexed source path, обычно найденный через `knowledge_search`, и дополнительно проверяет canonical knowledge roots.

Важно:

- enum-like аргументы должны быть quoted JSON strings;
- то есть `"mode": "full"`, а не `"mode": full`.
- `path` должен быть путём knowledge source, обычно полученным из `knowledge_search`; произвольные filesystem или SilverBullet paths сюда не передаются.

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

Пример: если оператор явно включил legacy connector `[daemon.mcp_connectors.lightpanda]`, модель может увидеть browser tools с exposed names вида `mcp__lightpanda__goto`, `mcp__lightpanda__markdown`, `mcp__lightpanda__semantic_tree`, `mcp__lightpanda__click`, `mcp__lightpanda__fill`. Для нового браузерного workflow предпочтительны built-in `browser_*` tools.

Legacy Obsidian connector `[daemon.mcp_connectors.obsidian]` аналогично должен быть disabled в текущем production stack. Основной путь для заметок: `silverbullet-space` skill, SilverBullet MCP when enabled, или bounded filesystem tools в canonical Markdown space.

Правило архитектуры: MCP connector расширяет canonical provider tool surface, но не создаёт отдельную модель prompt assembly, отдельный chat loop или отдельный debug ledger. Вызовы MCP tools должны попадать в тот же tool-call ledger, что и built-in tools.

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
- копирует `SYSTEM.md`, `AGENTS.md` и template skills в отдельный `agent_home`;
- создаёт отдельный default workspace для нового профиля: `workspaces/agents/<agent_id>`.

Важно:

- это не запускает отдельный процесс агента;
- это создаёт durable `Agent profile`, который можно выбрать для новых session или использовать в schedules/inter-agent calls.

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
