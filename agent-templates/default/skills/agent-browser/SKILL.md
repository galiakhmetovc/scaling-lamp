---
name: agent-browser
description: Use when a task needs a real JavaScript-capable browser through built-in browser_* tools backed by agent-browser and Browserless: dynamic pages, SPAs, forms, clicks, snapshots, screenshots, PDFs, or browser automation that web_fetch cannot do.
---

# Agent Browser

Use this skill when `web_search`/`web_fetch` are not enough and the task needs a real browser.

## Primary integration

- Browser automation is exposed as built-in `browser_*` tools in the canonical teamD tool loop.
- The runtime invokes the `agent-browser` CLI; production deployments should use Browserless as the browser backend.
- Do not call shell commands for browsing when `browser_*` tools are available.
- If browser tools are disabled or unavailable, say so explicitly and fall back to `web_search`/`web_fetch` only when that still satisfies the user.

## When to use browser tools

Use `browser_*` for:

- JavaScript-rendered pages, SPAs, infinite-load pages, and pages where static HTML is insufficient.
- Form filling, clicking, scrolling, and other interactive flows.
- Extracting text, snapshots, links, forms, and visible state from a live page.
- Following a search result through multiple pages when the user needs current content, not just a snippet.
- Debugging pages where `web_fetch` returns empty, incomplete, blocked, or script-heavy content.
- Screenshots and PDFs that need to be saved into the workspace.

Do not use browser tools for:

- Simple exact-URL reads where `web_fetch` returns enough readable content.
- Current information discovery before choosing a URL; use `web_search` first.
- High-frequency scraping, abusive automation, bypassing access controls, or ignoring robots/site policies.

## Typical workflow

1. Use `web_search` to discover candidate URLs when the user did not provide an exact URL.
2. Use `browser_open` for the chosen URL.
3. Use `browser_snapshot` to understand the page. Interactive refs like `@e1` are valid only for the current snapshot.
4. Use `browser_click`, `browser_fill`, `browser_press`, `browser_scroll`, or `browser_wait` for interaction.
5. After each page-changing action, call `browser_snapshot` again before using old interactive refs.
6. Use `browser_text`, `browser_eval`, `browser_screenshot`, or `browser_pdf` only when they match the task.
7. Summarize what was actually observed. Do not claim a browser action happened unless the tool succeeded.

## Operational notes

- Browserless sessions are isolated per teamD session.
- Large snapshots and text outputs are offloaded into artifacts; use `artifact_read` when you need the full payload later.
- Use workspace-relative paths for screenshots/PDFs, for example `scratch/browser/page.png`.
- Respect robots.txt and avoid high-frequency requests.
- For durable findings, save results through normal teamD surfaces: notes, docs, artifacts, or explicit files in the session workspace.
