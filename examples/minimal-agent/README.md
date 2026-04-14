# Minimal Agent Example

This is the smallest useful agent shape in this repository.

Files:

- `main.go` — tiny transport loop over stdin/stdout
- `provider.go` — provider contract and a fake provider
- `tools.go` — tool contract and one tool
- `memory.go` — session history and the agent loop

Run it:

```bash
cd examples/minimal-agent
go run .
```

Try:

- `hello`
- `what time is it?`

How it maps to the real project:

- `main.go` -> `cmd/coordinator/main.go`
- `provider.go` -> `internal/provider/*`
- `memory.go` -> `internal/runtime/conversation_engine.go`
- `tools.go` -> `internal/mcp/*` and `internal/transport/telegram/provider_tools.go`
