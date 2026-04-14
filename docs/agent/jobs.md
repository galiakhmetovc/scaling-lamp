# Jobs

`Jobs` в `teamD` — это фоновые процессы под управлением runtime.

Это отдельная сущность от `runs`.

## Зачем они нужны

`Run` нужен, когда модель думает и разговаривает.

`Job` нужен, когда надо:

- запустить команду detached
- сохранить её status
- читать stdout/stderr позже
- отменить выполнение
- пережить рестарт процесса

То есть `job` — это supervised background execution primitive.

## Что у job есть

- `job_id`
- `chat_id`
- `session_id`
- `command`
- `args`
- `cwd`
- `status`
- `exit_code`
- `failure_reason`
- `cancel_requested`

И отдельно:

- `runtime_job_logs`
  - stdout/stderr chunks
- `runtime_events`
  - lifecycle events вроде `job.created`, `job.started`, `job.completed`

## Где смотреть код

- [jobs_service.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/jobs_service.go)
- [types.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/types.go)
- [store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/store.go)
- [postgres_store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/postgres_store.go)
- [sqlite_store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/sqlite_store.go)

## Как это течёт

1. клиент вызывает `POST /api/jobs`
2. runtime создаёт `JobRecord` со статусом `queued`
3. job запускается detached
4. stdout/stderr режутся на chunks и пишутся в `runtime_job_logs`
5. status обновляется в `runtime_jobs`
6. lifecycle пишется в `runtime_events`

## Почему job не равен worker

`Job` не умеет:

- inbox/outbox
- свою agent session
- свой LLM loop
- memory isolation

Если тебе нужен просто background process, используй `job`.

Если тебе нужен локальный субагент, используй `worker`.

## Как тестировать

```bash
curl -X POST http://127.0.0.1:18081/api/jobs \
  -H 'Content-Type: application/json' \
  -d '{"chat_id":1001,"session_id":"1001:default","command":"bash","args":["-lc","printf hello"]}'

teamd-agent jobs list
teamd-agent jobs show job-1
teamd-agent jobs logs job-1
teamd-agent jobs cancel job-1
```
