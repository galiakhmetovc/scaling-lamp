use super::*;

impl Command {
    pub(super) fn parse<I, S>(args: I) -> Result<Self, BootstrapError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let args = args
            .into_iter()
            .map(|value| value.as_ref().to_string())
            .collect::<Vec<_>>();

        match args.as_slice() {
            [] => Ok(Self::Status),
            [status] if status == "status" => Ok(Self::Status),
            [command] if command == "logs" || command == "логи" => {
                Ok(Self::Logs { max_lines: None })
            }
            [command, max_lines] if command == "logs" || command == "логи" => {
                Ok(Self::Logs {
                    max_lines: Some(parse_log_lines(max_lines)?),
                })
            }
            [command] if command == "version" || command == "версия" => Ok(Self::Version),
            [command] if command == "update" || command == "обновить" => {
                Ok(Self::Update { tag: None })
            }
            [command, tag]
                if command == "update" || command == "обновить" =>
            {
                Ok(Self::Update {
                    tag: Some(tag.clone()),
                })
            }
            [command] if command == "tui" => Ok(Self::Tui {
                host: None,
                port: None,
            }),
            [command, rest @ ..] if command == "tui" => parse_tui_command(rest),
            [scope, action] if scope == "telegram" && action == "run" => Ok(Self::TelegramRun),
            [scope, action, key] if scope == "telegram" && action == "pair" => {
                Ok(Self::TelegramPair { key: key.clone() })
            }
            [scope, action] if scope == "telegram" && action == "pairings" => {
                Ok(Self::TelegramPairings)
            }
            [command] if command == "daemon" => Ok(Self::Daemon),
            [scope, action] if scope == "daemon" && action == "stop" => Ok(Self::DaemonStop),
            [scope, action] if scope == "provider" && action == "smoke" => {
                Ok(Self::ProviderSmoke {
                    prompt: DEFAULT_SMOKE_PROMPT.to_string(),
                })
            }
            [scope, action, prompt @ ..] if scope == "provider" && action == "smoke" => {
                Ok(Self::ProviderSmoke {
                    prompt: join_required(prompt, "smoke prompt")?,
                })
            }
            [scope, action, session_id] if scope == "chat" && action == "show" => {
                Ok(Self::ChatShow {
                    session_id: session_id.clone(),
                })
            }
            [scope, action, session_id] if scope == "chat" && action == "repl" => {
                Ok(Self::ChatRepl {
                    session_id: session_id.clone(),
                })
            }
            [scope, action, session_id, message @ ..] if scope == "chat" && action == "send" => {
                Ok(Self::ChatSend {
                    session_id: session_id.clone(),
                    message: join_required(message, "chat message")?,
                })
            }
            [scope, action] if scope == "mission" && action == "tick" => Ok(Self::MissionTick {
                now: unix_timestamp()?,
            }),
            [scope, action, now] if scope == "mission" && action == "tick" => {
                Ok(Self::MissionTick {
                    now: parse_timestamp(now, "mission tick timestamp")?,
                })
            }
            [scope, action, id, title @ ..] if scope == "session" && action == "create" => {
                let title = join_required(title, "session title")?;
                Ok(Self::SessionCreate {
                    id: id.clone(),
                    title,
                })
            }
            [command, rest @ ..] if command == "sessions" || command == "сессии" => {
                parse_session_list_command(rest)
            }
            [scope, action, rest @ ..]
                if (scope == "session" || scope == "сессия")
                    && (action == "list" || action == "список") =>
            {
                parse_session_list_command(rest)
            }
            [scope, action, id] if scope == "session" && action == "show" => {
                Ok(Self::SessionShow { id: id.clone() })
            }
            [scope, action, id]
                if (scope == "session" || scope == "сессия")
                    && (action == "transcript" || action == "транскрипт") =>
            {
                Ok(Self::SessionTranscript { id: id.clone() })
            }
            [scope, action, id, rest @ ..]
                if (scope == "session" || scope == "сессия")
                    && (action == "tools" || action == "инструменты" || action == "тулы") =>
            {
                parse_session_tools_command(id, rest)
            }
            [scope, action, id] if scope == "session" && action == "skills" => {
                Ok(Self::SessionSkills { id: id.clone() })
            }
            [scope, action, id, skill_name]
                if scope == "session" && action == "enable-skill" =>
            {
                Ok(Self::SessionEnableSkill {
                    id: id.clone(),
                    skill_name: skill_name.clone(),
                })
            }
            [scope, action, id, skill_name]
                if scope == "session" && action == "disable-skill" =>
            {
                Ok(Self::SessionDisableSkill {
                    id: id.clone(),
                    skill_name: skill_name.clone(),
                })
            }
            [scope, action, id, session_id, objective @ ..]
                if scope == "mission" && action == "create" =>
            {
                let objective = join_required(objective, "mission objective")?;
                Ok(Self::MissionCreate {
                    id: id.clone(),
                    session_id: session_id.clone(),
                    objective,
                })
            }
            [scope, action, id] if scope == "mission" && action == "show" => {
                Ok(Self::MissionShow { id: id.clone() })
            }
            [scope, action, id] if scope == "run" && action == "show" => {
                Ok(Self::RunShow { id: id.clone() })
            }
            [scope, action, id] if scope == "job" && action == "show" => {
                Ok(Self::JobShow { id: id.clone() })
            }
            [scope, action, id] if scope == "job" && action == "execute" => Ok(Self::JobExecute {
                id: id.clone(),
                now: unix_timestamp()?,
            }),
            [scope, action, id, now] if scope == "job" && action == "execute" => {
                Ok(Self::JobExecute {
                    id: id.clone(),
                    now: parse_timestamp(now, "job execute timestamp")?,
                })
            }
            [scope, action, run_id] if scope == "approval" && action == "list" => {
                Ok(Self::ApprovalList {
                    run_id: run_id.clone(),
                })
            }
            [scope, action, run_id, approval_id]
                if scope == "approval" && action == "approve" =>
            {
                Ok(Self::ApprovalApprove {
                    run_id: run_id.clone(),
                    approval_id: approval_id.clone(),
                })
            }
            [scope, action, run_id] if scope == "delegate" && action == "list" => {
                Ok(Self::DelegateList {
                    run_id: run_id.clone(),
                })
            }
            [scope, action, run_id] if scope == "verification" && action == "show" => {
                Ok(Self::VerificationShow {
                    run_id: run_id.clone(),
                })
            }
            _ => Err(BootstrapError::Usage {
                reason: "expected one of: status | logs [max_lines] | version | update [tag] | tui | telegram run|pair|pairings | daemon | daemon stop | provider smoke | chat show/send/repl | mission create/show/tick | sessions | session create/list/show/transcript/tools/skills/enable-skill/disable-skill | run show | job show/execute | approval list/approve | delegate list | verification show".to_string(),
            }),
        }
    }
}

