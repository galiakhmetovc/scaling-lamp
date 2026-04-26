# Auto Context Compaction Design

**Дата:** 2026-04-26

## Цель

Сделать каноническую автоматическую compaction сессии перед provider turn, когда оценённый prompt достигает заданной доли context window. Новый механизм должен работать одинаково для chat, background, wakeup, mission и delegate turns без отдельного UI/runtime path.

## Проблема

Текущий runtime умеет compaction только вручную:

- оператор вызывает `\компакт` или HTTP/CLI эквивалент;
- trigger определяется только по количеству сообщений;
- execution path не защищён от длинного prompt до provider turn.

Это создаёт два разрыва:

1. длинная сессия может подойти к context limit, хотя `compaction_min_messages` ещё не сработал;
2. prompt assembly знает реальный размер контекста лучше, чем ручной оператор, но сейчас не использует это знание.

## Дизайн

### 1. Конфигурация

В `[context]` добавляются:

- `auto_compaction_trigger_ratio`
- `context_window_tokens_override`

Смысл:

- `auto_compaction_trigger_ratio` задаёт долю окна, после которой перед provider turn нужно compact history;
- `context_window_tokens_override` позволяет явно задать размер окна для текущего deployment/model.

Также добавляются env overrides:

- `TEAMD_CONTEXT_AUTO_COMPACTION_TRIGGER_RATIO`
- `TEAMD_CONTEXT_WINDOW_TOKENS`

### 2. Разрешение размера context window

Runtime определяет окно так:

1. `context.context_window_tokens_override`, если задан;
2. built-in mapping для известных моделей/provider families;
3. если размер окна не удалось определить, auto-compaction не срабатывает автоматически.

На первом этапе built-in mapping нужен как минимум для `glm-5-turbo`, чтобы production deployment работал из коробки.

### 3. Где срабатывает auto-compaction

Auto-compaction встраивается в канонический execution path перед первым provider request в `execute_provider_turn_loop`.

Ограничение:

- trigger применяется только для нового turn (`initial_loop_state == None`);
- resumed provider loop после approval/continuation не трогаем, чтобы не менять семантику уже идущего loop state.

### 4. Как считается trigger

Перед provider request runtime:

1. собирает prompt тем же каноническим способом;
2. оценивает размер prompt в токенах;
3. сравнивает его с `context_window_tokens * auto_compaction_trigger_ratio`;
4. если порог достигнут, выполняет compaction;
5. затем заново собирает prompt и продолжает обычный provider turn.

Оценка строится по реально собранным prompt messages и `instructions`/`prompt_override`, а не по числу transcript entries.

### 5. Семаника compaction

Автоматическая compaction использует тот же policy, что и ручная:

- `compaction_min_messages`
- `compaction_keep_tail_messages`
- `compaction_max_output_tokens`
- `compaction_max_summary_chars`

То есть auto-compaction не создаёт новый вид summary. Она только меняет trigger.

### 6. Prompt contract

После auto-compaction prompt assembly остаётся прежним:

1. `SYSTEM.md`
2. `AGENTS.md`
3. `SessionHead`
4. `Plan`
5. `ContextSummary`
6. offload refs
7. uncovered transcript tail

Никакого второго chat path, второго summary path или отдельного Telegram-specific compaction пути не появляется.

## Наблюдаемость

В `SessionHead` и summary остаётся текущий счётчик `compactifications`. Этого достаточно, чтобы оператор видел, что compaction уже происходила, а transcript/debug продолжали работать по тем же данным.

Отдельный audit trail для distinction `manual` vs `auto` можно добавить позже.

## Тестовая стратегия

Нужны регрессии на:

- chat turn: auto-compaction срабатывает до provider turn;
- chat turn: ниже порога compaction не происходит;
- background/wakeup path: используется тот же канонический trigger;
- prompt после auto-compaction содержит summary и только uncovered tail.
