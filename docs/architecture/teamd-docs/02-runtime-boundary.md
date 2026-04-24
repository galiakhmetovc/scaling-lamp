# teamD Runtime: граница ответственности

Связанный C4-элемент: `teamD Runtime`.

Этот раздел отвечает на вопрос: что считается частью `teamD Runtime`, а что является внешней системой.

## Внутри `teamD Runtime`

На следующих C4-уровнях внутри системы должны быть раскрыты:

- `agentd` CLI и daemon process;
- canonical runtime path для chat turns и background jobs;
- persistence слой для SQLite и payload-файлов;
- provider integration для `LLM Provider`;
- tool execution и approval flow;
- schedule wake-up;
- inter-agent messaging;
- TUI, HTTP и Telegram surfaces как тонкие интерфейсы над runtime.

На view `Containers` эти части сейчас сгруппированы в три крупных C4 containers:

- `Operator Surfaces`;
- `App / Runtime Core`;
- `Runtime Store`.

## Снаружи `teamD Runtime`

Эти элементы не являются частью runtime:

- `Operator` — человек, который управляет системой.
- `LLM Provider` — внешний API модели.
- `Telegram Bot API` — внешний транспорт сообщений.
- `MCP Servers` — внешние или локальные серверы capability.
- `GitHub Releases` — источник опубликованных обновлений.
- `Local Host` — машина, OS, workspace и файловая среда, с которой runtime взаимодействует.

## Архитектурное правило

CLI, TUI, Telegram и HTTP не должны иметь отдельные chat loops или отдельную prompt assembly.

Они должны оставаться тонкими surfaces над одним runtime path.
