---
name: silverbullet-space
description: Используй этот skill для работы с SilverBullet Space: заметки, база знаний, PARA, Zettelkasten, Markdown pages, wikilinks, inline tags, Telegram captures, project/resource/journal notes. Use when reading, creating, updating, searching, organizing, or remembering knowledge in SilverBullet.
---

# SilverBullet Space

Use this skill for SilverBullet knowledge-base and personal knowledge management work.

When this skill is active and the user asks to work with notes, knowledge, docs, PARA, projects, resources, daily notes, or SilverBullet, follow it directly. If it is not active, call `skill_read` for `silverbullet-space` before changing durable notes.

## Primary integration

- Canonical production space path: `/var/lib/teamd/knowledge/silverbullet/teamd`.
- SilverBullet is the browser UI over this Markdown space.
- If the `silverbullet` MCP connector is available, prefer it for note reads/searches/writes.
- Discover available MCP resources/tools with `mcp_search_resources` when unsure.
- If the MCP connector is unavailable, use canonical filesystem tools inside the space path only, and only after reading existing content first.
- Do not write knowledge notes into project roots, `/root`, `/var/lib/teamd/vault`, the legacy Logseq path, or old Obsidian paths.

## Source guides

The operator-facing source guides are inside the space:

- `[[r/silverbullet-instrukciya]]` / `https://teamd.qlbc.ru/sb/r/silverbullet-instrukciya` — practical SilverBullet workflow.
- `[[r/system-guide]]` / `https://teamd.qlbc.ru/sb/r/system-guide` — PARA + Zettelkasten rules.

If the structure, naming, or query behavior is unclear, read these notes before editing the space.

## Space contract

- Treat the space as the shared working knowledge layer for the agent and operator.
- Do not use the space as runtime state: transcripts, runs, tool calls, artifacts, schedules, approvals, audit logs, PostgreSQL control-plane state, and payload files remain in `agentd`.
- `agentd` may mirror runtime state into the space for transparency. Mirror pages are readable/editable notes for the operator, but edits do not mutate runtime state until a tool explicitly imports or applies them.
- Do not treat space notes as canonical repository documentation. Stable documentation still belongs in git under `docs/`; use space notes for working notes, drafts, decisions, research, and project logs before promoting stable material to repo docs.
- Future semantic search may index this space. Write notes so they are useful for humans and indexing: clear title, concise summary, stable headings, explicit links, and frontmatter when useful.
- Preserve Markdown frontmatter, wikilinks, inline `#tags`, headings, checkboxes, queries, and existing note structure.
- At the start of substantial SilverBullet work, tell the operator briefly what you are doing, for example: `Использую SilverBullet skill: ищу существующие заметки и обновлю память после записи.`

## Daily journal and TeamD mirrors

- The operator timezone is provided in `SessionHead`; use it when choosing `journals/YYYY-MM-DD.md`.
- Today's and yesterday's journal excerpts may be injected into `SessionHead`. Treat them as context only; read the target note before editing.
- Each substantial session should append important decisions, durable facts, completed work, blockers, and follow-up tasks to today's journal when persistence is implied.
- `a/teamd-agents.md` is the TeamD Agents area index for runtime mirrors.
- `p/teamd-session-<session_id>.md` pages are generated/readable session mirrors with plan snapshot, context summary, tool activity, and artifacts.
- Do not manually rewrite generated mirror sections unless the operator asks; add operator/agent notes in separate headings when useful.
- After writing durable SilverBullet content, use `memory_search` then `memory_add` or `memory_update` to maintain a short Mem0 pointer unless the note says `memory: false`.

## Current structure

The current space uses PARA + Zettelkasten.

Root container pages are catalogs and live lists. Do not store long content in them:

- `Projects.md` — active and completed projects via `#project`.
- `Areas.md` — ongoing areas via `#area`.
- `Resources.md` — references, guides, research, and literature notes via `#resource` and `#literature`.
- `Archive.md` — archived or inactive material via `#archive`.
- `00-Inbox.md` — quick captures, fleeting notes, Telegram input.
- `05-Journal.md` — daily notes via `#daily`.
- `06-Zettelkasten.md` — permanent notes via `#zettelkasten`.

Use one-level namespaces for actual notes:

- `p/<slug>.md` — projects: concrete outcome, deadline or finish condition.
- `a/<slug>.md` — areas: ongoing responsibility without an end date.
- `r/<slug>.md` — resources, guides, research, references, literature notes.
- `journals/YYYY-MM-DD.md` — daily notes.
- `template/<name>.md` — reusable templates.

Do not create new top-level folder systems such as `01-Projects/`, `02-Areas/`, `03-Resources/`, `04-Archive/`, `06-Tasks/`, `daily/`, `zettel/`, or nested paths like `p/project/backend/db`. Namespace depth is one level.

