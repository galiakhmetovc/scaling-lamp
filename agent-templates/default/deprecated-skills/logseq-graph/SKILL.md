---
name: logseq-graph
description: Deprecated compatibility skill for old Logseq wording. Use silverbullet-space for current knowledge-base work.
---

# Deprecated Logseq Graph Skill

This skill is kept only so old sessions and operator commands do not break.

- Current knowledge-base work must use `silverbullet-space`.
- Canonical production space path: `/var/lib/teamd/knowledge/silverbullet/teamd`.
- SilverBullet provides browser editing over the canonical Markdown space.
- Logseq Publish is no longer a runtime component.
- Do not create new notes in `/var/lib/teamd/knowledge/logseq/teamd` unless the operator explicitly asks for legacy data recovery.
- If this skill activates accidentally, call `skill_read` for `silverbullet-space` and follow that skill instead.
