---
name: lightpanda-browser
description: Deprecated compatibility skill for old Lightpanda MCP wording. Use agent-browser for current browser automation.
---

# Deprecated Lightpanda Browser Skill

This skill is kept only so old sessions and operator commands do not break.

- Current browser automation must use `agent-browser`.
- Use built-in tools such as `browser_open`, `browser_snapshot`, `browser_click`, `browser_fill`, `browser_text`, `browser_screenshot`, and `browser_pdf`.
- Do not look for `mcp__lightpanda__*` tools unless the operator explicitly asks to inspect legacy Lightpanda configuration.
- If this skill activates accidentally, call `skill_read` for `agent-browser` and follow that skill instead.
