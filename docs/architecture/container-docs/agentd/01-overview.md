# agentd

Связанные views: `Containers`, `Deployment`.

Связанный C4-элемент: `agentd`.

`agentd` — daemon/runtime process внутри execution node.

## Ответственность

- HTTP API для клиентов;
- canonical runtime operations;
- prompt assembly;
- provider loop;
- structured tool execution;
- approvals;
- schedules и wake-up;
- inter-agent routing;
- persistence coordination;
- MCP discovery/invocation.

## В deployment

Каждый execution node содержит один `agentd` instance. Несколько instances могут образовывать mesh для remote delegation, inter-agent routing и future A2A.
