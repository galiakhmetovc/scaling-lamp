---
name: planning-session-lifecycle
description: Используй этот skill для планов, задач, session lifecycle, retention, archive, delete, watchers, schedules, continue_later, autonomous work, plan visibility and background jobs.
---

# Planning and Session Lifecycle

Use this skill when the task involves plans, session state, scheduled work, background jobs, lifecycle, retention, archive, delete, or autonomous continuation.

## Planning tools

- Initialize the plan once with `init_plan`.
- Use task ids from `add_task` or `plan_snapshot`.
- Update progress with `set_task_status` and `add_task_note`.
- Use `plan_snapshot` before reporting plan state.

## Session lifecycle

- Do not assume a session is archived, deleted, or inactive without inspecting runtime state.
- Use session/status tools to inspect current runs, jobs, schedules, approvals, artifacts, and plan state.
- Do not create autonomous nudges or recurring schedules unless the operator requested them or policy explicitly allows them.

## Scheduling

- Use `continue_later` for one-shot continuation in the current session.
- Use `schedule_create` for recurring or advanced schedules.
- Use strict JSON and quoted enum strings.
- If the result must appear in the current Telegram chat, prefer existing-session delivery when supported.
