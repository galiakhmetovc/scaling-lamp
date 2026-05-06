use super::client::TelegramCommandSpec;
use super::render::render_usage;

pub(super) const TELEGRAM_INBOUND_QUEUE_MODE_REJECT: &str = "reject";
pub(super) const TELEGRAM_INBOUND_QUEUE_MODE_QUEUE: &str = "queue";
pub(super) const TELEGRAM_INBOUND_QUEUE_MODE_COALESCE: &str = "coalesce";
pub(super) const TELEGRAM_INBOUND_QUEUE_MODE_RESTART: &str = "restart";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ParsedTelegramCommand {
    Start,
    Help,
    New {
        title: Option<String>,
    },
    NewAgent {
        agent_identifier: String,
        title: Option<String>,
    },
    Sessions,
    Use {
        session_id: String,
    },
    Agents,
    AgentUse {
        agent_identifier: String,
    },
    Files,
    File {
        artifact_id: String,
    },
    Judge {
        message: String,
    },
    Agent {
        target_agent_id: String,
        message: String,
    },
    Status,
    Lifecycle,
    Rename {
        title: String,
    },
    Jobs,
    Plan,
    Queue {
        action: TelegramQueueAction,
    },
    Stop,
    Cancel,
    Model {
        model: Option<String>,
    },
    Think {
        level: Option<String>,
    },
    Reasoning {
        visible: bool,
    },
    AutoApprove {
        enabled: bool,
    },
    Compact,
    Skills,
    EnableSkill {
        skill_name: String,
    },
    DisableSkill {
        skill_name: String,
    },
    InvalidUsage(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TelegramQueueAction {
    Show,
    Set {
        mode: String,
        coalesce_window_ms: Option<u64>,
    },
    Flush,
    Clear,
}

pub(super) fn parse_command(text: &str) -> Option<ParsedTelegramCommand> {
    let trimmed = text.trim();
    if !trimmed.starts_with('/') {
        return None;
    }
    let mut parts = trimmed.splitn(2, char::is_whitespace);
    let command = parts.next()?.trim_start_matches('/');
    let command = command.split('@').next().unwrap_or(command);
    let args = parts.next().map(str::trim).unwrap_or("");
    parse_command_parts(command, args)
}

pub(super) fn parse_command_for_bot(
    text: &str,
    bot_username: &str,
) -> Option<ParsedTelegramCommand> {
    let trimmed = text.trim();
    if !trimmed.starts_with('/') {
        return None;
    }
    let mut parts = trimmed.splitn(2, char::is_whitespace);
    let command = parts.next()?.trim_start_matches('/');
    let (command, target_bot) = match command.split_once('@') {
        Some((command, target_bot)) => (command, Some(target_bot)),
        None => (command, None),
    };
    if let Some(target_bot) = target_bot
        && !target_bot.eq_ignore_ascii_case(bot_username)
    {
        return None;
    }
    let args = parts.next().map(str::trim).unwrap_or("");
    parse_command_parts(command, args)
}

fn parse_command_parts(command: &str, args: &str) -> Option<ParsedTelegramCommand> {
    match command {
        "start" => Some(ParsedTelegramCommand::Start),
        "help" => Some(ParsedTelegramCommand::Help),
        "new" => Some(ParsedTelegramCommand::New {
            title: (!args.is_empty()).then(|| args.to_string()),
        }),
        "newagent" => parse_newagent_command(args),
        "session" | "sessions" => Some(ParsedTelegramCommand::Sessions),
        "agents" => Some(ParsedTelegramCommand::Agents),
        "agentuse" => {
            if args.is_empty() {
                Some(ParsedTelegramCommand::InvalidUsage(render_usage(
                    "agentuse",
                    "<agent_id_or_name>",
                )))
            } else {
                Some(ParsedTelegramCommand::AgentUse {
                    agent_identifier: args.to_string(),
                })
            }
        }
        "status" => Some(ParsedTelegramCommand::Status),
        "lifecycle" => {
            if args.is_empty() {
                Some(ParsedTelegramCommand::Lifecycle)
            } else {
                Some(ParsedTelegramCommand::InvalidUsage(render_usage(
                    "lifecycle",
                    "",
                )))
            }
        }
        "rename" => {
            if args.is_empty() {
                Some(ParsedTelegramCommand::InvalidUsage(render_usage(
                    "rename", "<title>",
                )))
            } else {
                Some(ParsedTelegramCommand::Rename {
                    title: args.to_string(),
                })
            }
        }
        "jobs" => Some(ParsedTelegramCommand::Jobs),
        "plan" => Some(ParsedTelegramCommand::Plan),
        "queue" => match parse_queue_action(args) {
            Ok(action) => Some(ParsedTelegramCommand::Queue { action }),
            Err(usage) => Some(ParsedTelegramCommand::InvalidUsage(usage)),
        },
        "stop" | "pause" => Some(ParsedTelegramCommand::Stop),
        "cancel" => Some(ParsedTelegramCommand::Cancel),
        "model" => {
            if args.is_empty() {
                Some(ParsedTelegramCommand::InvalidUsage(render_usage(
                    "model",
                    "<model|default>",
                )))
            } else {
                Some(ParsedTelegramCommand::Model {
                    model: parse_optional_setting(args),
                })
            }
        }
        "think" => {
            if args.is_empty() {
                Some(ParsedTelegramCommand::InvalidUsage(render_usage(
                    "think",
                    "<off|low|medium|high|default>",
                )))
            } else {
                Some(ParsedTelegramCommand::Think {
                    level: parse_optional_setting(args),
                })
            }
        }
        "reasoning" => match parse_bool_setting(args) {
            Some(visible) => Some(ParsedTelegramCommand::Reasoning { visible }),
            None => Some(ParsedTelegramCommand::InvalidUsage(render_usage(
                "reasoning",
                "<on|off>",
            ))),
        },
        "autoapprove" => match parse_bool_setting(args) {
            Some(enabled) => Some(ParsedTelegramCommand::AutoApprove { enabled }),
            None => Some(ParsedTelegramCommand::InvalidUsage(render_usage(
                "autoapprove",
                "<on|off>",
            ))),
        },
        "compact" => Some(ParsedTelegramCommand::Compact),
        "skills" => Some(ParsedTelegramCommand::Skills),
        "enable" => {
            if args.is_empty() {
                Some(ParsedTelegramCommand::InvalidUsage(render_usage(
                    "enable",
                    "<skill_name>",
                )))
            } else {
                Some(ParsedTelegramCommand::EnableSkill {
                    skill_name: args.to_string(),
                })
            }
        }
        "disable" => {
            if args.is_empty() {
                Some(ParsedTelegramCommand::InvalidUsage(render_usage(
                    "disable",
                    "<skill_name>",
                )))
            } else {
                Some(ParsedTelegramCommand::DisableSkill {
                    skill_name: args.to_string(),
                })
            }
        }
        "files" => Some(ParsedTelegramCommand::Files),
        "file" => {
            if args.is_empty() {
                Some(ParsedTelegramCommand::InvalidUsage(render_usage(
                    "file",
                    "<artifact_id>",
                )))
            } else {
                Some(ParsedTelegramCommand::File {
                    artifact_id: args.to_string(),
                })
            }
        }
        "use" => {
            if args.is_empty() {
                Some(ParsedTelegramCommand::InvalidUsage(render_usage(
                    "use",
                    "<session_id>",
                )))
            } else {
                Some(ParsedTelegramCommand::Use {
                    session_id: args.to_string(),
                })
            }
        }
        "judge" => {
            if args.is_empty() {
                Some(ParsedTelegramCommand::InvalidUsage(render_usage(
                    "judge",
                    "<message>",
                )))
            } else {
                Some(ParsedTelegramCommand::Judge {
                    message: args.to_string(),
                })
            }
        }
        "agent" => {
            let mut parts = args.splitn(2, char::is_whitespace);
            let Some(target_agent_id) = parts.next().map(str::trim).filter(|part| !part.is_empty())
            else {
                return Some(ParsedTelegramCommand::InvalidUsage(render_usage(
                    "agent",
                    "<agent_id> <message>",
                )));
            };
            let Some(message) = parts.next().map(str::trim).filter(|part| !part.is_empty()) else {
                return Some(ParsedTelegramCommand::InvalidUsage(render_usage(
                    "agent",
                    "<agent_id> <message>",
                )));
            };
            Some(ParsedTelegramCommand::Agent {
                target_agent_id: target_agent_id.to_string(),
                message: message.to_string(),
            })
        }
        _ => None,
    }
}

fn parse_newagent_command(args: &str) -> Option<ParsedTelegramCommand> {
    let mut parts = args.splitn(2, char::is_whitespace);
    let Some(agent_identifier) = parts.next().map(str::trim).filter(|part| !part.is_empty()) else {
        return Some(ParsedTelegramCommand::InvalidUsage(render_usage(
            "newagent",
            "<agent_id_or_name> [title]",
        )));
    };
    let title = parts
        .next()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string);
    Some(ParsedTelegramCommand::NewAgent {
        agent_identifier: agent_identifier.to_string(),
        title,
    })
}

fn parse_optional_setting(args: &str) -> Option<String> {
    let value = args.trim();
    if value.eq_ignore_ascii_case("default")
        || value.eq_ignore_ascii_case("reset")
        || value.eq_ignore_ascii_case("none")
    {
        None
    } else {
        Some(value.to_string())
    }
}

fn parse_queue_action(args: &str) -> Result<TelegramQueueAction, String> {
    let trimmed = args.trim();
    if trimmed.is_empty() {
        return Ok(TelegramQueueAction::Show);
    }

    let mut parts = trimmed.split_whitespace();
    let command = parts.next().unwrap_or_default().to_ascii_lowercase();
    match command.as_str() {
        TELEGRAM_INBOUND_QUEUE_MODE_REJECT
        | TELEGRAM_INBOUND_QUEUE_MODE_QUEUE
        | TELEGRAM_INBOUND_QUEUE_MODE_RESTART => {
            if parts.next().is_some() {
                return Err(render_usage(
                    "queue",
                    "[reject|queue|coalesce [5000ms|5s]|restart|flush|clear]",
                ));
            }
            Ok(TelegramQueueAction::Set {
                mode: command,
                coalesce_window_ms: None,
            })
        }
        TELEGRAM_INBOUND_QUEUE_MODE_COALESCE => {
            let coalesce_window_ms = match parts.next() {
                Some(value) => Some(parse_queue_duration_ms(value).ok_or_else(|| {
                    render_usage(
                        "queue",
                        "[reject|queue|coalesce [5000ms|5s]|restart|flush|clear]",
                    )
                })?),
                None => None,
            };
            if parts.next().is_some() {
                return Err(render_usage(
                    "queue",
                    "[reject|queue|coalesce [5000ms|5s]|restart|flush|clear]",
                ));
            }
            Ok(TelegramQueueAction::Set {
                mode: command,
                coalesce_window_ms,
            })
        }
        "flush" | "start" => Ok(TelegramQueueAction::Flush),
        "clear" | "stop" => Ok(TelegramQueueAction::Clear),
        _ => Err(render_usage(
            "queue",
            "[reject|queue|coalesce [5000ms|5s]|restart|flush|clear]",
        )),
    }
}

fn parse_queue_duration_ms(value: &str) -> Option<u64> {
    let lower = value.trim().to_ascii_lowercase();
    if let Some(ms) = lower.strip_suffix("ms") {
        return ms.parse::<u64>().ok();
    }
    if let Some(seconds) = lower.strip_suffix('s') {
        return seconds
            .parse::<u64>()
            .ok()
            .map(|value| value.saturating_mul(1_000));
    }
    lower.parse::<u64>().ok()
}

fn parse_bool_setting(args: &str) -> Option<bool> {
    match args.trim().to_ascii_lowercase().as_str() {
        "on" | "true" | "yes" | "1" | "enable" | "enabled" | "вкл" | "да" => Some(true),
        "off" | "false" | "no" | "0" | "disable" | "disabled" | "выкл" | "нет" => {
            Some(false)
        }
        _ => None,
    }
}

pub(super) fn is_session_operator_command(command: &ParsedTelegramCommand) -> bool {
    matches!(
        command,
        ParsedTelegramCommand::Status
            | ParsedTelegramCommand::Lifecycle
            | ParsedTelegramCommand::Rename { .. }
            | ParsedTelegramCommand::Jobs
            | ParsedTelegramCommand::Plan
            | ParsedTelegramCommand::Queue { .. }
            | ParsedTelegramCommand::Stop
            | ParsedTelegramCommand::Cancel
            | ParsedTelegramCommand::Model { .. }
            | ParsedTelegramCommand::Think { .. }
            | ParsedTelegramCommand::Reasoning { .. }
            | ParsedTelegramCommand::AutoApprove { .. }
            | ParsedTelegramCommand::Compact
            | ParsedTelegramCommand::Skills
            | ParsedTelegramCommand::EnableSkill { .. }
            | ParsedTelegramCommand::DisableSkill { .. }
    )
}

pub(super) fn is_valid_telegram_queue_mode(value: &str) -> bool {
    matches!(
        value,
        TELEGRAM_INBOUND_QUEUE_MODE_REJECT
            | TELEGRAM_INBOUND_QUEUE_MODE_QUEUE
            | TELEGRAM_INBOUND_QUEUE_MODE_COALESCE
            | TELEGRAM_INBOUND_QUEUE_MODE_RESTART
    )
}

pub(super) fn coalesce_window_seconds(window_ms: u64) -> i64 {
    let seconds = window_ms.saturating_add(999) / 1_000;
    i64::try_from(seconds).unwrap_or(i64::MAX)
}

pub(super) fn default_command_specs() -> Vec<TelegramCommandSpec> {
    vec![
        TelegramCommandSpec::new("start", "Get a pairing key"),
        TelegramCommandSpec::new("help", "Show Telegram help"),
        TelegramCommandSpec::new("new", "Create and select a session"),
        TelegramCommandSpec::new("newagent", "Create a session for an agent"),
        TelegramCommandSpec::new("sessions", "List sessions"),
        TelegramCommandSpec::new("use", "Select a session by id"),
        TelegramCommandSpec::new("agents", "List agent profiles"),
        TelegramCommandSpec::new("agentuse", "Set chat default agent"),
        TelegramCommandSpec::new("status", "Show current session status"),
        TelegramCommandSpec::new("lifecycle", "Show current session lifecycle"),
        TelegramCommandSpec::new("rename", "Rename current session"),
        TelegramCommandSpec::new("jobs", "Show current session jobs"),
        TelegramCommandSpec::new("plan", "Show current session plan"),
        TelegramCommandSpec::new("queue", "Show or set inbound queue mode"),
        TelegramCommandSpec::new("stop", "Stop the active turn"),
        TelegramCommandSpec::new("pause", "Alias for stop"),
        TelegramCommandSpec::new("cancel", "Cancel current session work"),
        TelegramCommandSpec::new("model", "Set session model"),
        TelegramCommandSpec::new("think", "Set session think level"),
        TelegramCommandSpec::new("reasoning", "Toggle reasoning visibility"),
        TelegramCommandSpec::new("autoapprove", "Toggle auto-approve"),
        TelegramCommandSpec::new("compact", "Compact current session context"),
        TelegramCommandSpec::new("skills", "List session skills"),
        TelegramCommandSpec::new("enable", "Enable a session skill"),
        TelegramCommandSpec::new("disable", "Disable a session skill"),
        TelegramCommandSpec::new("files", "List files in the current session"),
        TelegramCommandSpec::new("file", "Send a session file by artifact id"),
        TelegramCommandSpec::new("judge", "Send a message to Judge"),
        TelegramCommandSpec::new("agent", "Send a message to another agent"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_session_operator_commands() {
        assert_eq!(
            parse_command("/model gpt-5.5"),
            Some(ParsedTelegramCommand::Model {
                model: Some("gpt-5.5".to_string())
            })
        );
        assert_eq!(
            parse_command("/model default"),
            Some(ParsedTelegramCommand::Model { model: None })
        );
        assert_eq!(
            parse_command("/think off"),
            Some(ParsedTelegramCommand::Think {
                level: Some("off".to_string())
            })
        );
        assert_eq!(
            parse_command("/think default"),
            Some(ParsedTelegramCommand::Think { level: None })
        );
        assert_eq!(
            parse_command("/reasoning on"),
            Some(ParsedTelegramCommand::Reasoning { visible: true })
        );
        assert_eq!(
            parse_command("/autoapprove off"),
            Some(ParsedTelegramCommand::AutoApprove { enabled: false })
        );
        assert_eq!(
            parse_command("/status"),
            Some(ParsedTelegramCommand::Status)
        );
        assert_eq!(
            parse_command("/lifecycle"),
            Some(ParsedTelegramCommand::Lifecycle)
        );
        assert_eq!(
            parse_command("/rename Leads triage"),
            Some(ParsedTelegramCommand::Rename {
                title: "Leads triage".to_string()
            })
        );
        assert!(matches!(
            parse_command("/rename"),
            Some(ParsedTelegramCommand::InvalidUsage(_))
        ));
        assert_eq!(parse_command("/jobs"), Some(ParsedTelegramCommand::Jobs));
        assert_eq!(parse_command("/plan"), Some(ParsedTelegramCommand::Plan));
        assert_eq!(
            parse_command("/session"),
            Some(ParsedTelegramCommand::Sessions)
        );
        assert_eq!(
            parse_command("/queue coalesce 5s"),
            Some(ParsedTelegramCommand::Queue {
                action: TelegramQueueAction::Set {
                    mode: "coalesce".to_string(),
                    coalesce_window_ms: Some(5_000)
                }
            })
        );
        assert_eq!(
            parse_command("/queue coalesce 4s"),
            Some(ParsedTelegramCommand::Queue {
                action: TelegramQueueAction::Set {
                    mode: "coalesce".to_string(),
                    coalesce_window_ms: Some(4_000)
                }
            })
        );
        assert_eq!(
            parse_command("/queue stop"),
            Some(ParsedTelegramCommand::Queue {
                action: TelegramQueueAction::Clear
            })
        );
        assert!(matches!(
            parse_command("/queue reject extra"),
            Some(ParsedTelegramCommand::InvalidUsage(_))
        ));
        assert_eq!(parse_command("/pause"), Some(ParsedTelegramCommand::Stop));
        assert_eq!(parse_command("/stop"), Some(ParsedTelegramCommand::Stop));
        assert_eq!(
            parse_command("/cancel"),
            Some(ParsedTelegramCommand::Cancel)
        );
        assert_eq!(
            parse_command("/compact"),
            Some(ParsedTelegramCommand::Compact)
        );
        assert_eq!(
            parse_command("/skills"),
            Some(ParsedTelegramCommand::Skills)
        );
        assert_eq!(
            parse_command("/agents"),
            Some(ParsedTelegramCommand::Agents)
        );
        assert_eq!(
            parse_command("/agentuse judge"),
            Some(ParsedTelegramCommand::AgentUse {
                agent_identifier: "judge".to_string()
            })
        );
        assert_eq!(
            parse_command("/newagent judge Review room"),
            Some(ParsedTelegramCommand::NewAgent {
                agent_identifier: "judge".to_string(),
                title: Some("Review room".to_string())
            })
        );
        assert_eq!(
            parse_command("/newagent judge"),
            Some(ParsedTelegramCommand::NewAgent {
                agent_identifier: "judge".to_string(),
                title: None
            })
        );
        assert_eq!(
            parse_command("/enable silverbullet-space"),
            Some(ParsedTelegramCommand::EnableSkill {
                skill_name: "silverbullet-space".to_string()
            })
        );
        assert_eq!(
            parse_command("/disable silverbullet-space"),
            Some(ParsedTelegramCommand::DisableSkill {
                skill_name: "silverbullet-space".to_string()
            })
        );
    }

    #[test]
    fn registers_session_operator_commands() {
        let commands = default_command_specs()
            .into_iter()
            .map(|command| command.command)
            .collect::<Vec<_>>();

        for expected in [
            "status",
            "lifecycle",
            "rename",
            "jobs",
            "queue",
            "stop",
            "pause",
            "cancel",
            "model",
            "think",
            "reasoning",
            "autoapprove",
            "compact",
            "skills",
            "agents",
            "agentuse",
            "newagent",
            "enable",
            "disable",
        ] {
            assert!(
                commands.iter().any(|command| command == expected),
                "missing Telegram command: {expected}"
            );
        }
    }
}
