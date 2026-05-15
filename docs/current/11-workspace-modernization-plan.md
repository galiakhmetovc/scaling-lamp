# План модернизации workspace-модели

Этот документ начинался как целевой план. Итоговое решение уже принято и частично реализовано:

- встроенный `Agent template` только один: `default`;
- отдельного глобального skills storage для runtime activation нет;
- отдельного `agent_home` как смысловой сущности нет;
- каждый `Agent profile` имеет canonical workspace `workspaces/agents/<agent_id>/`;
- в этом workspace лежат `SYSTEM.md`, `AGENTS.md`, `skills/` и рабочая область tools для новых sessions;
- поле `agent_profiles.agent_home` оставлено как compatibility field и должно указывать на тот же workspace;
- legacy `data_dir/agents/<agent_id>/` при bootstrap копируется в workspace, если там ещё нет соответствующих файлов.

Ниже остаются исходная постановка проблемы и исторический план. Если старый текст противоречит списку выше, актуальным считается список выше.

## Проблема

Сейчас в `data_dir` есть каталог:

```text
/var/lib/teamd/state/agents/
├── default/
│   ├── AGENTS.md
│   ├── SYSTEM.md
│   └── skills/
└── judge/
    ├── AGENTS.md
    ├── SYSTEM.md
    └── skills/
```

По смыслу это `agent_home` для `Agent profile`: там лежат prompts и локальные skills. Но это легко спутать с рабочим каталогом проекта. Из-за этого возможна ошибка, когда агент стартует в `data_dir` или в `state`, хотя должен работать в project workspace.

Отдельная проблема: пока не хватает чёткой модели, где живут шаблоны агентов, где живут изменяемые профили, а где рабочие директории, на которые tools реально воздействуют.

## Термины

| Термин | Смысл |
| --- | --- |
| `Agent template` | Исходный шаблон агента: базовые `SYSTEM.md`, `AGENTS.md`, набор skills и allowed tools. |
| `Agent profile` | Созданный из template профиль агента. Имеет id, имя, `agent_home`, `default_workspace_root`, allowed tools и operator-visible metadata. |
| `agent_home` | Каталог профиля агента с prompts и skills. Это не project workspace. |
| `Workspace` | Рабочий каталог, где tools читают/пишут проектные файлы и запускают команды. |
| `Session workspace` | Workspace, закреплённый за session. Нужен, чтобы один и тот же диалог стабильно работал в одном проекте. |
| `Schedule workspace` | Workspace, из которого запускается scheduled/background работа. Сейчас у schedules уже есть `workspace_root`. |

## Строгая гигиена workspace

Workspace — это место работы над проектом, а не корзина для экспериментов runtime. Агент должен считать корень workspace чувствительной зоной:

- не создавать в корне временные файлы, скачанные страницы, одноразовые скрипты, логи экспериментов и диагностические дампы без прямого запроса оператора;
- использовать отдельный scratch path для временной работы и удалять его после завершения;
- складывать долговременные результаты в явные места: `docs/`, `artifacts/`, `diagnostics/`, SilverBullet Space, project-specific subdirectory или другой согласованный каталог;
- перед завершением работы проверять, какие файлы были созданы/изменены, и убирать случайный мусор;
- не использовать `agent_home`, `data_dir`, `state`, `audit`, `transcripts` или `artifacts` как рабочий каталог проекта.

Эти правила намеренно дублируются в built-in `SYSTEM.md`/`AGENTS.md`, потому что модель должна видеть их как поведенческий contract, а не только как operator documentation.

## Текущая модель

Сейчас:

- `agent_profiles.agent_home` указывает на `data_dir/agents/<agent_id>`;
- `agent_profiles.default_workspace_root` для built-in и newly-created profiles указывает на отдельный каталог `workspaces/agents/<agent_id>` рядом с `data_dir`;
- bootstrap создаёт builtin profiles `default` и `judge`;
- prompt assembly читает `SYSTEM.md`, `AGENTS.md` и skills из `agent_home`;
- schedules уже имеют поле `workspace_root`;
- обычные sessions уже имеют persisted `workspace_root` и получают его из выбранного `Agent profile`, если у профиля задан default workspace;
- execution path не должен создавать отдельный runtime для Telegram/TUI/CLI.

Вывод: `data_dir/agents/<agent_id>` сейчас ближе к profile home/template copy, чем к workspace.

## Целевая модель

Цель — разделить три слоя:

```text
agent templates -> agent profiles -> session/schedule workspaces
```

Предлагаемый production layout:

