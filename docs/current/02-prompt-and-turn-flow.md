# Prompt assembly и chat turn flow

## Откуда берётся prompt

Prompt собирается не “из воздуха” и не отдельно для каждого интерфейса. Каноническая логика сидит в:

- [`cmd/agentd/src/prompting.rs`](../../cmd/agentd/src/prompting.rs)
- [`crates/agent-runtime/src/prompt.rs`](../../crates/agent-runtime/src/prompt.rs)

### Порядок сборки

Фактический порядок в текущем коде:

1. `SYSTEM.md`
2. `AGENTS.md`
3. active skill prompts
4. `SessionHead`
5. `AutonomyState`
6. `PlanPromptView`
7. `ContextSummary`
8. offload refs
9. `RecentToolActivity`
10. uncovered transcript tail

Это важно по двум причинам:

- модель сначала получает правила и краткий state, а уже потом длинную историю;
- разные интерфейсы не могут случайно подсунуть модели разный prompt.

Подробный разбор спорных мест и черновик целевого contract описан в [12-prompt-contract-decision.md](12-prompt-contract-decision.md).

## Что такое `SessionHead`

`SessionHead` — это короткая сводка перед transcript tail. Его строит [`build_session_head`](../../cmd/agentd/src/prompting.rs).

Туда входят:

- session id и title;
- agent profile и путь к `agent_home`;
- provider, model и think level, если они известны;
- context window, auto-compaction trigger, usable context budget и текущая оценка prompt;
- workspace root;
- количество сообщений;
- оценка контекстных токенов;
- количество compactifications;
- previews последнего user/assistant сообщения;
- небольшой workspace overview/tree;
- pending approval count.

Идея простая: вместо того чтобы заставлять модель каждый раз заново “читать” runtime state, runtime заранее упаковывает самые полезные факты. `SessionHead` теперь должен оставаться runtime orientation block. Schedule, subagents, A2A и mesh obligations уходят в отдельный `AutonomyState`, а недавняя tool/debug activity — в `RecentToolActivity` и debug surfaces.

## Что такое `AutonomyState`

`AutonomyState` — отдельный bounded слой для автономной работы агента.

Туда должны попадать:

- источник turn: direct user, Telegram, schedule, wakeup, inter-agent, continuation;
- активное расписание, если turn пришёл от schedule;
- сведения о subagents/delegated sessions;
- agent-to-agent chain metadata;
- mesh/node hints, когда mesh станет runtime-сущностью.

Полное состояние читается через schedule/session/agent tools и будущий aggregate tool `autonomy_state_read`.

`autonomy_state_read` уже доступен как canonical model-facing tool. Он возвращает bounded aggregate по текущей session: источник turn, связанные schedules, active jobs, child sessions, inbox events, inter-agent chain metadata и configured A2A peers. Это read-only view; изменения выполняются отдельными schedule/session/agent tools.

## Что такое `RecentToolActivity`

`RecentToolActivity` — bounded view по persisted `tool_calls`.

Он нужен, чтобы модель видела недавние ошибки и значимые результаты tools, но prompt не превращался в audit log. Текущий turn всё ещё получает tool outputs через provider continuation path, а прошлые tool outcomes попадают сюда в компактном виде. Полные аргументы, stdout/stderr, raw result и artifact payload читаются через debug surfaces и artifact tools.

## Prompt budget tools

`usable_context_tokens` считается как `context_window_tokens * auto_compaction_trigger_ratio`.

Модель может читать и менять распределение бюджета через canonical tools:

- `prompt_budget_read` — показывает effective source policy, context window, auto-compaction basis, usable context tokens, проценты слоёв и target tokens по слоям. Если стоит одноразовый override на следующий turn, `source` будет `next_turn_override`, а `pending_next_turn_override=true`.
- `prompt_budget_update(scope="session")` — меняет durable session policy.
- `prompt_budget_update(scope="next_turn")` — ставит одноразовый override на следующий полный prompt assembly и не меняет durable session policy.
- `prompt_budget_update(scope="next_turn", reset=true)` без `percentages` очищает queued one-shot override.
- После merge сумма процентов обязана быть `100`, иначе tool возвращает ошибку.

