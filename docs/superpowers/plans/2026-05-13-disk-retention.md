# План: disk retention и maintenance pruning

## Цель

Сделать явный операторский контур для диска TeamD:

- показать размер runtime-данных по категориям;
- показать, что именно будет удалено при очистке;
- по умолчанию выполнять только dry-run;
- удалять только явно сгенерированные TeamD-данные, не трогая проекты и пользовательские workspace-файлы.

## Контракт CLI

- `agentd disk usage`
- `agentd disk prune`
- `agentd disk prune --execute`
- русские алиасы: `диск использование`, `диск очистка`

Через `teamdctl` команды должны идти в daemon, чтобы оператору не приходилось вручную подставлять окружение systemd.

## Категории

- `artifacts`: `data_dir/artifacts`
- `transcripts`: `data_dir/transcripts`
- `archives`: `data_dir/archives`
- `runs`: `data_dir/runs`
- `agents`: `data_dir/agents`
- `audit`: `data_dir/audit/runtime.jsonl`
- `debug-bundles`: `data_dir/audit/debug-bundles`
- `legacy-sqlite`: `data_dir/state.sqlite*`
- `workspaces-trash`: `.trash` внутри agent workspaces
- `workspaces-scratch`: `scratch` внутри agent workspaces
- `deploy-backups`: путь из config
- `diagnostics`: путь из config

## Политика безопасности

- dry-run по умолчанию;
- фактическое удаление только с `--execute`;
- проекты и обычные workspace-файлы не удаляются автоматически;
- внешние пути для deploy backups и diagnostics должны быть явными в config.

## Проверка

- unit tests: парсинг CLI, dry-run не удаляет файл, execute удаляет legacy sqlite при retention `0`;
- `cargo fmt --all`;
- targeted tests;
- перед финалом полный набор verification по возможности.
