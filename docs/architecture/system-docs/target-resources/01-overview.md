# Target Resources

Связанный view: `SystemContext`.

Связанный C4-элемент: `Target Resources`.

`Target Resources` — ресурсы, на которые agentd или MCP tools могут воздействовать.

## Примеры

- workspace files;
- filesystem;
- OS processes;
- Git repositories;
- HTTP APIs;
- cloud resources;
- databases;
- Kubernetes;
- VMs и infrastructure.

## Как agentd воздействует на resources

Есть два пути:

- напрямую через built-in tools;
- через `MCP Capability Providers`, которые предоставляют tools/resources/prompts.

## Важное правило

`Target Resources` не являются частью execution mesh. Они могут быть локальными для execution node или внешними.