Existing legacy or miscellaneous root notes may remain. Do not move or rename them unless the operator asks.

## Tags and queries

- SilverBullet queries use inline `#tags` in the note body. YAML frontmatter `tags:` is useful for humans, but it is not enough for query visibility.
- Put the type tag near the top of the body, for example `**Тип:** #project`.
- Use tags sparingly: `#project`, `#area`, `#resource`, `#daily`, `#inbox`, `#fleeting`, `#zettelkasten`, `#evergreen`, `#literature`, `#archive`, `#done`.
- If unsure, use one type tag plus at most one topic/status tag.
- Container pages use SilverBullet v2 Space Lua / Lua Integrated Query blocks. Do not use legacy `[query: ...]`: SilverBullet v2 renders it as plain text.
- Prefer this container query shape:
  `${template.each(query[[
  from p = index.tag "project"
  where p.tag == "page" and p.name:startsWith("p/")
  order by p.name
  ]], templates.pageItem)}`
- Do not replace query blocks with static lists unless the operator asks.

## Note workflow

1. Search before creating a new note unless the user asks for a clearly new note.
2. Read existing notes before editing them.
3. Choose the correct namespace and a stable readable slug.
4. Preserve frontmatter, wikilinks, Space Lua query blocks, headings, tasks, and existing structure.
5. Add inline type tags so container queries can see the note.
6. Add at least one useful wikilink when natural: project -> area, resource -> project/area, zettel -> related idea.
7. Write concise Markdown with stable headings and meaningful filenames.
8. If a container has query blocks, inline tags are normally enough. Add manual links only to curated sections where useful, for example `Resources.md` -> `Инструкции`.
9. Verify after create/update with a search/read that the note exists and has the expected tag/path.
10. After a durable write/update tool succeeds, create or update a short Mem0 pointer memory unless the note has `memory: false`.
11. The pointer memory should include note path, title/topic, short summary, tags, and why the note matters; do not copy the whole note into Mem0.
12. Use `memory_search` before `memory_add` to avoid duplicate pointers when updating an existing note.
13. After a write/update succeeds, summarize exactly what changed and where, and mention whether a Mem0 pointer was saved or skipped.
14. If a tool fails, report the failure and retry with corrected arguments; do not claim the note was saved.

## Common operations

- Capture an idea: append to `00-Inbox.md` or create a fleeting note with inline `#inbox #fleeting`.
- Start a project: create `p/<slug>.md` with `#project`, goal, next actions, area link, resources, decisions, and log.
- Add an area: create `a/<slug>.md` with `#area`, scope, active projects, key notes, and maintenance rules.
- Add a resource or guide: create `r/<slug>.md` with `#resource`, summary, sources, related projects/areas, and key points.
- Add a daily entry: create/update `journals/YYYY-MM-DD.md` with `#daily`, focus, tasks, log, ideas, and links.
- Add a session work note: append to today's journal and, when relevant, link `[[p/teamd-session-<session_id>]]`.
- Create a permanent note: use a flat readable page or agreed namespace with `#zettelkasten`; keep it atomic and linked to related notes.
- Process inbox: turn captures into project actions, resources, permanent notes, archive items, or delete only when the operator agrees.
- Complete a project: add `#done` or `#archive` and update the result; do not silently delete completed material.
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

## Tags and Markdown syntax

- Prefer wikilinks like `[[note name]]` for internal relationships.
- Use checkboxes `- [ ]` and `- [x]` for task lists.
- Preserve existing links, aliases, headings, frontmatter, and properties.

## Operating rules

- Never delete or archive user material unless the user asked for it or the note clearly says it is ready to archive.
- Do not invent completed tasks, sources, dates, or decisions.
- If the target namespace or naming convention is ambiguous, choose the closest PARA namespace and state the assumption.
- Keep note names stable and readable; avoid timestamp-only filenames except daily notes.
- If a user message contains a durable fact, decision, task, or resource, offer to save it or save it directly when the request implies persistence.
- At the start of substantial work, search/read relevant project, area, or resource notes from the space.
- After an important decision or completed task, update the relevant project note or daily journal.
- When a working note becomes stable documentation, offer to promote it into repository docs and commit it.

## Common mistakes

- Do not create numbered PARA folders like `01-Projects/`; use `p/`, `a/`, `r/`, `journals/`, and root container pages.
- Do not rely only on YAML `tags:`; add inline `#tags` in the body.
- Do not store long content in `Projects.md`, `Areas.md`, `Resources.md`, `Archive.md`, `00-Inbox.md`, `05-Journal.md`, or `06-Zettelkasten.md`; those are navigation/query pages.
- Do not create deep folder hierarchies. Use links and tags for relationships.
- Do not claim memory was updated unless `memory_search`/`memory_add` actually succeeded.
