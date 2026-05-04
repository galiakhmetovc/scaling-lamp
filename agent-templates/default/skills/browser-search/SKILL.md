---
name: browser-search
description: Используй этот skill для веб-поиска, web_search, web_fetch, browser automation, Browserless, agent-browser, screenshots, dynamic pages, JavaScript pages, current information, research and online sources.
---

# Browser and Search

Use this skill when current external information or a real browser is needed.

## Tool choice

- Use `web_search` first for discovery, current facts, news, product data, laws, weather, and unknown URLs.
- Use `web_fetch` for an exact URL supplied by the user or returned by search.
- Use browser tools for JavaScript-heavy pages, forms, clicks, screenshots, PDFs, and dynamic content that `web_fetch` cannot read.

## Safety

- Prefer primary sources.
- Do not claim a web or browser result unless the tool succeeded.
- Keep fetched content bounded and use artifacts for large outputs.
- Respect access controls and do not perform abusive scraping.
