# teamD Execution Mesh

Связанный view: `SystemContext`.

Связанный C4-элемент: `teamD Execution Mesh`.

`teamD Execution Mesh` — основная система. Это один или несколько execution nodes, где запущены `agentd` instances и выполняется агентская работа.

## Что делает mesh

- хранит sessions, runs, jobs, schedules и artifacts;
- исполняет chat turns и background jobs;
- собирает prompt и вызывает `LLM Provider APIs`;
- исполняет built-in tools;
- вызывает `MCP Capability Providers`;
- взаимодействует с `Target Resources`;
- поддерживает agentd-to-agentd mesh для delegation, inter-agent routing и будущего A2A.

## Что не входит в mesh

- `Operators` — люди или automation-участники.
- `agentd Clients` — CLI, TUI, HTTP clients и Telegram-mediated client flow.
- `LLM Provider APIs` — внешние API моделей.
- `External MCP Servers` — MCP providers вне execution nodes.
- `Target Resources` — ресурсы, на которые воздействуют tools.

## Как читать дальше

- View `SystemContext` показывает mesh как одну систему.
- View `Containers` показывает containers внутри mesh.
- View `Deployment` показывает execution nodes и agentd instances.
