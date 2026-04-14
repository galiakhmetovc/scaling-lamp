# Clean-Room CLI Chat

Entry points:

```bash
./agent --config ./config/zai-smoke/agent.yaml --chat
./agent --config ./config/zai-smoke/agent.yaml --chat --resume <session-id>
```

Current behavior:

- real TTY input runs the workspace TUI in `internal/runtime/tui`
- non-interactive stdin falls back to the older line-based chat loop in `internal/runtime/cli`
- `--chat` starts a new session by default
- `--resume <session-id>` resumes an existing session
- chat behavior is still controlled by `ChatContract`

Current `zai-smoke` chat strategies:

- `ChatInputPolicy.multiline_buffer`
- `ChatSubmitPolicy.double_enter`
- `ChatOutputPolicy.streaming_text`
- `ChatStatusPolicy.inline_terminal`
- `ChatCommandPolicy.slash_commands`
- `ChatResumePolicy.explicit_resume_only`

Current `zai-smoke` chat params:

- `ChatInputPolicy`
  - `primary_prompt = "> "`
  - `continuation_prompt = ". "`
- `ChatSubmitPolicy`
  - `empty_line_threshold = 1`
- `ChatOutputPolicy`
  - `show_final_newline = true`
  - `render_markdown = true`
  - `markdown_style = dark`
- `ChatStatusPolicy`
  - `show_header = true`
  - `show_usage = true`
  - `show_tool_calls = true`
  - `show_tool_results = true`
  - `show_plan_after_plan_tools = true`
- `ChatCommandPolicy`
  - `exit_command = /exit`
  - `help_command = /help`
  - `session_command = /session`
- `ChatResumePolicy`
  - `require_explicit_id = true`

Current line-based fallback behavior per turn:

1. record user message in session event stream
2. record run start
3. execute provider client with streaming enabled
4. write streamed deltas to stdout
5. record transport attempt events
6. when provider emits tool calls, render short `[tool]` blocks in the terminal
7. when a plan tool mutates active plan state, render compact `[plan]` projection output in the terminal
8. record assistant message in session event stream
9. record run completion

Current terminal observability behavior:

- full request bodies and full tool payloads stay in `events.jsonl`
- terminal output only shows short operator-facing summaries
- short tool summaries are rendered from `ToolActivity` callbacks in the CLI layer
- plan rendering is sourced from `PlanHeadProjection`, not from ad hoc string assembly in chat runtime
- active plan rendering is session-scoped and follows the current chat session, not a global workspace plan
- when `render_markdown = true`, assistant text is buffered until the turn completes and then post-rendered through a terminal markdown renderer
- tool/status/plan lanes remain append-only and are not mixed into markdown rendering

Current tool activity events:

- `tool.call.started`
- `tool.call.completed`

Current resume read model:

- `TranscriptProjection` is the primary source for chat history reconstruction
- raw session-event replay remains only as a fallback path