```text
/etc/teamd/
├── config.toml
└── teamd.env

/var/lib/teamd/state/
├── agent-profiles/
│   └── <agent_id>/
│       ├── SYSTEM.md
│       ├── AGENTS.md
│       └── skills/
├── PostgreSQL runtime store
└── ...

/var/lib/teamd/workspaces/
└── agents/
    └── <agent_id>/
```

Operator-editable templates уже материализуются в `data_dir/agent-templates`. Бинарь содержит только bundled fallback, чтобы первый запуск мог создать файлы. После bootstrap рабочий источник template prompts/skills — обычные Markdown-файлы:

```text
data_dir/agent-templates/
└── <template_id>/
    ├── SYSTEM.md
    ├── AGENTS.md
    └── skills/
```

На production это обычно:

```text
/var/lib/teamd/state/agent-templates/
├── default/
│   ├── SYSTEM.md
│   ├── AGENTS.md
│   └── skills/
└── judge/
    ├── SYSTEM.md
    └── AGENTS.md
```

Правка этих файлов не требует пересборки `agentd`. Уже созданный `agent_home` остаётся отдельным экземпляром: если оператор вручную изменил `data_dir/agents/<agent_id>/SYSTEM.md`, bootstrap не должен молча затирать эту правку.

## Как копировать prompts и skills

Рекомендуемая политика:

- при создании `Agent profile` копировать `SYSTEM.md` и `AGENTS.md` из `Agent template` в `agent_home`;
- при создании `Agent profile` создавать отдельный `default_workspace_root` в `workspaces/agents/<agent_id>`;
- skills тоже копировать в `agent_home/skills/`, если они являются частью поведения профиля;
- изменения template после создания профиля не должны молча менять уже существующий profile;
- если нужен общий skill catalog, он должен быть отдельным слоем и явно подключаться, а не подменять profile-local skills.

Причина: `Agent profile` должен быть воспроизводимым. Если template обновился, оператор должен явно решить, обновлять ли существующий profile.

## Что нужно добавить в runtime

1. Добавить persisted `default_workspace_root` к `agent_profiles`.
2. Добавить persisted `workspace_root` к `sessions`.
3. При создании session выбирать workspace в таком порядке:
   - явно переданный workspace;
   - selected/default workspace профиля;
   - безопасный configured default workspace;
   - ошибка, если workspace не определён.
4. Запретить использовать `data_dir`, `data_dir/state`, `data_dir/audit`, `data_dir/transcripts`, `data_dir/artifacts` как workspace.
5. В `ExecutionService` передавать workspace в `ToolRuntime` из persisted session context.
6. В Telegram-created sessions использовать тот же session workspace selection path.
7. В TUI/CLI показывать workspace в session detail.
8. Для schedules оставить `workspace_root`, но привести его к тем же validation rules.
9. Добавить миграцию: существующим sessions проставить текущий configured workspace или оставить `NULL` с явной ошибкой при следующем запуске tool’ов.
10. Добавить CLI/TUI команды для просмотра и смены workspace session, но не создавать отдельный execution path.

## Что не делать

- Не использовать `agent_home` как рабочий каталог проекта.
- Не заводить отдельный Telegram workspace path.
- Не заводить отдельный prompt assembly для конкретной поверхности.
- Не копировать runtime state в workspace.
- Не делать скрытый fallback в `/var/lib/teamd/state`, если workspace не задан.

## Минимальный план внедрения

1. Schema migration: добавить workspace fields.
2. Config: добавить operator-facing default workspace root.
3. Bootstrap: `data_dir/agent-templates` уже отделён от `data_dir/agents`; дальше можно добавить operator command для явного refresh конкретного `agent_home` из template.
4. Session create path: сохранять effective workspace.
5. Tool runtime: брать cwd из session workspace.
6. Surfaces: CLI/TUI/Telegram/HTTP показывают и меняют одно и то же поле session.
7. Tests: проверить, что tools не стартуют в `data_dir`, Telegram-created session получает workspace, schedule сохраняет свой workspace.

## Открытые решения

- Нужен ли физический rename `state/agents` -> `state/agent-profiles`, или достаточно документации и новых полей?
- Нужна ли команда `agent refresh-from-template <agent_id>` с diff/confirmation, чтобы явно обновлять agent instance из `data_dir/agent-templates`?
- Должны ли skills копироваться всегда, или часть skills должна подключаться как shared catalog?
- Нужен ли один workspace на agent profile или отдельный workspace на session по умолчанию?

Практичная рекомендация: хранить workspace на уровне session, а у agent profile держать только default. Это даёт стабильность диалога и не ломает сценарий, где один и тот же agent profile работает с несколькими проектами.
