mod parse;
mod process;
mod render;
mod repl;

use crate::bootstrap::{App, BootstrapError, SessionSummary};
use crate::daemon;
use crate::execution::{ChatExecutionEvent, ExecutionError, ToolExecutionStatus};
use crate::help::QUICK_HELP_LINE;
use crate::http::client::{DaemonClient, DaemonConnectOptions};
use crate::http::types::{SessionDetailResponse, StatusResponse};
use crate::telegram;
use crate::tui;
use agent_persistence::{
    ArtifactRepository, JobRepository, MissionRecord, MissionRepository, PersistenceStore,
    RunRepository, SessionRecord, SessionRepository, ToolCallRepository, TraceRepository,
};
use agent_runtime::mission::{MissionExecutionIntent, MissionSchedule, MissionSpec, MissionStatus};
use agent_runtime::provider::{FinishReason, ProviderMessage, ProviderRequest, ProviderStreamMode};
use agent_runtime::run::RunSnapshot;
use agent_runtime::session::{MessageRole, Session, SessionSettings};
use encoding_rs::Encoding;
use rusqlite::Connection;
use std::env;
use std::io::{BufRead, Write};
use std::time::{SystemTime, UNIX_EPOCH};

use self::process::{
    daemon_connection_for_process, daemon_supports_command, execute_command,
    execute_daemon_command, merge_connect_options,
};
use self::repl::{run_chat_repl, run_chat_repl_with_backend};

