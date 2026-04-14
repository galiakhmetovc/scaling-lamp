# 2026-04-12 — Mesh Prep Through Unified Delegation

Решение: mesh не строить как второй execution runtime рядом с AgentCore.

Правильная форма:
- local runtime остаётся canonical execution engine
- local workers становятся canonical local delegation backend
- future mesh peers становятся remote delegation backend
- UX, approvals, policy, handoff, artifacts, replay и operator visibility должны быть общими

Epic:
- `teamD-c0q` — Unify local and remote delegation model before mesh

Children:
1. `teamD-c0q.1` — Delegation contract for local and remote delegates
2. `teamD-c0q.2` — Align local workers to delegation contract
3. `teamD-c0q.3` — Shared policy and approval propagation for delegates
4. `teamD-c0q.4` — Remote mesh backend adapter
5. `teamD-c0q.5` — Operator visibility for remote delegates
6. `teamD-c0q.6` — Delegation scheduling and fallback policy

Принцип:
- не включать `internal/mesh` как parallel hot path
- сначала shared delegation contract
- потом mesh как backend adapter поверх этого контракта
