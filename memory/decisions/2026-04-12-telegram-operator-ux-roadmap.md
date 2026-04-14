# 2026-04-12 — Telegram Operator UX Roadmap

Решение: Telegram развиваем как полноценный operator surface поверх существующего runtime/control plane, без Telegram-only semantics.

Epic:
- `teamD-uxw` — Build Telegram operator UX over teamD control plane

Фазы:
1. `teamD-uxw.1` — Telegram session UX v2
2. `teamD-uxw.2` — Telegram approval UX v2
3. `teamD-uxw.3` — Telegram timeout and error steering
4. `teamD-uxw.4` — Telegram worker and job operator surfaces
5. `teamD-uxw.5` — Telegram plan and artifact surfaces
6. `teamD-uxw.6` — Telegram status model v2
7. `teamD-uxw.7` — Telegram debug and inspect surface
8. `teamD-uxw.8` — Telegram policy actions

Приоритет для ежедневной работы:
- сначала `sessions`
- потом `approvals`
- потом `timeouts/errors`
- потом `workers/jobs`

Принцип:
- semantics живут в runtime/API/control plane
- Telegram остаётся renderer + input adapter + callback client
- без отдельной Telegram-логики, конкурирующей с core
