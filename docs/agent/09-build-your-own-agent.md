# Build Your Own Agent

Если тебе нужен похожий агент, минимальный набор такой.

## 1. Runtime Core First

Начинай не с мессенджера, а с runtime core.

Минимальный костяк:

- run lifecycle
- provider contract
- tool runtime
- persistent state
- event plane

Transport потом просто подключается сверху.

## 2. Transport

Нужен слой, который принимает внешний input и отправляет output.

В этом проекте есть два полноценных operator-facing транспорта:

- Telegram
- CLI operator chat / control plane

- normalize update
- dispatch command vs normal message
- send/edit/delete message
- или в CLI:
  - start run through API
  - watch events through SSE
  - render readable control-plane output

## 3. Runtime loop

Нужен слой, который умеет:

- создать run
- держать cancel token
- запускать provider rounds
- исполнять tools
- завершать run

Это ядро агента.

## 4. Provider contract

Нужен интерфейс вида:

- вход: messages + tools + config
- выход: text или tool calls

Не завязывай runtime напрямую на HTTP-клиент конкретного провайдера.

## 5. Tool runtime

Нужен единый executor для:

- shell
- filesystem
- memory
- skills
- любых domain tools

Модель должна видеть tools через общий contract, а не знать, как они реализованы внутри.

Хорошая граница:

- tool definitions отдельно
- execution отдельно
- loop guards отдельно

## 6. Memory

Минимум:

- session history
- checkpoint
- recall

Если хочешь умнее:

- memory documents
- embeddings
- vector search

## 7. Compaction

Без compaction длинные агентные сессии разваливаются.

Нужны:

- trigger
- checkpoint synthesis
- prompt assembly
- защита активного user turn

## 8. Control Plane

Если хочешь production-like single-agent систему, не останавливайся на одном run loop.

Нужны:

- HTTP API
- CLI over API
- approvals
- control state
- events and SSE
- operator chat

Это превращает агента из "бота" в наблюдаемую и управляемую систему.

## 9. Launcher

Не запускай production-like бота через `nohup` и случайные shell wrappers.

Сразу делай:

- systemd user service
- status/logs/restart
- stale process cleanup

## 10. Observability

Без этого ты не поймёшь, что именно ушло в модель.

Минимум:

- raw provider trace
- run status
- logs
- tool activity

## Рецепт

Если строить своего агента по образцу этого проекта, делай в таком порядке:

1. provider contract
2. tool runtime
3. single run loop
4. persistence
5. memory/recall
6. compaction
7. HTTP API
8. CLI / operator control plane
9. transport adapters
10. launcher
11. observability

И только потом:

12. multi-agent / mesh

## Что делать после single-agent ядра

Следующий взрослый шаг не обязательно mesh.

Правильный промежуточный этап:

1. `jobs`
   - detached background execution
2. `workers`
   - local managed subagents без mesh
3. `delegation tools`
   - чтобы основной агент мог ими пользоваться

Это даёт:

- нормальный control plane
- supervised background work
- локальную делегацию
- и только потом уже осмысленный переход к mesh

Смотри:

- [jobs.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/jobs.md)
- [workers.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/workers.md)
- [operator-chat.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/operator-chat.md)
- [http-api.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/http-api.md)
- [cli.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/cli.md)

## Минимальный рабочий skeleton

Если нужен совсем короткий стартовый пример, смотри:

- [examples/minimal-agent/README.md](/home/admin/AI-AGENT/data/projects/teamD/examples/minimal-agent/README.md)
- [examples/minimal-agent/main.go](/home/admin/AI-AGENT/data/projects/teamD/examples/minimal-agent/main.go)
- [examples/minimal-agent/provider.go](/home/admin/AI-AGENT/data/projects/teamD/examples/minimal-agent/provider.go)
- [examples/minimal-agent/tools.go](/home/admin/AI-AGENT/data/projects/teamD/examples/minimal-agent/tools.go)
- [examples/minimal-agent/memory.go](/home/admin/AI-AGENT/data/projects/teamD/examples/minimal-agent/memory.go)

Этот пример специально не тащит Telegram, Postgres, compaction и traces.

Он нужен, чтобы сначала понять базовую форму агента:

1. получить user input
2. собрать messages
3. спросить provider
4. если надо, выполнить tool
5. вернуть финальный ответ
