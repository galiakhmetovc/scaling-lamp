workspace "teamD" "C4-модель архитектуры локальной среды для AI-агентов." {
    !identifiers hierarchical
    !docs docs

    model {
        operators = person "Operators" "Люди или внешние automation-участники: работают с агентами, читают результаты, подтверждают действия и управляют runtime."

        agentdClients = softwareSystem "agentd Clients" "CLI, TUI, HTTP clients и Telegram-mediated client flow. Клиенты отправляют команды и показывают состояние, но не исполняют агентскую работу." {
            !docs system-docs/agentd-clients
            tags "Client"
        }

        executionMesh = softwareSystem "teamD Execution Mesh" "Один или несколько execution nodes с agentd, связанных в mesh. Здесь исполняются sessions, jobs, tools, schedules, inter-agent flows и provider calls." {
            !docs system-docs/execution-mesh

            agentd = container "agentd" "Daemon/runtime process внутри execution node: HTTP API, canonical runtime, provider loop, tools, approvals, schedules, persistence, inter-agent routing." "Rust binary" {
                !docs container-docs/agentd
            }

            internalMcp = container "Internal MCP Server" "MCP server, запущенный внутри execution node или управляемый тем же окружением. Даёт agentd локальные tools/resources/prompts." "MCP server" {
                !docs container-docs/internal-mcp-server
            }

            tags "System"
        }

        llmProviderApis = softwareSystem "LLM Provider APIs" "Внешние API моделей: принимают provider requests и возвращают assistant text, reasoning и structured tool calls." {
            tags "External"
        }

        mcpCapabilityProviders = softwareSystem "MCP Capability Providers" "Внешние или внутренние поставщики capabilities: MCP tools, resources и prompts. На deployment view раскладываются на internal/external MCP servers." {
            !docs system-docs/mcp-capability-providers
            tags "Capability Boundary"
        }

        targetResources = softwareSystem "Target Resources" "Ресурсы, на которые agentd или MCP tools могут воздействовать: workspace, filesystem, OS processes, Git repos, APIs, infrastructure, databases, cloud resources." {
            !docs system-docs/target-resources
            tags "Resource Boundary"
        }

        operators -> agentdClients "Работают через CLI, TUI, HTTP или Telegram"
        agentdClients -> executionMesh "Отправляют команды, сообщения и читают состояние"
        executionMesh -> llmProviderApis "Отправляет provider requests"
        executionMesh -> mcpCapabilityProviders "Ищет и вызывает capabilities"
        executionMesh -> targetResources "Воздействует напрямую через built-in tools"
        mcpCapabilityProviders -> targetResources "Воздействуют через MCP tools"

        agentdClients -> executionMesh.agentd "Подключаются к daemon API"
        executionMesh.agentd -> llmProviderApis "Отправляет provider requests"
        executionMesh.agentd -> mcpCapabilityProviders "Вызывает MCP capabilities"
        executionMesh.agentd -> executionMesh.internalMcp "Вызывает локальные MCP tools/resources/prompts"
        executionMesh.agentd -> targetResources "Читает/пишет workspace, запускает processes, вызывает APIs"
        executionMesh.internalMcp -> targetResources "Воздействует на локальные или внешние resources"

        production = deploymentEnvironment "Runtime Mesh" {
            nodeA = deploymentNode "Execution Node A" "Машина или окружение, где запущен agentd daemon и локальное состояние." "Linux/WSL/server" {
                agentdA = containerInstance executionMesh.agentd
                internalMcpA = containerInstance executionMesh.internalMcp
                localResourcesA = infrastructureNode "Local Target Resources A" "Workspace, filesystem, OS processes и локальные tools этого execution node." "Local resources" "Resource Boundary"
            }

            nodeB = deploymentNode "Execution Node B" "Второй execution node в mesh. Может быть локальным, удалённым или временным." "Linux/WSL/server" {
                agentdB = containerInstance executionMesh.agentd
                localResourcesB = infrastructureNode "Local Target Resources B" "Workspace, filesystem, OS processes и локальные tools этого execution node." "Local resources" "Resource Boundary"
            }

            externalMcp = deploymentNode "External MCP Server" "MCP server вне execution nodes: отдельный сервис, remote tool gateway или shared capability provider." "MCP server" {
                externalMcpServer = infrastructureNode "External MCP Endpoint" "MCP tools/resources/prompts outside the execution mesh." "MCP"
            }

            externalResources = deploymentNode "External Target Resources" "Ресурсы вне execution nodes: GitHub, cloud, databases, Kubernetes, external APIs, infrastructure." "External systems" {
                externalTargets = infrastructureNode "External APIs / Infrastructure" "External resources controlled through built-in tools or MCP tools." "External resources" "Resource Boundary"
            }

            production.nodeA -> production.nodeB "Mesh: remote delegation, inter-agent routing, future A2A"
            production.nodeB -> production.nodeA "Mesh: callbacks and reverse delegation"
            production.nodeA.agentdA -> production.nodeA.localResourcesA "Built-in tools"
            production.nodeB.agentdB -> production.nodeB.localResourcesB "Built-in tools"
            production.nodeA.internalMcpA -> production.nodeA.localResourcesA "MCP tools"
            production.nodeA.agentdA -> production.externalMcp.externalMcpServer "MCP protocol"
            production.nodeB.agentdB -> production.externalMcp.externalMcpServer "MCP protocol"
            production.externalMcp.externalMcpServer -> production.externalResources.externalTargets "MCP tools"
            production.nodeA.agentdA -> production.externalResources.externalTargets "Built-in tools / remote APIs"
            production.nodeB.agentdB -> production.externalResources.externalTargets "Built-in tools / remote APIs"
        }

        telegramRuntime = deploymentEnvironment "Telegram Runtime" {
            operatorDevice = deploymentNode "Operator Device" "Устройство оператора с Telegram client." "Phone/Desktop" {
                telegramClient = infrastructureNode "Telegram Client" "Мобильный или desktop Telegram client оператора." "Telegram client" "Client"
            }

            telegramCloud = deploymentNode "Telegram Cloud" "Внешняя инфраструктура Telegram." "Telegram" {
                telegramBotApi = infrastructureNode "Telegram Bot API" "Bot API endpoint: long polling, commands, pairing keys, replies and notifications." "Telegram Bot API" "External"
            }

            executionNode = deploymentNode "Execution Node" "Машина или окружение, где запущен agentd daemon с Telegram long polling." "Linux/WSL/server" {
                agentdTelegram = containerInstance executionMesh.agentd
                localState = infrastructureNode "Local State" "SQLite metadata, payload files, config and .env for this node." "SQLite + files" "Resource Boundary"
                localResources = infrastructureNode "Local Target Resources" "Workspace, filesystem, OS processes and local tools." "Local resources" "Resource Boundary"
            }

            llmCloud = deploymentNode "LLM Provider" "Внешний provider API, который обслуживает agent turns." "External API" {
                llmEndpoint = infrastructureNode "LLM Provider API" "Model endpoint for assistant text, reasoning and tool calls." "HTTPS API" "External"
            }

            telegramRuntime.operatorDevice.telegramClient -> telegramRuntime.telegramCloud.telegramBotApi "Messages and commands"
            telegramRuntime.telegramCloud.telegramBotApi -> telegramRuntime.executionNode.agentdTelegram "Updates via long polling; replies use Bot API"
            telegramRuntime.executionNode.agentdTelegram -> telegramRuntime.executionNode.localState "Stores sessions, jobs, schedules, artifacts"
            telegramRuntime.executionNode.agentdTelegram -> telegramRuntime.executionNode.localResources "Built-in tools"
            telegramRuntime.executionNode.agentdTelegram -> telegramRuntime.llmCloud.llmEndpoint "Provider requests"
        }
    }

    views {
        systemContext executionMesh "SystemContext" {
            include operators
            include agentdClients
            include executionMesh
            include llmProviderApis
            include mcpCapabilityProviders
            include targetResources
            autoLayout lr 320 240
            title "teamD - System Context"
            description "Показывает Operators, agentd Clients, teamD Execution Mesh, LLM Provider APIs, MCP Capability Providers и Target Resources."
        }

        container executionMesh "Containers" {
            include *
            autoLayout lr 320 240
            title "teamD Execution Mesh - Containers"
            description "Показывает внутренние containers execution mesh: agentd и internal MCP server."
        }

        deployment executionMesh production "Deployment" {
            include *
            autoLayout lr 320 240
            title "teamD Execution Mesh - Deployment"
            description "Показывает execution nodes, agentd instances, internal/external MCP и target resources."
        }

        deployment executionMesh telegramRuntime "TelegramDeployment" {
            include *
            autoLayout lr 320 240
            title "teamD Telegram Runtime - Deployment"
            description "Показывает практический deployment для работы оператора через Telegram: client, Bot API, один execution node с agentd, local state/resources и LLM provider."
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

            element "Client" {
                background #3a6ea5
                color #ffffff
            }

            element "External" {
                background #f4d35e
                color #17202a
            }

            element "Capability Boundary" {
                background #f4a261
                color #17202a
            }

            element "Resource Boundary" {
                background #e9ecef
                color #17202a
            }

            element "Deployment Node" {
                background #ffffff
                color #17202a
                stroke #697386
            }

            relationship "Relationship" {
                routing Orthogonal
            }
        }
    }
}
