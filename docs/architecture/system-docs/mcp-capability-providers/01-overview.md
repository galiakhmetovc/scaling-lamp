# MCP Capability Providers

Связанный view: `SystemContext`.

Связанный C4-элемент: `MCP Capability Providers`.

`MCP Capability Providers` — поставщики capabilities для agentd: tools, resources и prompts.

## Типы MCP providers

- Internal MCP servers — запущены внутри execution node или управляются тем же окружением.
- External MCP servers — отдельные сервисы вне execution mesh.

## Что они делают

MCP providers дают agentd доступ к capabilities. Эти capabilities могут читать или изменять `Target Resources`.

## Почему это отдельная граница

MCP server может быть локальным, удалённым, временным или общим для нескольких execution nodes. Поэтому на System Context это capability boundary, а на Deployment view оно раскладывается на internal/external MCP.
