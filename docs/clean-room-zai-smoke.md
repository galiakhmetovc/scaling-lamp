# Clean-Room z.ai Smoke

This document describes the first runnable `z.ai` smoke path in `rewrite/clean-room-root`.

## What Exists Now

- CLI smoke entrypoint:
  - `./agent --config ./config/zai-smoke/agent.yaml --smoke "ping"`
- runtime smoke seam:
  - `internal/runtime/smoke.go`
- explicit modular config graph:
  - `config/zai-smoke/agent.yaml`
  - `config/zai-smoke/contracts/...`
  - `config/zai-smoke/policies/...`

## Current z.ai Baseline

- base URL:
  - `https://api.z.ai/api/coding/paas/v4`
- path:
  - `/chat/completions`
- auth env var:
  - `TEAMD_ZAI_API_KEY`
- model:
  - `glm-5-turbo`

## What The Smoke Path Does

1. build the agent from `config/zai-smoke/agent.yaml`
2. create a smoke session if one does not already exist
3. record run start event
4. send one user message through prompt assets, request-shape, transport, and provider parsing
5. record run completion or failure event
6. print the assistant text to stdout

## Current Operational Blocker

The code path is ready, but the first live request still requires:

- `TEAMD_ZAI_API_KEY` in the process environment

Without that env var, transport auth fails before the request is sent.
