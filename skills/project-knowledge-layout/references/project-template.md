# Project Template

Use this when creating a new project workspace or retrofitting an existing one.

## Directory Tree

```text
projects/
  index.md
  <project>/
    README.md
    docs/
      architecture.md
      decisions.md
    state/
      current.md
      backlog.md
    notes/
      YYYY-MM-DD.md
    artifacts/
```

## `projects/index.md`

```md
# Projects Index

## <Project Name>

- path: `projects/<project>`
- purpose: <one-line purpose>
- status: <active | paused | archived>
- canonical state: `projects/<project>/state/current.md`
```

## `README.md`

```md
# <Project Name>

## Purpose

<What this project is for>

## Status

<active | paused | archived>

## Canonical Files

- current state: `state/current.md`
- architecture: `docs/architecture.md`
- decisions: `docs/decisions.md`
```

## `state/current.md`

```md
# Current State

## Active Work

- <current task>

## Blockers

- <blocker or none>

## Next Steps

- <next action>
```
