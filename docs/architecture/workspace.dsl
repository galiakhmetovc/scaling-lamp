workspace "teamD" "C4 architecture model for the local AI-agent runtime." {
    !identifiers hierarchical

    model {
        operator = person "Operator" "A user, developer, or administrator who works with agents, reviews results, approves actions, and manages runtime state."

        teamd = softwareSystem "teamD Runtime" "Local general-purpose AI-agent runtime with canonical execution, tools, schedules, memory, inter-agent workflows, and operator surfaces." {
            tags "System"
        }

        llmProvider = softwareSystem "LLM Provider" "External model API used to generate assistant text, reasoning, and structured tool calls." {
            tags "External"
        }

        telegram = softwareSystem "Telegram Bot API" "External Telegram API used for operator chat access, pairing, commands, and outbound notifications." {
            tags "External"
        }

        mcpServers = softwareSystem "MCP Servers" "External or local MCP-compatible servers that expose additional tools, resources, and prompts." {
            tags "External"
        }

        githubReleases = softwareSystem "GitHub Releases" "Release source used by the updater to check and download published agentd binaries." {
            tags "External"
        }

        localHost = softwareSystem "Local Host" "The operator's machine or server: filesystem, OS processes, terminal, workspace, SQLite database, and payload files." {
            tags "External"
        }

        operator -> teamd "Works with agents through CLI, TUI, Telegram, and HTTP surfaces"
        teamd -> llmProvider "Sends provider requests and receives text, reasoning, and tool calls"
        teamd -> telegram "Polls updates, registers commands, sends replies and notifications"
        teamd -> mcpServers "Discovers and invokes external capabilities"
        teamd -> githubReleases "Checks and downloads runtime updates"
        teamd -> localHost "Reads/writes workspace files, runs processes, and stores persistent state"
        operator -> localHost "Runs agentd, edits configuration, and opens local UI/browser views"
    }

    views {
        systemContext teamd "SystemContext" {
            include *
            autoLayout lr
            title "teamD - System Context"
            description "Shows teamD as a local AI-agent runtime and the external systems/operators it interacts with."
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
        }
    }
}
