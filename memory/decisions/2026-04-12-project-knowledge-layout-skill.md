# 2026-04-12 — Project Knowledge Layout Skill

## Решение

Добавлен глобальный skill `project-knowledge-layout` в:

- `/home/admin/.codex/skills/project-knowledge-layout/SKILL.md`

## Зачем

- нужен повторяемый протокол хранения проектных данных в файлах
- агент должен понимать, где искать source of truth
- chat/session memory не должна быть единственным носителем проектного контекста

## Каноническая модель

- top-level registry: `projects/index.md`
- per-project files:
  - `README.md`
  - `docs/architecture.md`
  - `docs/decisions.md`
  - `state/current.md`
  - `state/backlog.md`
  - `notes/YYYY-MM-DD.md`
  - `artifacts/`

## Артефакты skill

- skill: `/home/admin/.codex/skills/project-knowledge-layout/SKILL.md`
- template: `/home/admin/.codex/skills/project-knowledge-layout/references/project-template.md`
- scaffold: `/home/admin/.codex/skills/project-knowledge-layout/scripts/scaffold-project-layout.sh`
- agent wiring: `/home/admin/.codex/skills/project-knowledge-layout/agents/openai.yaml`
