# Bootstrap And Config

## Entry Point

Запуск начинается в [cmd/coordinator/main.go](/home/admin/AI-AGENT/data/projects/teamD/cmd/coordinator/main.go).

Там сейчас только:

1. `config.LoadDotEnv(".env")`
2. `config.Load()`
3. `buildApp(...)`
4. optional Telegram polling
5. shutdown

Вся тяжёлая сборка зависимостей вынесена в [cmd/coordinator/bootstrap.go](/home/admin/AI-AGENT/data/projects/teamD/cmd/coordinator/bootstrap.go).

## Что делает `buildApp`

`buildApp(...)` собирает:

- provider
- skills catalog
- optional mesh runtime
- session store
- runtime store
- memory store
- Telegram adapter

Это главное место, где видно, какие подсистемы вообще существуют.

## Главные env-переменные

Смотри [internal/config/config.go](/home/admin/AI-AGENT/data/projects/teamD/internal/config/config.go).

Ключевые группы:

- Telegram
  - `TEAMD_TELEGRAM_TOKEN`
  - `TEAMD_TELEGRAM_BASE_URL`
- Provider
  - `TEAMD_ZAI_API_KEY`
  - `TEAMD_ZAI_BASE_URL`
  - `TEAMD_ZAI_MODEL`
  - `TEAMD_PROVIDER_ROUND_TIMEOUT`
- API / operator
  - `TEAMD_API_LISTEN_ADDR`
  - `TEAMD_API_BASE_URL`
  - `TEAMD_API_AUTH_TOKEN`
- Memory
  - `TEAMD_POSTGRES_DSN`
  - `TEAMD_OLLAMA_BASE_URL`
  - `TEAMD_MEMORY_EMBEDDINGS_ENABLED`
  - `TEAMD_MEMORY_EMBED_MODEL`
  - `TEAMD_MEMORY_EMBED_DIMS`
- Compaction
  - `TEAMD_CONTEXT_WINDOW_TOKENS`
  - `TEAMD_PROMPT_BUDGET_TOKENS`
  - `TEAMD_COMPACTION_TRIGGER_TOKENS`
  - `TEAMD_MAX_TOOL_CONTEXT_CHARS`
  - `TEAMD_LLM_COMPACTION_ENABLED`
  - `TEAMD_LLM_COMPACTION_TIMEOUT`
- Mesh
  - `TEAMD_MESH_ENABLED`
  - остальные `TEAMD_MESH_*`

## Важный принцип

Бот теперь считается **single-agent by default**.

Mesh создаётся только если:

- `TEAMD_MESH_ENABLED=true`
- и заданы остальные mesh-параметры.
