workspace "teamD" "C4-модель архитектуры локального runtime для AI-агентов." {
    !identifiers hierarchical

    model {
        operator = person "Operator" "Пользователь, разработчик или администратор: общается с агентами, читает результаты, подтверждает действия и управляет runtime."

        teamd = softwareSystem "teamD Runtime" "Локальная среда для AI-агентов общего назначения: каноническое выполнение, tools, schedules, memory, inter-agent workflows и интерфейсы оператора." {
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
        teamd -> llmProvider "Отправляет provider requests и получает текст, reasoning и tool calls"
        teamd -> telegram "Получает updates, регистрирует commands, отправляет replies и notifications"
        teamd -> mcpServers "Ищет и вызывает внешние возможности"
        teamd -> githubReleases "Проверяет и скачивает обновления runtime"
        teamd -> localHost "Читает и пишет workspace, запускает процессы и хранит состояние"
        operator -> localHost "Запускает agentd, редактирует config и открывает локальные UI/browser-представления"
    }

    views {
        systemContext teamd "SystemContext" {
            include *
            autoLayout lr 320 240
            title "teamD - System Context"
            description "Показывает teamD как локальный AI-agent runtime и внешние системы/оператора, с которыми он взаимодействует."
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
