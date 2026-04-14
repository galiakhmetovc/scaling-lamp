# Plan: Worker Approval Control And Runtime Decoupling

## Task 1. Worker approval events

- add `worker.approval_requested`
- include `approval_id`, `tool`, `reason`, `run_id`
- cover via worker/runtime tests

## Task 2. CLI operator visibility

- render worker approval requests in `teamd-agent chat`
- extend `/status` to show:
  - active run
  - pending approvals
  - active workers
  - active jobs

## Task 3. Runtime prompt context

- move prompt-context assembly out of Telegram transport
- keep the same resulting prompt behavior
- adapt execution hooks to use runtime-owned injection

## Task 4. Telegram thinning

- switch Telegram to runtime-owned prompt/control helpers
- keep Telegram-only rendering and callback handling local to transport

## Task 5. Verification

- targeted tests first
- full `go test ./...`
- rebuild live binaries
- live smoke:
  - worker approval in CLI chat
  - `/status` while worker waits approval
  - normal Telegram path still works
