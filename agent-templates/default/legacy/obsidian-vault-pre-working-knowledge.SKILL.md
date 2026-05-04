---
name: obsidian-vault
description: Use when working with Obsidian, vault, PARA, projects, areas, resources, archive, notes, knowledge base, Markdown notes, daily notes, tasks, links, frontmatter, or Telegram-sourced knowledge capture.
---

# Obsidian Vault

Use this skill for Obsidian knowledge-base and personal knowledge management work.

## Primary integration

- Use the `obsidian` MCP connector first.
- Discover available MCP resources/tools with `mcp_search_resources` when unsure.
- Prefer exposed Obsidian MCP tools such as `mcp__obsidian__read_note`, `mcp__obsidian__write_note`, `mcp__obsidian__search_notes`, or equivalent discovered names.
- Do not use generic filesystem write tools for normal note work.
- Use direct filesystem writes only as an emergency/admin fallback when MCP is unavailable and the user explicitly accepts that fallback.

## Vault contract

- Canonical production vault path: `/var/lib/teamd/vaults/teamd`.
- Compatibility path: `/var/lib/teamd/vault`; it must remain a symlink to the canonical vault.
- Do not create a second vault at `~/vault`, `/root/vault`, or another ad-hoc path.

## PARA structure

Use PARA as the default organization model:

- `00-Inbox` — raw captures, quick ideas, unsorted Telegram notes, temporary input.
- `01-Projects` — active outcomes with deadlines or clear finish conditions.
- `02-Areas` — ongoing responsibilities without an end date.
- `03-Resources` — reusable reference material, research, guides, snippets, domain notes.
- `04-Archive` — inactive projects, old resources, completed or deprecated material.
- `05-Journal` — dated daily notes, reviews, logs, and timeline entries.
- `06-Tasks` — task notes when a task needs its own page.
- `attachments` — files embedded or linked from notes.
- `templates` — reusable note templates.

Daily notes should normally be `05-Journal/YYYY-MM-DD.md`. Do not create a separate `daily/` tree unless it already exists or the user asks for it.

## Note workflow

1. Search before creating a new note unless the user asks for a clearly new note.
2. Read an existing note before editing it.
3. Preserve frontmatter, Obsidian links, headings, tasks, and existing folder structure.
4. Write concise Markdown with stable headings and meaningful filenames.
5. After a write/update tool succeeds, summarize exactly what changed and where.
6. If the tool fails, report the failure and retry with corrected arguments; do not claim the note was saved.

## Common operations

- Capture an idea: append/create a short note in `00-Inbox` with source and timestamp.
- Create a task: create or update a note in `06-Tasks`, with checkboxes and priority.
- Start a project: create `01-Projects/<project-name>.md` with goal, status, next actions, resources, and open questions.
- Add a resource: create `03-Resources/<topic>.md` with summary, source links, and related notes.
- Add a daily entry: update `05-Journal/YYYY-MM-DD.md`.
- Process inbox: move or rewrite inbox items into Projects, Areas, Resources, Archive, or Tasks.
- Complete work: update status, add result, then move inactive project notes to `04-Archive` only when the user agrees or completion is explicit.
- Search: search existing notes before duplicating concepts.

## Templates

When creating new notes, use lightweight frontmatter when useful:

```markdown
---
type: project|area|resource|task|daily|note
status: active|waiting|done|archived
created: YYYY-MM-DD
updated: YYYY-MM-DD
tags: []
---
```

Project notes should include: goal, status, next actions, decisions, resources, log.
Task notes should include: priority, status, checklist, context, result.
Daily notes should include: date, focus, log, tasks, captures.
Resource notes should include: summary, key points, sources, related notes.

## Tags and Obsidian syntax

- Use tags sparingly: `#project`, `#area`, `#resource`, `#task`, `#daily`, `#inbox`, `#archive`.
- Priority tags: `#p0`, `#p1`, `#p2`, `#p3` only when priority matters.
- Prefer wikilinks like `[[note name]]` for internal relationships.
- Use checkboxes `- [ ]` and `- [x]` for task lists.
- Use callouts for important blocks: `> [!note]`, `> [!warning]`, `> [!decision]`.
- Preserve existing embeds `![[...]]`, links, aliases, headings, and frontmatter.

## Operating rules

- Never delete or archive user material unless the user asked for it or the note clearly says it is ready to archive.
- Do not invent completed tasks, sources, dates, or decisions.
- If the target folder or naming convention is ambiguous, choose the closest PARA folder and state the assumption.
- Keep note names stable and readable; avoid timestamp-only filenames except daily notes.
- If a user message contains a durable fact, decision, task, or resource, offer to save it or save it directly when the request implies persistence.
