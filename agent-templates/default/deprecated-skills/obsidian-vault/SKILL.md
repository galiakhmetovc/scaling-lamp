---
name: obsidian-vault
description: Deprecated compatibility skill for old Obsidian/vault wording. Use silverbullet-space for current knowledge-base work.
---

# Deprecated Obsidian Vault Skill

This skill is kept only so old sessions and operator commands do not break.

- Current knowledge-base work must use `silverbullet-space`.
- Canonical production space path: `/var/lib/teamd/knowledge/silverbullet/teamd`.
- SilverBullet provides browser editing over the canonical Markdown space.
- Do not create new notes in `/var/lib/teamd/vaults/teamd`, `/var/lib/teamd/vault`, `~/vault`, or `/root/vault` unless the operator explicitly asks for legacy Obsidian recovery.
- If this skill activates accidentally, call `skill_read` for `silverbullet-space` and follow that skill instead.