Слои prompt физически ограничиваются этим budget при сборке provider request. Если слой не помещается в свой target, runtime оставляет bounded excerpt/tail и добавляет system notice `Prompt Budget Truncation` с `layer`, `target_tokens`, `original_approx_tokens`, `hidden_approx_tokens` и, где применимо, `hidden_messages`. Полное скрытое содержимое остаётся в канонических источниках: файлы `SYSTEM.md`/`AGENTS.md`, skill files, transcript/debug surfaces и artifacts.

`transcript_tail` режется с конца: runtime сохраняет самые новые сообщения и скрывает более старые uncovered messages. Остальные system layers режутся как prefix/excerpt внутри своего слоя.

Важное ограничение: provider continuation rounds не пересобирают полный prompt. Поэтому `scope="next_turn"` не влияет на текущий tool loop после вызова tool; он применяется к следующему user/scheduled/inter-agent turn, где runtime снова собирает base prompt, и сразу очищается после применения.

## Откуда берутся системные тексты

`prompting.rs` сейчас загружает:

- `SYSTEM.md` из `agent_home/<agent>/SYSTEM.md`;
- `AGENTS.md` из `agent_home/<agent>/AGENTS.md`;
- если файлов нет — fallback templates из [`cmd/agentd/src/agents.rs`](../../cmd/agentd/src/agents.rs).

Это значит, что agent profile реально влияет на то, как модель будет себя вести.

Важный открытый вопрос: fallback сейчас зависит от `agent_id`, а целевая модель, скорее всего, должна иметь общий emergency fallback для всех профилей. Built-in profile prompts должны материализоваться в `agent_home`, а не оставаться скрытой per-agent подстановкой в prompt loader.

## Активные skills

Сессия может включать skills. `prompting.rs`:

- сканирует skill catalog;
- берёт активные skills;
- подмешивает их body в prompt.

То есть skills — это не отдельная магия TUI, а часть канонического prompt assembly.

Общий список всех skills в prompt не вставляется и не должен вставляться. В prompt попадают только active skill prompts.

Текущая auto-activation простая: manual enable/disable через session settings, иначе token overlap по имени/описанию skill с title и последними user-сообщениями. Целевой contract требует budgeted `SkillPromptView`: header, activation mode/reason, path/ref и bounded body/excerpt.

Модель может сама инспектировать и менять session-level activation через canonical tools:

- `skill_list` — показывает merged catalog для текущего agent profile: global skills плюс agent-local overrides, включая activation mode и paths.
- `skill_read` — читает bounded body конкретного `SKILL.md`, если активного excerpt в prompt недостаточно.
- `skill_enable` / `skill_disable` — меняют только session settings; skill files и templates не редактируются.

Практический смысл: prompt остаётся компактным, но агент не слепой. Он видит активные skills в prompt, а общий каталог и полные инструкции может запросить явно.

## Как выполняется обычный chat turn

Основная логика живёт в [`cmd/agentd/src/execution/provider_loop.rs`](../../cmd/agentd/src/execution/provider_loop.rs).

Упрощённый сценарий:

1. Runtime загружает session, transcript, runs, plan, summary, skills.
2. Строит `PromptAssemblyInput`.
3. `PromptAssembly::build_messages(...)` превращает его в provider messages.
4. Создаёт `ProviderRequest`.
5. Отправляет request в provider.
6. Получает:
   - text deltas,
   - reasoning deltas,
   - tool calls,
   - final output.
7. Если нужны tools — запускает tool round.
8. Если нужен approval — переводит run в waiting state и сохраняет loop state.
9. Если provider завершил ответ — пишет transcript/run result.

## Что хранит provider loop

`ProviderLoopCursor` хранит state между round’ами:

- current round;
- max rounds;
- pending tool outputs;
- continuation messages;
- previous response id;
- seen tool signatures;
- completion nudges used;
- empty-response recoveries used.

Это нужно, чтобы:

