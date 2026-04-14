# teamD Clean-Room Rewrite

This branch is the root of the new agent rewrite.

Rules:

- legacy code is reference only
- new runtime is event-sourced
- behavior is expressed through policies, strategies, resolved contracts, and executors
- configuration is explicit, modular, and rooted at one config file per agent instance

Current baseline documents:

- `docs/superpowers/specs/2026-04-14-context-policy-design.md`
- `docs/superpowers/plans/2026-04-14-context-policy-implementation.md`
