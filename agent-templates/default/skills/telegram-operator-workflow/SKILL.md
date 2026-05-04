---
name: telegram-operator-workflow
description: Используй этот skill для работы через Telegram: команды бота, session switching, status, stop, queue, файлы, документы, план, skills, operator workflow, Telegram commands and mobile chat operations.
---

# Telegram Operator Workflow

Use this skill when the operator is working from Telegram and asks about bot commands, sessions, status, files, plans, queues, or agent switching.

## Rules

- Prefer Telegram commands when the operator asks to inspect or control runtime state from the chat.
- Use concise replies; Telegram is the primary mobile surface.
- Do not ask the operator to use SSH, tunnels, or manual server commands unless there is no product command.
- If a command is missing, state the missing capability and suggest the closest existing command.

## Common commands

- `/status` for current session/runtime state.
- `/session` for session list and switching.
- `/skills` to inspect skills.
- `/enable <skill>` and `/disable <skill>` for session-scoped skill changes.
- `/stop` or `/cancel` for active work cancellation.
- `/queue` for inbound queue behavior.

## File handling

When the user sends a Telegram document, it should become a session-scoped artifact or approved workspace file. Confirm filename, size, and how the agent can reference it.
