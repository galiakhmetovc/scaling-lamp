# Clean-Room CLI Chat

Entry points:

```bash
./agent --config ./config/zai-smoke/agent.yaml --chat
./agent --config ./config/zai-smoke/agent.yaml --chat --resume <session-id>
```

Current behavior:

- `--chat` starts a new session by default
- `--resume <session-id>` resumes an existing session
- chat behavior is controlled by `ChatContract`
- input is multiline
- send happens on double `Enter`
- assistant output is streamed to stdout
- slash commands:
  - `/help`
  - `/session`
  - `/exit`

Current `zai-smoke` chat strategies:

- `ChatInputPolicy.multiline_buffer`
- `ChatSubmitPolicy.double_enter`
- `ChatOutputPolicy.streaming_text`
- `ChatStatusPolicy.inline_terminal`
- `ChatCommandPolicy.slash_commands`
- `ChatResumePolicy.explicit_resume_only`

Current runtime behavior per turn:

1. record user message in session event stream
2. record run start
3. execute provider client with streaming enabled
4. write streamed deltas to stdout
5. record transport attempt events
6. record assistant message in session event stream
7. record run completion
