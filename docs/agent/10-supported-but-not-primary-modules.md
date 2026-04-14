# Supported But Not Primary Modules

Эти модули не обязательны для первого чтения бота, но они поддерживаются и участвуют в системе.

## `internal/events`

Role:
- event types and bus helpers

Почему не primary:
- не нужен для понимания простого request lifecycle

## `internal/observability`

Role:
- logging and tracing helpers

Почему не primary:
- полезно после того, как понятен основной runtime flow

## `internal/approvals`

Role:
- approval and policy surfaces

Почему не primary:
- это guardrail layer, а не ядро single-agent loop

## `internal/artifacts`

Role:
- сохранение файлов-артефактов

Почему не primary:
- это supporting storage, а не основной run path

## `internal/workspace`

Role:
- workspace context loading
- `AGENTS.md` discovery

Почему не primary:
- нужен для prompt context, но не для понимания базового polling/tool loop

## `internal/worker`

Role:
- worker runtime/checkpoint types

Почему не primary:
- часть старой/параллельной архитектурной линии, которую ещё предстоит дочистить

## `internal/coordinator`

Role:
- coordination/service abstractions

Почему не primary:
- текущий single-agent путь уже не должен требовать чтения coordinator internals первым делом

## `internal/mesh`

Role:
- multi-agent orchestration

Почему не primary:
- выключен по умолчанию
- описан отдельно в [mesh-boundary.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/mesh-boundary.md)
