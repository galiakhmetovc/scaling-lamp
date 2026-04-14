# 2026-04-12 — teamD local skills vs global Codex skills

## Решение

Для агентов `teamD` source of truth для skills — repo-local каталог:

- `skills/<name>/SKILL.md`

Глобальные Codex skills в:

- `/home/admin/.codex/skills/...`

не считаются автоматически skills для runtime `teamD`.

## Следствие

Если skill нужен именно агентам `teamD`, его надо класть в репозиторий:

- `skills/project-knowledge-layout/SKILL.md`

а не только в глобальный каталог Codex.

## Применение

Добавлен локальный skill bundle:

- `skills/project-knowledge-layout/SKILL.md`
- `skills/project-knowledge-layout/references/project-template.md`
- `skills/project-knowledge-layout/scripts/scaffold-project-layout.sh`

Также обновлён:

- `skills/README.md`
