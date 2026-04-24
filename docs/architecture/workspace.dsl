workspace "teamD" "C4-модель архитектуры локального runtime для AI-агентов." {
    !identifiers hierarchical
    !docs docs

    model {
        operator = person "Operator" "Пользователь, разработчик или администратор: общается с агентами, читает результаты, подтверждает действия и управляет runtime."

        teamd = softwareSystem "teamD Runtime" "Локальная среда для AI-агентов общего назначения: каноническое выполнение, tools, schedules, memory, inter-agent workflows и интерфейсы оператора." {
            !docs teamd-docs
            surfaces = container "Operator Surfaces" "CLI, TUI, HTTP API и Telegram adapters. Тонкие интерфейсы над одним runtime path." "Rust binary/modules"
            runtime = container "App / Runtime Core" "Каноническое выполнение chat turns, prompt assembly, provider loop, tools, approvals, schedules и inter-agent routing." "Rust"
            store = container "Runtime Store" "Persistent state: sessions, transcripts, runs, jobs, plans, schedules, artifacts и audit trail." "SQLite + payload files"
            tags "System"
        }

        llmProvider = softwareSystem "LLM Provider" "Внешний API модели: принимает запросы provider и возвращает assistant text, reasoning и structured tool calls." {
            tags "External"
        }

        telegram = softwareSystem "Telegram Bot API" "Внешний API Telegram: доступ оператора к чату, pairing, commands и исходящие notifications." {
            tags "External"
        }

        mcpServers = softwareSystem "MCP Servers" "Внешние или локальные MCP-совместимые серверы: дополнительные tools, resources и prompts." {
            tags "External"
        }

        githubReleases = softwareSystem "GitHub Releases" "Источник release-артефактов для проверки и загрузки опубликованных бинарников agentd." {
            tags "External"
        }

        localHost = softwareSystem "Local Host" "Машина или сервер оператора: filesystem, процессы OS, terminal, workspace, SQLite database и payload-файлы." {
            tags "External"
        }

        operator -> teamd "Работает с агентами через CLI, TUI, Telegram и HTTP"
        operator -> teamd.surfaces "Работает через CLI, TUI, HTTP и Telegram"
        teamd.surfaces -> teamd.runtime "Вызывает canonical runtime operations"
        teamd.runtime -> teamd.store "Читает и пишет persistent state"
        teamd -> llmProvider "Отправляет provider requests и получает текст, reasoning и tool calls"
        teamd.runtime -> llmProvider "Отправляет provider requests"
        teamd -> telegram "Получает updates, регистрирует commands, отправляет replies и notifications"
        teamd.surfaces -> telegram "Получает updates и отправляет notifications"
        teamd -> mcpServers "Ищет и вызывает внешние возможности"
        teamd.runtime -> mcpServers "Ищет и вызывает tools/resources/prompts"
        teamd -> githubReleases "Проверяет и скачивает обновления runtime"
        teamd.runtime -> githubReleases "Проверяет и скачивает updates"
        teamd -> localHost "Читает и пишет workspace, запускает процессы и хранит состояние"
        teamd.runtime -> localHost "Читает workspace и запускает processes"
        operator -> localHost "Запускает agentd, редактирует config и открывает локальные UI/browser-представления"
    }

    views {
        systemContext teamd "SystemContext" {
            include *
            autoLayout lr 320 240
            title "teamD - System Context"
            description "Показывает teamD как локальный AI-agent runtime и внешние системы/оператора, с которыми он взаимодействует."
        }

        container teamd "Containers" {
            include *
            autoLayout lr 320 240
            title "teamD Runtime - Containers"
            description "Показывает крупные внутренние части teamD Runtime и их связи с внешними системами."
        }

        styles {
            element "Person" {
                shape person
                background #084c61
                color #ffffff
            }

            element "System" {
                background #177e89
                color #ffffff
            }

            element "External" {
                background #f4d35e
                color #17202a
            }

            relationship "Relationship" {
                routing Orthogonal
            }
        }
    }
}