- не повторять бесконечно одинаковые tool calls;
- уметь продолжать provider turn после tool round;
- использовать `previous_response_id`, если provider это умеет;
- bounded-образом восстанавливаться после временных провалов.

## Как работают tool rounds

Если provider вернул `tool_calls`, loop:

1. разбирает tool call в typed `ToolCall`;
2. проверяет permission policy;
3. либо выполняет tool сразу;
4. либо создаёт approval;
5. возвращает output обратно в provider как structured tool result.

Важный момент: tool round — это часть того же provider loop, а не “внешний мини-рантайм”.

## Защита от зацикливания

В loop есть несколько защит:

- ограничение числа tool rounds;
- ограничение на одинаковые подряд tool signatures;
- ограничение на transient retries;
- ограничение на empty-response recovery;
- completion nudges.

Это значит, что система предпочитает остановиться с явной причиной, а не молча крутиться.

## Approval semantics

Если tool требует approval:

- run получает `waiting_approval`;
- в store сохраняется `PendingProviderApproval`;
- оператор или auto-approve позже продолжает этот же run;
- continuation идёт через ту же каноническую provider loop логику.

То есть approval — это пауза одного и того же run, а не отдельный чат.

## Completion nudges

Runtime отслеживает ситуацию, когда модель остановилась слишком рано. Вместо немедленного провала можно:

- либо автоматически “пнуть” модель ещё раз;
- либо запросить approval на continuation.

Эта логика важна для UX: агент может не бросать работу после первой слишком ранней остановки.

## Context offload и artifacts

Большие outputs не кладутся целиком обратно в prompt. Вместо этого:

- payload сохраняется в offload/artifact storage;
- в prompt уходит bounded summary или ссылка;
- при необходимости модель или оператор читают artifact явно.

Это ключевой механизм масштабирования long sessions.

Offload refs — это не все artifacts сессии. Это bounded набор ссылок, который runtime считает relevant context для модели. Полный artifact читается явно.

Как refs попадают в prompt:

- `ContextOffloadSnapshot` хранит bounded список refs на крупные payload;
- `PromptAssembly` выбирает refs внутри budget слоя `offload_refs`;
- manual pinned refs идут первыми;
- auto-pinned refs идут следующими после 3 явных `artifact_read` в этой session;
- newest refs заполняют остаток budget;
- если refs не поместились, prompt показывает hidden count и подсказку читать полный context через `artifact_search` или `artifact_read`.

Модель может управлять видимостью refs через canonical tools:

- `artifact_read` — читает payload и увеличивает `explicit_read_count`;
- `artifact_search` — ищет по labels, summaries и payloads текущего context offload;
- `artifact_pin` — вручную закрепляет ref в будущих prompt;
- `artifact_unpin` — снимает только ручной pin; auto-pin сохранится, если ref уже прочитан 3+ раза.

## Compaction

Compaction сейчас не “магическая фоновая сила”, а управляемый механизм работы с длинной историей:

- session может иметь `ContextSummary`;
- summary покрывает часть старого transcript;
- tail остаётся не покрытым;
- prompt assembly подмешивает summary и только uncovered tail.

Compaction теперь бывает двух видов:

- ручная — оператор вызывает `\компакт`;
- автоматическая — runtime запускает её перед provider turn, если оценённый prompt достигает `auto_compaction_trigger_ratio` от известного context window.

Это важно знать при отладке: summary больше не означает только ручное действие оператора. Нужно смотреть config и конкретный execution path.

## Где смотреть в коде

- Prompt assembly contract: [`crates/agent-runtime/src/prompt.rs`](../../crates/agent-runtime/src/prompt.rs)
- SessionHead construction: [`cmd/agentd/src/prompting.rs`](../../cmd/agentd/src/prompting.rs)
- Main provider loop: [`cmd/agentd/src/execution/provider_loop.rs`](../../cmd/agentd/src/execution/provider_loop.rs)
- Tool execution bridge: [`cmd/agentd/src/execution/tools.rs`](../../cmd/agentd/src/execution/tools.rs)
- Memory/session read surface: [`cmd/agentd/src/execution/memory.rs`](../../cmd/agentd/src/execution/memory.rs)
