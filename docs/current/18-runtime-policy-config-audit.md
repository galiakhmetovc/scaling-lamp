# Runtime Policy Config Audit

Этот документ фиксирует, какие runtime decisions должны жить в `config.toml`, а не быть скрытыми `const`/`if` в коде. Цель простая: оператор должен видеть и менять policy без пересборки `agentd`.

## Уже вынесено в конфиг

### `[telegram]`

Telegram worker больше не держит эти значения только в коде:

- `inbound_min_coalesce_window_ms`
- `message_text_soft_cap`
- `caption_soft_cap`
- `status_detail_char_cap`
- `status_ttl_seconds`
- `typing_initial_delay_ms`
- `typing_heartbeat_interval_seconds`
- `delivery_retry_attempts`
- `delivery_retry_base_delay_ms`
- `chat_turn_fast_settle_ms`

Практический эффект:

- ошибки вида `MESSAGE_TOO_LONG` чинятся изменением soft cap в `config.toml`;
- retry/typing/status lifecycle Telegram настраивается оператором;
- `/queue coalesce ...` парсит пользовательское значение, а min clamp берётся из конфига.

### `[runtime_limits]`

В `runtime_limits` добавлены policy для tool/runtime path:

- `sqlite_lock_retry_attempts`
- `fs_list_default_limit`
- `fs_list_max_limit`
- `process_output_read_default_max_bytes`
- `process_output_read_max_bytes`
- `process_output_read_default_max_lines`
- `process_output_read_max_lines`
- `process_wait_default_timeout_ms`
- `process_wait_max_timeout_ms`
- `process_wait_poll_interval_ms`
- `process_terminate_grace_ms`
- `process_reader_drain_grace_ms`
- `provider_loop_max_transient_retries`
- `provider_loop_max_identical_tool_call_repeats`
- `provider_loop_max_empty_response_recoveries`
- `tool_result_preview_char_limit`
- `offload_max_context_refs`
- `offload_inline_tool_output_token_limit`
- `offload_inline_find_in_files_preview_limit`
- `artifact_read_default_max_bytes`
- `artifact_read_max_bytes`
- `kv_list_default_limit`
- `kv_list_max_limit`
- `kv_key_max_bytes`
- `kv_value_max_bytes`
- `kv_metadata_max_bytes`
- `skill_list_default_limit`
- `skill_list_max_limit`
- `skill_read_default_max_bytes`
- `skill_read_max_bytes`
- `autonomy_state_default_max_items`
- `autonomy_state_max_items`
- `prompt_recent_filesystem_activity_limit`
- `prompt_recent_process_activity_limit`
- `prompt_workspace_tree_limit`
- `interagent_default_max_hops`

Эти значения применяются в canonical runtime path:

- SQLite lock retry wrapper вокруг runtime store операций;
- `ToolRuntime` для `fs_list`, `fs_glob`, `exec_read_output`, `exec_wait`, `exec_kill`;
- active process tail в session diagnostics;
- provider transient retry loop;
- tool-call ledger preview size before artifact offload;
- context offload и `artifact_read`;
- `kv_*` limits;
- `skill_list`/`skill_read`;
- `autonomy_state_read`;
- `SessionHead` recent activity и workspace tree;
- новые root inter-agent chains.

### `[runtime_timing]` и `[daemon]`

Background worker lease больше не держит policy только в `background.rs`:

- `runtime_timing.sqlite_lock_retry_delay_ms`
- `runtime_timing.daemon_background_worker_lease_seconds`
- `daemon.worker_lease_owner`

Практический эффект: длительность lease и owner, который пишет daemon в job records, меняются через конфиг.

### `[knowledge]`

`knowledge_*` roots/extensions и SilverBullet mirror policy вынесены в конфиг:

- `source_files`
- `source_dirs`
- `allowed_extensions`
- `silverbullet_session_area_path`
- `silverbullet_text_artifact_extensions`
- `silverbullet_script_artifact_extensions`

Практический эффект:

- можно менять, какие workspace файлы индексируются через `knowledge_search`/`knowledge_read`;
- `knowledge_search` остаётся bounded SQLite FTS index по configured roots, а не произвольным filesystem search; unreadable/stale/non-UTF8 files пропускаются при индексации;
- можно менять, куда SilverBullet пишет index page зеркал сессий;
- можно менять, какие artifact extensions inline'ятся в mirror pages как text/script.

## Что ещё нельзя считать завершённым

### Tool surface policy

Остаётся hardcoded в `crates/agent-runtime/src/tool/catalog.rs`:

- список model-facing tools;
- `read_only`;
- `destructive`;
- `requires_approval`.

Нужный следующий шаг: добавить config layer вроде `[tools]` с `enabled`, `disabled`, `policy_overrides`, чтобы оператор мог убрать legacy tools или изменить approval policy без изменения Rust-кода.

### Built-in templates and skills

Часть bootstrap source всё ещё собирается из bundled templates в `cmd/agentd/src/agents.rs`. Runtime уже материализует profiles/skills на диск, но source-of-truth для встроенного набора ещё не полностью external-first.

Нужный следующий шаг: считать `/var/lib/teamd/state/agent-templates` или config-defined template dirs главным источником, а compiled-in templates оставить только recovery fallback.

### Prompt budget/offload refs internals

В `agent-runtime` остаются prompt/offload constants, например auto-pin threshold и часть selection caps. Их нельзя просто механически вынести: нужно сохранить prompt contract и бюджет по usable context tokens.

Нужный следующий шаг: сделать отдельную `[prompt_policy]` или расширить `[runtime_limits]` после согласования prompt layers.

## Правило для новых изменений

Нельзя добавлять новый hardcoded runtime policy в код без одного из вариантов:

- параметр уже есть в `config.toml`;
- добавлен новый config field с default, validation, test и документацией;
- это не policy, а protocol/domain constant, и это явно написано рядом с кодом.

Особенно запрещены скрытые safety filters в execution path. Если оператор хочет запретить класс команд, это должно быть явной `permissions`/policy-конфигурацией, а не `if command.contains(...)` в Rust-коде.
