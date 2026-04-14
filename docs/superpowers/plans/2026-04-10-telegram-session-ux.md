# Telegram Session UX Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the current command-heavy Telegram session management UX with a cleaner control surface: startup command sync, a persistent session entrypoint button, a compact session card, dynamic switch controls, and explicit reset confirmation.

**User Problem:** Named sessions now work technically, but the current interaction model is still awkward. Slash commands from earlier iterations linger in Telegram, the user has to remember exact commands, and the inline controls are too static to serve as a primary UX. The bot needs one obvious session entrypoint and one consistent session control panel.

**Scope:** This slice is UX/UI and Telegram control plumbing only. It does not add MCP session tools, subagent orchestration, or broader policy changes. Existing multi-session storage, tool-calling, and footer behavior remain in place and are reused.

**Tech Stack:** Go 1.25+, existing Telegram adapter, existing session store, Telegram Bot API methods `setMyCommands`, `deleteMyCommands`, `sendMessage`, `editMessageText`, `editMessageReplyMarkup`, reply keyboard markup, inline keyboard markup, callback query handling, Go tests.

---

## File Structure

- Modify: `cmd/coordinator/main.go`
  Purpose: sync Telegram command menu and keyboard behavior at startup.
- Modify: `internal/config/config.go`
  Purpose: add toggles/settings for Telegram UX defaults if needed.
- Modify: `internal/transport/telegram/adapter.go`
  Purpose: render session card, reply keyboard entrypoint, callback flows, and reset confirmation.
- Modify: `internal/transport/telegram/footer.go`
  Purpose: keep footer consistent with the higher-level session card.
- Modify: `internal/transport/telegram/adapter_test.go`
  Purpose: cover command sync, reply keyboard, session card, dynamic switching, and confirmation flow.
- Modify: `README.md`
  Purpose: document the new Telegram session UX.

---

## UX Model

### 1. Startup Command Sync

- On coordinator startup, sync Telegram bot commands so stale slash commands do not linger.
- Recommended minimal command menu:
  - `/session`
  - `/reset`
- Preferred behavior:
  - either call `deleteMyCommands` first, then `setMyCommands`
  - or overwrite with a canonical list in one place
  - avoid unnecessary repeated sync if the command list has not changed recently

### 2. Persistent Entrypoint

- The bot should expose a persistent reply keyboard button:
  - `Sessions`
- This is the main entrypoint for users who do not want to remember commands.

### 3. Session Card

- `/session` and the `Sessions` reply keyboard button should open one compact session card.
- The card should show:
  - active session
  - message count
  - total session count
  - last activity, if available

- Inline buttons under the card:
  - `Switch`
  - `New`
  - `Stats`
  - `Reset`

### 4. Dynamic Switch Menu

- `Switch` should show a dedicated menu listing available sessions.
- Each session gets its own button.
- The active one is explicitly marked, for example:
  - `• deploy (active)`
- Include a `Back` button.
- Respect Telegram `callback_data` size limits; do not place arbitrarily long session names directly into callback payloads without a bounded encoding strategy.

### 5. Create Flow

- `/session new <name>` and `создай сессию <name>` remain valid.
- Creating a session should always auto-activate it.
- The reply after creation should be the same session card UX, not a plain confirmation string.

### 6. Stats Panel

- `Stats` should show a short operational summary for the current session:
  - active session name
  - message count
  - tool call count
  - last activity
- Keep it short enough for Telegram, not a dump.

### 7. Reset Confirmation

- `Reset` should not immediately destroy the current session.
- It should open a confirmation prompt with:
  - `Confirm reset`
  - `Cancel`
- Confirmation state should be ephemeral and bounded in time for MVP.

### 8. Footer Role

- Footer stays as a low-level diagnostic indicator.
- The new session card becomes the primary UX.
- Do not duplicate too much text between footer and session card.

---

## Implementation Order

### Task 1: Telegram Command Sync

- [ ] Add startup command sync in `cmd/coordinator/main.go`
- [ ] Ensure stale command menus are replaced by the canonical list
- [ ] Make sync rate-limit-aware enough for repeated coordinator restarts
- [ ] Cover with an adapter/client test if practical

### Task 2: Reply Keyboard Entrypoint

- [ ] Add a persistent reply keyboard with `Sessions`
- [ ] Ensure the bot can re-send the keyboard when opening the session panel
- [ ] Add tests that confirm reply keyboard markup is attached

### Task 3: Session Card

- [ ] Add a reusable session summary formatter
- [ ] `/session` should send the session card instead of raw text
- [ ] `Sessions` reply keyboard input should open the same card
- [ ] Add tests for session card content

### Task 4: Dynamic Switch Menu

- [ ] Render dynamic inline buttons per session
- [ ] Mark the active session clearly
- [ ] Add `Back` navigation
- [ ] Keep callback payloads bounded and Telegram-safe
- [ ] Add callback tests for switching among multiple named sessions

### Task 5: Create Flow Polish

- [ ] Ensure `new` always auto-activates
- [ ] Route command-based and natural-language creation into the same card response
- [ ] Add tests that verify consistent output between entry paths

### Task 6: Stats Panel

- [ ] Add session stats formatter
- [ ] Expose it through the `Stats` callback
- [ ] Keep the output concise and Telegram-friendly

### Task 7: Reset Confirmation

- [ ] Add a reset confirmation callback flow
- [ ] Require explicit `Confirm reset`
- [ ] Store pending confirmation state in a small ephemeral in-memory map with timeout
- [ ] Add tests for confirm/cancel behavior

### Task 8: Documentation and Verification

- [ ] Update `README.md` with the new Telegram session UX
- [ ] Prefer updating an existing session card via `editMessageText` / `editMessageReplyMarkup` where practical, instead of spamming new messages
- [ ] Run `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./...`
- [ ] Manually verify in Telegram:
  - `Sessions` button appears
  - `/session` opens the same card
  - switching sessions works
  - reset requires confirmation
  - stale old commands no longer appear in the bot menu

---

## Non-Goals

- No MCP `session.*` tools in this slice
- No subagent spawning UX in Telegram
- No advanced fuzzy intent parsing beyond the current simple session phrases
- No pagination or search for very large session lists

---

## Follow-On Work

After this plan lands, likely next UX improvements are:
- dynamic `Use <session>` buttons everywhere a session list appears
- better formatting/truncation of tool output in Telegram
- policy-aware button visibility for destructive actions
