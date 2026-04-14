# CLI

CLI живёт в том же бинарнике `teamd-agent`, но сам не содержит runtime-логики.

Он всегда ходит в HTTP API.

Это важное правило:

- CLI не знает, как работает агент внутри
- CLI знает только API contract

По умолчанию CLI теперь печатает короткий operator-friendly plain output для control-plane команд.

Если нужен быстрый вход без вспоминания синтаксиса:

```bash
teamd-agent help
teamd-agent --help
```

Это печатает короткую карту самых частых operator-команд.

Если нужен стабильный машинный вывод, используй глобальный флаг:

```bash
teamd-agent --json runs list
teamd-agent --json jobs show job-1
teamd-agent --json approvals list 1001:default
```

Если API защищён, CLI берёт bearer token из `TEAMD_API_AUTH_TOKEN`.
Разово переопределить его можно так:

```bash
teamd-agent --api-token operator-secret runs list
```

## Основные команды

```bash
teamd-agent runtime show
teamd-agent runtime show 1001:default
teamd-agent runtime clear 1001:default
teamd-agent runtime memory-profile 1001:default standard

teamd-agent chat 1001 1001:default

teamd-agent events list
teamd-agent events list run run-1
teamd-agent events list worker worker-1 10 50
teamd-agent events watch run run-1

teamd-agent artifacts show artifact://tool-output-1
teamd-agent artifacts cat artifact://tool-output-1
teamd-agent artifacts search run run-1 error
teamd-agent artifacts search --global error

teamd-agent plans list run run-1
teamd-agent plans show plan-1
teamd-agent plans create run run-1 "Investigate rollout"
teamd-agent plans replace-items plan-1 '["Inspect runtime","Verify CLI"]'
teamd-agent plans note plan-1 "Focus on runtime-owned state."
teamd-agent plans start-item plan-1 plan-1-item-1
teamd-agent plans complete-item plan-1 plan-1-item-1

teamd-agent sessions list
teamd-agent sessions list 1001
teamd-agent sessions show 1001:default
teamd-agent sessions clear 1001:default
teamd-agent sessions memory-profile 1001:default standard

teamd-agent session-actions show 1001
teamd-agent session-actions create 1001 deploy
teamd-agent session-actions use 1001 deploy
teamd-agent session-actions list 1001
teamd-agent session-actions stats 1001
teamd-agent session-actions reset 1001

teamd-agent control 1001:default run.status 1001
teamd-agent control 1001:default run.cancel 1001

teamd-agent runs list
teamd-agent runs list 1001
teamd-agent runs list 1001 1001:default
teamd-agent runs start 1001 1001:default "hello"
teamd-agent runs status run-1
teamd-agent runs replay run-1
teamd-agent runs cancel run-1

teamd-agent jobs list
teamd-agent jobs start 1001 1001:default bash -lc 'printf hello'
teamd-agent jobs show job-1
teamd-agent jobs logs job-1
teamd-agent jobs cancel job-1

teamd-agent workers list
teamd-agent workers list 1001
teamd-agent workers spawn 1001 1001:default "say hi"
teamd-agent workers show worker-1
teamd-agent workers message worker-1 "continue"
teamd-agent workers wait worker-1
teamd-agent workers handoff worker-1
teamd-agent workers close worker-1

teamd-agent approvals list 1001:default
teamd-agent approvals approve approval-1
teamd-agent approvals reject approval-1

teamd-agent memory search 1001 1001:default "token"
teamd-agent memory read continuity:1001:1001:default
```

CLI теперь показывает не только lifecycle-состояние, но и metadata control plane:

- `chat` даёт operator-style REPL поверх существующего control plane
- plain text в `chat` создаёт новый `run` в текущем `session_id`
- `chat` локально понимает `/approve`, `/reject`, `/plan`, `/handoff`, `/artifact`, `/cancel`, `/quit`
- `chat` рендерит readable `system/job/worker/plan/memory` lines из live event stream
- `runs status` и `runs list` возвращают persisted `policy_snapshot`
- `runs replay` даёт ordered observable timeline из persisted run + events + final response
- `runs status` и `workers show` также несут `artifact_refs`, если runtime offload'ил большой tool output
- `jobs show` и `workers show` тоже возвращают persisted `policy_snapshot`
- `approvals list/approve/reject` работают с approval audit metadata, а не только со статусом
- `events list` показывает `artifact.offloaded`, если tool result ушёл в artifact store
- `events watch` читает тот же persisted event plane через SSE и печатает события по мере прихода
- `artifacts show/cat` позволяют дочитать offloaded payload без раздувания prompt/history
- `artifacts search` даёт scoped-first recall по offloaded payloads без blind global scan
- `plans ...` дают оператору first-class skeleton работы для run/worker, а не только текстовые ответы агента
- `workers handoff ...` даёт canonical summary worker execution вместо необходимости читать весь transcript
- generic control actions теперь существуют в API отдельно от Telegram callback model:
  - `run.status`
  - `run.cancel`
- session management actions теперь тоже существуют вне Telegram:
  - `session.show`
  - `session.create`
  - `session.use`
  - `session.list`
  - `session.stats`
  - `session.reset`

## Где смотреть код

- [cli.go](/home/admin/AI-AGENT/data/projects/teamD/cmd/coordinator/cli.go)
- [client.go](/home/admin/AI-AGENT/data/projects/teamD/internal/cli/client.go)

## Что важно понять новичку

CLI здесь нужен не как “удобная консолька”, а как reference client.

Если CLI работает через API, значит:

- потом можно поднять Web UI без второго orchestration path
- Telegram не остаётся единственным способом управлять runtime
- оператор может одинаково работать с runs/sessions/approvals независимо от transport

И теперь ещё:

- `jobs` через CLI показывают background execution как first-class subsystem
- `workers` через CLI показывают local subagent control plane без mesh

## Chat Console

`teamd-agent chat <chat_id> <session_id>` нужен, когда оператору нужен один живой поток, а не набор отдельных control-plane команд.

Если забываешь формат, CLI теперь отвечает usage + пример:

```text
usage: teamd-agent chat <chat_id> <session_id>
example: teamd-agent chat 1001 1001:main
```

Внутри чата сейчас есть:

- `you:` отправленная реплика
- `system:` run lifecycle и fallback status
- `job:` background jobs
- `worker:` worker lifecycle и handoff
- `plan:` persisted plan updates
- `memory:` artifact offload и related runtime signals
- `approval:` решения оператора

Локальные команды:

- `/approve <id>`
- `/reject <id>`
- `/plan`
- `/handoff <worker_id>`
- `/artifact <ref>`
- `/cancel`
- `/quit`

В интерактивном TTY режиме у `chat` теперь есть минимальный `tab` completion:

- slash-команды
- known approval ids
- known worker ids
- known artifact refs

Non-interactive режим через pipe сохраняет старый scanner-loop без line editor.
