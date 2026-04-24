# Runtime Store

Связанный view: `Containers`.

Связанный C4-элемент: `Runtime Store`.

`Runtime Store` хранит durable state `teamD Runtime`.

## Что хранится

- sessions;
- transcripts;
- runs;
- jobs;
- plans;
- schedules;
- context summaries;
- artifacts и offloaded payloads;
- audit trail.

## Физическое хранение

Основные метаданные хранятся в SQLite.

Большие payloads хранятся рядом как файлы, чтобы не раздувать prompt и transcript.

## Основные связи

- `App / Runtime Core` читает и пишет `Runtime Store`.
- `Operator Surfaces` не должны обходить runtime и менять store напрямую, кроме явно разрешённых read-only/render paths.

## Правило изменения

Любая новая persisted сущность должна иметь понятную связь с `Session`, `Run`, `Job`, `Tool` или `Artifact`, иначе модель данных быстро станет нечитаемой.