fn parse_log_lines(raw: &str) -> Result<usize, BootstrapError> {
    let value = parse_positive_usize(raw, "logs max_lines")?;
    Ok(value)
}

fn parse_session_list_command(args: &[String]) -> Result<Command, BootstrapError> {
    let mut format = SessionListFormat::Human;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--raw" => {
                format = SessionListFormat::Raw;
                index += 1;
            }
            "--human" => {
                format = SessionListFormat::Human;
                index += 1;
            }
            "--format" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(BootstrapError::Usage {
                        reason: "session list --format requires a value".to_string(),
                    });
                };
                format = parse_session_list_format(value)?;
                index += 2;
            }
            other => {
                return Err(BootstrapError::Usage {
                    reason: format!("unsupported session list argument {other}"),
                });
            }
        }
    }

    Ok(Command::SessionList { format })
}

fn parse_session_list_format(raw: &str) -> Result<SessionListFormat, BootstrapError> {
    match raw {
        "human" | "человек" => Ok(SessionListFormat::Human),
        "raw" | "line" | "lines" | "сырой" => Ok(SessionListFormat::Raw),
        other => Err(BootstrapError::Usage {
            reason: format!("unsupported session list format {other}; expected human|raw"),
        }),
    }
}

fn parse_session_tools_command(id: &str, args: &[String]) -> Result<Command, BootstrapError> {
    let mut limit = None;
    let mut offset = 0;
    let mut format = SessionToolsFormat::Human;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--raw" => {
                format = SessionToolsFormat::Raw;
                index += 1;
            }
            "--human" => {
                format = SessionToolsFormat::Human;
                index += 1;
            }
            "--format" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(BootstrapError::Usage {
                        reason: "session tools --format requires a value".to_string(),
                    });
                };
                format = parse_session_tools_format(value)?;
                index += 2;
            }
            "--limit" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(BootstrapError::Usage {
                        reason: "session tools --limit requires a value".to_string(),
                    });
                };
                limit = Some(parse_positive_usize(value, "session tools --limit")?);
                index += 2;
            }
            "--offset" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(BootstrapError::Usage {
                        reason: "session tools --offset requires a value".to_string(),
                    });
                };
                offset = parse_usize(value, "session tools --offset")?;
                index += 2;
            }
            other => {
                return Err(BootstrapError::Usage {
                    reason: format!("unsupported session tools argument {other}"),
                });
            }
        }
    }

    Ok(Command::SessionTools {
        id: id.to_string(),
        limit,
        offset,
        format,
    })
}

