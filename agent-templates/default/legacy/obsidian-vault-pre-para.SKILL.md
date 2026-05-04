---
name: obsidian-vault
description: Use when working with Obsidian, vault, notes, knowledge base, Markdown notes, daily notes, tasks, links, frontmatter, or Telegram-sourced knowledge capture.
---

# Obsidian Vault

Use this skill for Obsidian knowledge-base work.

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

## Note workflow

1. Search before creating a new note unless the user asks for a clearly new note.
2. Read an existing note before editing it.
3. Preserve frontmatter, Obsidian links, headings, tasks, and existing folder structure.
4. Write concise Markdown with stable headings and meaningful filenames.
5. After a write/update tool succeeds, summarize exactly what changed and where.
6. If the tool fails, report the failure and retry with corrected arguments; do not claim the note was saved.

## Suggested folders

- `00-Inbox` for quick captures.
- `01-Projects` for project-specific notes.
- `02-Areas` for ongoing areas of responsibility.
- `03-Resources` for reference material.
- `05-Journal` for dated notes.
- `06-Tasks` for task lists.
- `templates` for reusable note templates.
