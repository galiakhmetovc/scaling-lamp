# Internal MCP Server

Связанные views: `Containers`, `Deployment`.

Связанный C4-элемент: `Internal MCP Server`.

`Internal MCP Server` — MCP server, запущенный внутри execution node или управляемый тем же окружением.

## Ответственность

- предоставляет локальные tools/resources/prompts;
- может работать с local `Target Resources`;
- вызывается `agentd` через MCP protocol.

## Чем отличается от external MCP

Internal MCP развёрнут рядом с `agentd`. External MCP живёт вне execution nodes и показан на deployment view как отдельный deployment node.