fn parse_session_tools_format(raw: &str) -> Result<SessionToolsFormat, BootstrapError> {
    match raw {
        "human" | "человек" => Ok(SessionToolsFormat::Human),
        "raw" | "line" | "lines" | "сырой" => Ok(SessionToolsFormat::Raw),
        other => Err(BootstrapError::Usage {
            reason: format!("unsupported session tools format {other}; expected human|raw"),
        }),
    }
}

fn parse_positive_usize(raw: &str, label: &str) -> Result<usize, BootstrapError> {
    let value = parse_usize(raw, label)?;
    if value == 0 {
        return Err(BootstrapError::Usage {
            reason: format!("{label} must be greater than zero"),
        });
    }
    Ok(value)
}

fn parse_usize(raw: &str, label: &str) -> Result<usize, BootstrapError> {
    raw.parse::<usize>().map_err(|_| BootstrapError::Usage {
        reason: format!("{label} must be a non-negative integer"),
    })
}

impl ProcessInvocation {
    pub(super) fn parse<I, S>(args: I) -> Result<Self, BootstrapError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut args = args
            .into_iter()
            .map(|value| value.as_ref().to_string())
            .collect::<Vec<_>>();
        let connect = parse_global_connect_options(&mut args)?;
        let command = Command::parse(args)?;
        Ok(Self { connect, command })
    }
}

pub(super) fn parse_global_connect_options(
    args: &mut Vec<String>,
) -> Result<DaemonConnectOptions, BootstrapError> {
    let mut host = None;
    let mut port = None;
    let index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--host" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(BootstrapError::Usage {
                        reason: "--host requires a value".to_string(),
                    });
                };
                host = Some(value.clone());
                args.drain(index..=index + 1);
            }
            "--port" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(BootstrapError::Usage {
                        reason: "--port requires a value".to_string(),
                    });
                };
                port = Some(parse_port_arg(value, "--port")?);
                args.drain(index..=index + 1);
            }
            _ => break,
        }
    }
    Ok(DaemonConnectOptions { host, port })
}

pub(super) fn parse_tui_command(args: &[String]) -> Result<Command, BootstrapError> {
    let mut host = None;
    let mut port = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--host" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(BootstrapError::Usage {
                        reason: "tui --host requires a value".to_string(),
                    });
                };
                host = Some(value.clone());
                index += 2;
            }
            "--port" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(BootstrapError::Usage {
                        reason: "tui --port requires a value".to_string(),
                    });
                };
                port = Some(parse_port_arg(value, "tui --port")?);
                index += 2;
            }
            other => {
                return Err(BootstrapError::Usage {
                    reason: format!("unsupported tui argument {other}"),
                });
            }
        }
    }
    Ok(Command::Tui { host, port })
}
