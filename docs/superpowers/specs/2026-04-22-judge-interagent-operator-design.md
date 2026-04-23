# Judge And Inter-Agent Operator UX Design

## Goal

Сделать judge и межагентское взаимодействие операторски удобными из CLI и TUI, не добавляя второй execution path и не дублируя runtime semantics.

## Current Gap

Runtime уже умеет:
- `message_agent`
- `grant_agent_chain_continuation`
- reply routing
- chain metadata и hop limits

Но operator UX фрагментирован:
- у оператора нет прямой команды отправить сообщение агенту;
- нет прямой команды выдать continuation grant для blocked chain;
- chain state виден только в debug-heavy surfaces;
- TUI не даёт удобного сценария для judge review из текущей сессии.

## Chosen Approach

Использовать существующий канонический app/runtime слой и добавить поверх него операторские команды и TUI-диалоги:

1. App/bootstrap получает прямые operator methods:
   - отправить сообщение агенту из текущей сессии;
   - выдать continuation grant по `chain_id`;
   - рендерить компактный inter-agent state для текущей сессии.
2. CLI получает симметричные команды:
   - `\агент написать <agent_id> <message>`
   - `\цепочка продолжить <chain_id> <reason>`
   - удобный alias для judge:
     - `\судья <message>`
3. TUI получает:
   - диалог "написать агенту" из browser/текущей сессии;
   - быстрый judge-flow для выбранного judge;
   - диалог continuation grant для blocked chain;
   - richer chain-state в `\статус` / `\система` и chat header detail lines.

## Non-Goals

- не делать отдельный inter-agent dashboard;
- не делать новую prompt assembly path;
- не вводить отдельную background/delegate loop специально для UX;
- не закрывать сейчас remote A2A operator lifecycle.

## Data And State

Канонические источники состояния остаются прежними:
- `Session`
- `RunSnapshot`
- `JobSpec`
- transcript entries
- inter-agent chain records / grants

Новый UX должен читать и мутировать только эти существующие сущности через app-layer methods.

## Surfaces

### CLI

Добавить operator commands в существующий REPL path:
- `\агент написать`
- `\судья`
- `\цепочка продолжить`

Они должны идти не через текст к модели, а напрямую через app/backend methods.

### TUI

Встроить новые действия в существующий browser/dialog pattern:
- agents browser: action "написать"
- chat/session: action "судья"
- dialog для continuation grant

Никаких новых экранов.

### Status And Debug

`render_active_run` и `render_system` должны явно показывать:
- active/blocked chain id
- hop/max_hops
- chain state
- parent/child inter-agent linkage where available

## Testing

Нужны регрессии на:
- app-level operator send/grant methods
- CLI command parsing and output
- TUI dialogs/actions
- enriched status/system rendering for chain metadata

