# Skills

File-based Agent Skills bundles live here.

## Layout

- One skill per directory
- Project-local discovery:
  - `skills/<name>/SKILL.md`
  - `.agents/skills/<name>/SKILL.md`
- Optional bundled resources are discovered under:
  - `scripts/`
  - `references/`
  - `assets/`

## `SKILL.md` Format

Supported frontmatter fields:

```yaml
---
name: example
description: Example skill bundle for Telegram bot sessions
version: 1
license: Apache-2.0
allowed-tools:
  - shell.exec
---
```

The body after frontmatter becomes the injected skill prompt.

The parser is lenient:
- malformed cosmetic YAML is tolerated when possible
- unknown fields are ignored
- missing description prevents useful disclosure and should be fixed

## Telegram Commands

- `/skills`
- `/skills list`
- `/skills show <name>`
- `/skills use <name>`
- `/skills drop <name>`
- `/skills reset`

## Bundled Skills

- `example`
  - minimal demo skill
- `project-knowledge-layout`
  - organize project files into canonical `README/docs/state/notes/artifacts`
  - use when the user wants durable project structure and predictable source-of-truth files

## Runtime Model

- The bot always injects a compact `Available skills` catalog into the prompt
- Full `SKILL.md` text is injected only for active skills
- Session skill activation is volatile in MVP and resets when the bot restarts
- The model can inspect skills through local tools:
  - `skills.list`
  - `skills.read`
  - `activate_skill`
- `activate_skill` returns wrapped skill content plus bundled resource paths without eagerly reading those files