const DEFAULT_SMOKE_PROMPT: &str = "Reply with the single word ready.";
const REPL_HELP: &str = QUICK_HELP_LINE;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Command {
    Status,
    Logs {
        max_lines: Option<usize>,
    },
    Analytics {
        max_lines: Option<usize>,
    },
    TraceShow {
        trace_id: String,
    },
    TraceRun {
        run_id: String,
    },
    TraceExport {
        trace_id: String,
    },
    Version,
    Update {
        tag: Option<String>,
    },
    ProviderSmoke {
        prompt: String,
    },
    AgentList,
    AgentShow {
        identifier: Option<String>,
    },
    AgentSelect {
        identifier: String,
    },
    AgentCreate {
        name: String,
        template_identifier: Option<String>,
    },
    AgentOpen {
        identifier: Option<String>,
    },
    ChatShow {
        session_id: String,
    },
    SessionTranscript {
        id: String,
    },
    SessionTools {
        id: String,
        limit: Option<usize>,
        offset: usize,
        format: SessionToolsFormat,
        include_results: bool,
    },
    SessionToolResult {
        tool_call_id: String,
        raw: bool,
    },
    ChatSend {
        session_id: String,
        message: String,
    },
    ChatRepl {
        session_id: String,
    },
    Tui {
        host: Option<String>,
        port: Option<u16>,
    },
    TelegramRun,
    TelegramPair {
        key: String,
    },
    TelegramPairings,
    Daemon,
    DaemonStop,
    MissionTick {
        now: i64,
    },
    SessionCreate {
        id: String,
        title: String,
    },
    SessionList {
        format: SessionListFormat,
    },
    SessionShow {
        id: String,
    },
    SessionSkills {
        id: String,
    },
    SessionEnableSkill {
        id: String,
        skill_name: String,
    },
    SessionDisableSkill {
        id: String,
        skill_name: String,
    },
    MissionCreate {
        id: String,
        session_id: String,
        objective: String,
    },
    MissionShow {
        id: String,
    },
    RunShow {
        id: String,
    },
    JobShow {
        id: String,
    },
    JobExecute {
        id: String,
        now: i64,
    },
    ApprovalList {
        run_id: String,
    },
    ApprovalApprove {
        run_id: String,
        approval_id: String,
    },
    DelegateList {
        run_id: String,
    },
    VerificationShow {
        run_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProcessInvocation {
    connect: DaemonConnectOptions,
    command: Command,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionToolsFormat {
    Human,
    Raw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionListFormat {
    Human,
    Raw,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ChatSendOutcome {
    Completed {
        session_id: String,
        run_id: Option<String>,
        response_id: String,
        output_text: String,
    },
    WaitingApproval {
        session_id: String,
        run_id: Option<String>,
        approval_id: String,
    },
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct ReplPendingApproval {
    run_id: String,
    approval_id: String,
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn execute<I, S>(app: &App, args: I) -> Result<String, BootstrapError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let command = Command::parse(args)?;

    match command {
        Command::Status => render::render_status(app),
        Command::Logs { max_lines } => render::render_diagnostics_tail(app, max_lines),
        Command::Analytics { max_lines } => render::render_runtime_analytics(app, max_lines),
        Command::TraceShow { trace_id } => render::show_trace(&app.store()?, &trace_id),
        Command::TraceRun { run_id } => render::show_trace_for_run(&app.store()?, &run_id),
        Command::TraceExport { trace_id } => render::export_trace_json(&app.store()?, &trace_id),
        Command::Version => app.render_version_info(),
        Command::Update { tag } => app.update_runtime_binary(tag.as_deref()),
        Command::ProviderSmoke { prompt } => render::run_provider_smoke(app, &prompt),
        Command::AgentList => app.render_agents(),
        Command::AgentShow { identifier } => app.render_agent_profile(identifier.as_deref()),
        Command::AgentSelect { identifier } => {
            let profile = app.select_agent_profile(&identifier)?;
            Ok(format!("текущий агент: {} ({})", profile.name, profile.id))
        }
        Command::AgentCreate {
            name,
            template_identifier,
        } => {
            let profile = app.create_agent_from_template(&name, template_identifier.as_deref())?;
            Ok(format!(
                "создан агент {} ({}) из шаблона {}",
                profile.name,
                profile.id,
                profile.template_kind.as_str()
            ))
        }
        Command::AgentOpen { identifier } => app.render_agent_home(identifier.as_deref()),
        Command::ChatShow { session_id } => render::show_chat(app, &session_id),
        Command::SessionTranscript { id } => render::show_chat(app, &id),
        Command::SessionTools {
            id,
            limit,
            offset,
            format,
            include_results,
        } => render::show_session_tools(&app.store()?, &id, limit, offset, format, include_results),
        Command::SessionToolResult { tool_call_id, raw } => {
            render::show_session_tool_result(&app.store()?, &tool_call_id, raw)
        }
        Command::ChatSend {
            session_id,
            message,
        } => render::send_chat(app, &session_id, &message),
        Command::ChatRepl { .. } => Err(BootstrapError::Usage {
            reason: "chat repl requires interactive I/O".to_string(),
        }),
        Command::Tui { .. } => Err(BootstrapError::Usage {
            reason: "tui requires interactive terminal I/O".to_string(),
        }),
        Command::TelegramRun => Err(BootstrapError::Usage {
            reason: "telegram run requires process I/O".to_string(),
        }),
        Command::TelegramPair { key } => telegram::activate_pairing(app, &key),
        Command::TelegramPairings => telegram::render_pairings(app),
        Command::Daemon => Err(BootstrapError::Usage {
            reason: "daemon requires server mode I/O".to_string(),
        }),
        Command::DaemonStop => Err(BootstrapError::Usage {
            reason: "daemon stop requires process I/O".to_string(),
        }),
        Command::MissionTick { now } => render::run_mission_tick(app, now),
        Command::SessionCreate { id, title } => render::create_session(&app.store()?, &id, &title),
        Command::SessionList { format } => {
            render::show_session_list(&app.list_session_summaries()?, format)
        }
        Command::SessionShow { id } => render::show_session(&app.store()?, &id),
        Command::SessionSkills { id } => app.render_session_skills(&id),
        Command::SessionEnableSkill { id, skill_name } => {
            app.enable_session_skill(&id, &skill_name)?;
            app.render_session_skills(&id)
        }
        Command::SessionDisableSkill { id, skill_name } => {
            app.disable_session_skill(&id, &skill_name)?;
            app.render_session_skills(&id)
        }
        Command::MissionCreate {
            id,
            session_id,
            objective,
        } => render::create_mission(&app.store()?, &id, &session_id, &objective),
        Command::MissionShow { id } => render::show_mission(&app.store()?, &id),
        Command::RunShow { id } => render::show_run(&app.store()?, &id),
        Command::JobShow { id } => render::show_job(&app.store()?, &id),
        Command::JobExecute { id, now } => render::execute_job(app, &id, now),
        Command::ApprovalList { run_id } => render::list_approvals(&app.store()?, &run_id),
        Command::ApprovalApprove {
            run_id,
            approval_id,
        } => render::approve_run(app, &run_id, &approval_id),
        Command::DelegateList { run_id } => render::list_delegates(&app.store()?, &run_id),
        Command::VerificationShow { run_id } => render::show_verification(&app.store()?, &run_id),
    }
}

pub fn execute_process_with_io<I, S, R, W>(
    app: &App,
    args: I,
    input: &mut R,
    output: &mut W,
) -> Result<(), BootstrapError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
    R: BufRead,
    W: Write,
{
    let invocation = ProcessInvocation::parse(args)?;
    let ProcessInvocation { connect, command } = invocation;

    match command {
        Command::Daemon => daemon::serve(app.clone()).map_err(BootstrapError::Stream),
        Command::DaemonStop => {
            let client = DaemonClient::new(&app.config, &connect);
            client.shutdown()?;
            writeln!(output, "daemon stopping").map_err(BootstrapError::Stream)
        }
        Command::Tui { host, port } => {
            let connect = merge_connect_options(connect, host, port);
            tui::run_daemon_backed(app, connect)
        }
        Command::TelegramRun => telegram::run(app),
        Command::ChatRepl { session_id } => {
            let connection = daemon_connection_for_process(app, &connect)?;
            let result =
                run_chat_repl_with_backend(connection.client(), &session_id, input, output);
            let shutdown_result = connection.shutdown_if_autospawned();
            match result {
                Ok(()) => shutdown_result,
                Err(error) => {
                    let _ = shutdown_result;
                    Err(error)
                }
            }
        }
        other if daemon_supports_command(&other) => {
            let connection = daemon_connection_for_process(app, &connect)?;
            let rendered = execute_daemon_command(connection.client(), other);
            let write_result = match rendered {
                Ok(rendered) => writeln!(output, "{rendered}").map_err(BootstrapError::Stream),
                Err(error) => Err(error),
            };
            let shutdown_result = connection.shutdown_if_autospawned();
            match write_result {
                Ok(()) => shutdown_result,
                Err(error) => {
                    let _ = shutdown_result;
                    Err(error)
                }
            }
        }
        other => {
            if connect.host.is_some() || connect.port.is_some() {
                return Err(BootstrapError::Usage {
                    reason: "this command is not available over daemon transport yet".to_string(),
                });
            }
            let rendered = execute_command(app, other)?;
            writeln!(output, "{rendered}").map_err(BootstrapError::Stream)
        }
    }
}

pub fn execute_with_io<I, S, R, W>(
    app: &App,
    args: I,
    input: &mut R,
    output: &mut W,
) -> Result<(), BootstrapError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
    R: BufRead,
    W: Write,
{
    let command = Command::parse(args)?;
    match command {
        Command::ChatRepl { session_id } => run_chat_repl(app, &session_id, input, output),
        Command::Tui { host, port } => {
            tui::run_daemon_backed(app, DaemonConnectOptions { host, port })
        }
        Command::TelegramRun => telegram::run(app),
        Command::Daemon => daemon::serve(app.clone()).map_err(BootstrapError::Stream),
        other => {
            let rendered = execute_command(app, other)?;
            writeln!(output, "{rendered}").map_err(BootstrapError::Stream)
        }
    }
}

fn parse_timestamp(raw: &str, label: &str) -> Result<i64, BootstrapError> {
    raw.parse::<i64>().map_err(|_| BootstrapError::Usage {
        reason: format!("{label} must be an integer unix timestamp"),
    })
}

fn parse_port_arg(raw: &str, label: &str) -> Result<u16, BootstrapError> {
    raw.parse::<u16>().map_err(|_| BootstrapError::Usage {
        reason: format!("{label} must be a valid port number"),
    })
}

fn read_repl_line<R: BufRead>(input: &mut R, line: &mut String) -> Result<usize, std::io::Error> {
    let mut bytes = Vec::new();
    let count = input.read_until(b'\n', &mut bytes)?;
    if count == 0 {
        line.clear();
        return Ok(0);
    }

    *line = decode_repl_line_bytes(&bytes, terminal_encoding_label().as_deref())
        .map_err(|message| std::io::Error::new(std::io::ErrorKind::InvalidData, message))?;
    Ok(count)
}

fn terminal_encoding_label() -> Option<String> {
    ["LC_ALL", "LC_CTYPE", "LANG"]
        .into_iter()
        .find_map(|key| env::var(key).ok())
        .and_then(|value| locale_encoding_label(&value))
}

fn locale_encoding_label(locale: &str) -> Option<String> {
    let normalized = locale.trim();
    if normalized.is_empty() || normalized.eq_ignore_ascii_case("c") || normalized == "POSIX" {
        return None;
    }

    let label = normalized
        .split('.')
        .nth(1)
        .unwrap_or(normalized)
        .split('@')
        .next()
        .unwrap_or(normalized)
        .trim();

    if label.is_empty() {
        None
    } else {
        Some(label.to_string())
    }
}

fn decode_repl_line_bytes(bytes: &[u8], locale_hint: Option<&str>) -> Result<String, String> {
    if let Ok(decoded) = String::from_utf8(bytes.to_vec()) {
        return Ok(decoded);
    }

    let mut labels = Vec::new();
    if let Some(label) = locale_hint {
        labels.push(label.to_string());
    }
    labels.push("windows-1251".to_string());
    labels.push("koi8-r".to_string());

    for label in labels {
        let Some(encoding) = Encoding::for_label(label.as_bytes()) else {
            continue;
        };
        let (decoded, _, had_errors) = encoding.decode(bytes);
        if !had_errors {
            return Ok(decoded.into_owned());
        }
    }

    Err("stream did not contain valid UTF-8".to_string())
}

fn join_required(parts: &[String], label: &'static str) -> Result<String, BootstrapError> {
    let joined = parts.join(" ");
    if joined.trim().is_empty() {
        return Err(BootstrapError::Usage {
            reason: format!("{label} must not be empty"),
        });
    }

    Ok(joined)
}

fn unix_timestamp() -> Result<i64, BootstrapError> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(BootstrapError::Clock)?
        .as_secs() as i64)
}

#[cfg(test)]
mod tests;
