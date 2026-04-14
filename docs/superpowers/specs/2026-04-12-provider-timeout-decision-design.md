# Provider Timeout Decision Design

## Goal

When an LLM round hits `provider-timeout`, the run must stop failing immediately. Instead, the runtime should surface a pending operator decision, automatically continue the run once after 5 minutes of no response, and only fail if a second timeout happens without operator intervention.

## Current Problem

- `ExecuteConversation(...)` returns `llm round timed out after ...`
- Telegram marks the run as failed immediately
- The operator sees a timeout after the fact and cannot steer the same run
- Long runs die even when “wait longer and continue” is the correct next step

## Desired Runtime Behavior

### First provider timeout

1. The run transitions from `running` to `waiting_operator`.
2. Runtime persists a `timeout decision` record tied to the run.
3. Runtime emits a `run.provider_timeout` event.
4. Operator surfaces show actions:
   - `continue`
   - `retry_round`
   - `cancel`
   - `fail`
5. If no operator response arrives for 5 minutes, runtime automatically performs `continue` once.

### Second provider timeout for the same run

1. Runtime surfaces the same decision again.
2. Auto-continue does not happen a second time.
3. If no operator response arrives for 5 minutes, runtime marks the run as failed with a timeout-specific reason.

## Scope

### In scope

- generic persisted timeout-decision state in runtime store
- one-shot auto-continue after 5 minutes
- runtime resume path for `continue`
- Telegram operator actions for timeout decisions
- live status/state updates
- tests for first timeout, auto-continue, second-timeout failure path

### Out of scope

- arbitrary retry backoff strategies
- provider-specific transport changes
- changing the configured provider timeout value
- full TUI implementation

## Data Model

Add a persisted timeout decision record with:

- `run_id`
- `chat_id`
- `session_id`
- `status`: `pending | continued | retried | cancelled | failed | expired`
- `failure_reason`
- `requested_at`
- `resolved_at`
- `auto_continue_deadline`
- `auto_continue_used`
- `round_index`

Only one active timeout decision may exist per run.

## Control Semantics

### `continue`

- keep the run alive
- re-enter provider wait with a fresh timeout window
- do not re-run completed tool calls

### `retry_round`

- retry the same provider round once from the current prompt state
- no tool replay

### `cancel`

- cancel the run

### `fail`

- mark the run failed with timeout reason

## Transport Behavior

Telegram remains a renderer and action source:

- status card shows `waiting_operator`
- timeout decision is visible in chat/status
- callback buttons invoke generic runtime timeout-decision actions

The actual timeout state and auto-continue timer live in runtime, not Telegram.

## Testing

- first timeout creates pending decision instead of failing the run
- pending decision auto-continues after 5 minutes
- second timeout does not auto-continue again
- second unattended timeout fails the run
- Telegram callback path resolves the pending timeout decision
