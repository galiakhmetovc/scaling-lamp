# Межагентное общение, background jobs и расписания

## Agent profiles и шаблоны

Система поддерживает agent profiles. Сейчас ключевые шаблоны:

- `default`
- `judge`

Шаблон определяет:

- базовые `SYSTEM.md` и `AGENTS.md`;
- allowlist tools;
- роль агента с точки зрения оператора.

`judge` — это уже не скрытая магия, а обычный профиль агента с отдельной session и своими ограничениями.

## Как работает `message_agent`

Логика живёт в [`cmd/agentd/src/execution/interagent.rs`](../../cmd/agentd/src/execution/interagent.rs).

Когда текущий агент вызывает `message_agent`:

1. runtime валидирует target agent;
2. определяет текущую inter-agent chain или создаёт root chain;
3. проверяет, можно ли сделать ещё один hop;
4. если нельзя — ищет continuation grant;
5. создаёт child session;
6. создаёт `InterAgentMessage` job;
7. пишет system transcript entry в родительскую session;
8. возвращает ids дочерней ветки.

Результат — отдельная session вида `Agent: Judge`, а не скрытый ответ “внутри” исходного чата.

## Почему это асинхронно

Inter-agent flow специально сделан асинхронным, потому что:

- другой агент может отвечать долго;
- у него может быть свой tool loop и approvals;
- оператор должен видеть дочернюю ветку как самостоятельную сущность.

Поэтому правильная модель такая:

- `message_agent` ставит работу в очередь;
- `session_wait` или прямое чтение дочерней session позволяют наблюдать результат.

## Hop count и chain grants

Каждая inter-agent chain хранит:

- `chain_id`
- `origin_session_id`
- `origin_agent_id`
- `hop_count`
- `max_hops`
- parent linkage

Если `hop_count` достиг лимита, chain блокируется. Тогда оператор или агент с нужными правами может выдать `grant_agent_chain_continuation`.

Это сделано, чтобы inter-agent диалог не улетал в бесконтрольную recursive болтовню.

## Session wait

`session_wait` — bridge между асинхронностью и удобством:

- ждёт, пока session перестанет иметь active runs/jobs;
- в ожидании может pump’ить background jobs этой session;
- возвращает bounded transcript/timeline/summary/artifacts snapshot.

Для новичка это можно понимать так: это не “sleep”, а “подожди и честно сходи проверь, не закончилась ли дочерняя работа”.

## Background worker

Главная логика — в [`cmd/agentd/src/execution/background.rs`](../../cmd/agentd/src/execution/background.rs).

У background worker несколько задач:

- поддерживать MCP connectors;
- поддерживать memory;
- dispatch’ить due agent schedules;
- прогонять supervisor tick;
- исполнять активные jobs;
- эмитить inbox events;
- будить session wakeup turns.

Это один из важнейших слоёв для понимания runtime: многие “почему само произошло позже?” вопросы идут именно сюда.

## Типы background jobs

Worker умеет исполнять:

- `MissionTurn`
- `ChatTurn`
- `ScheduledChatTurn`
- `InterAgentMessage`
- `ApprovalContinuation`
- `Delegate`

Если job kind не поддержан, job переводится в failed с явной ошибкой.

## Schedules

Agent schedules позволяют регулярно запускать работу агента.

Ключевые параметры:

- `agent_profile_id`
- `mode`
- `delivery_mode`
- `interval_seconds`
- `prompt`
- `enabled`
- optional `target_session_id`

### Delivery modes

- `fresh_session` — каждый запуск создаёт новую session.
- `existing_session` — используется уже существующая target session.

Если `existing_session` указывает на отсутствующую session, runtime может создать новую и привязать её как target.

## Wakeups и inbox events

Система использует session inbox events как способ отложенно “разбудить” session:

- background job может положить событие в inbox;
- worker видит доступное событие;
- если у session нет активного run, запускает wakeup turn.

Это позволяет связывать завершение фоновой работы с последующим активным продолжением агента.

## Delegate и A2A

Локальное межагентное общение и удалённое A2A delegation — не одно и то же.

- локальный inter-agent flow создаёт child session внутри того же runtime;
- A2A delegation использует HTTP-ориентированный обмен между daemon’ами.

Но и то и другое стараются вписать в один execution model: jobs, runs, transcripts, callbacks, inbox events.

## Что смотреть в коде

- Inter-agent queue/wait/grant: [`cmd/agentd/src/execution/interagent.rs`](../../cmd/agentd/src/execution/interagent.rs)
- Background worker: [`cmd/agentd/src/execution/background.rs`](../../cmd/agentd/src/execution/background.rs)
- Agent metadata and prompt fallbacks: [`cmd/agentd/src/agents.rs`](../../cmd/agentd/src/agents.rs)
- A2A integration: [`cmd/agentd/src/a2a.rs`](../../cmd/agentd/src/a2a.rs), [`cmd/agentd/src/http/server/a2a.rs`](../../cmd/agentd/src/http/server/a2a.rs)
- Schedule UI/help: [`cmd/agentd/src/help.rs`](../../cmd/agentd/src/help.rs), [`cmd/agentd/src/tui/app.rs`](../../cmd/agentd/src/tui/app.rs)
