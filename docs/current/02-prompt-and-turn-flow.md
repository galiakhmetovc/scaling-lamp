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
5. `Plan`
6. `ContextSummary`
7. offload refs
8. uncovered transcript tail

Это важно по двум причинам:

- модель сначала получает правила и краткий state, а уже потом длинную историю;
- разные интерфейсы не могут случайно подсунуть модели разный prompt.

Подробный разбор спорных мест и черновик целевого contract описан в [12-prompt-contract-decision.md](12-prompt-contract-decision.md).

## Что такое `SessionHead`

`SessionHead` — это короткая сводка перед transcript tail. Его строит [`build_session_head`](../../cmd/agentd/src/prompting.rs).

Туда входят:

- session id и title;
- agent profile;
- schedule summary;
- количество сообщений;
- оценка контекстных токенов;
- количество compactifications;
- previews последнего user/assistant сообщения;
- недавняя файловая активность;
- недавняя process activity;
- компактное дерево workspace;
- pending approval count.

Идея простая: вместо того чтобы заставлять модель каждый раз заново “читать” runtime state, runtime заранее упаковывает самые полезные факты. При этом состав `SessionHead` остаётся архитектурным решением: часть текущих полей полезна модели, а часть больше похожа на diagnostics и может быть вынесена в debug surfaces.

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
