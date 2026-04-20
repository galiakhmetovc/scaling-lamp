use crate::bootstrap::{App, BootstrapError};
use crate::daemon;
use crate::execution::{ChatExecutionEvent, ExecutionError, ToolExecutionStatus};
use crate::http::client::DaemonConnectOptions;
use crate::tui;
use agent_persistence::{
    JobRepository, MissionRecord, MissionRepository, PersistenceStore, RunRepository,
    SessionRecord, SessionRepository,
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

const DEFAULT_SMOKE_PROMPT: &str = "Reply with the single word ready.";
const REPL_HELP: &str = "commands: /help | /show | /plan | /approve [approval-id] | /exit";

#[derive(Debug, Clone, PartialEq, Eq)]
enum Command {
    Status,
    ProviderSmoke {
        prompt: String,
    },
    ChatShow {
        session_id: String,
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
    Daemon,
    MissionTick {
        now: i64,
    },
    SessionCreate {
        id: String,
        title: String,
    },
    SessionShow {
        id: String,
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

#[cfg_attr(not(test), allow(dead_code))]
pub fn execute<I, S>(app: &App, args: I) -> Result<String, BootstrapError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let command = Command::parse(args)?;

    match command {
        Command::Status => render_status(app),
        Command::ProviderSmoke { prompt } => run_provider_smoke(app, &prompt),
        Command::ChatShow { session_id } => show_chat(app, &session_id),
        Command::ChatSend {
            session_id,
            message,
        } => send_chat(app, &session_id, &message),
        Command::ChatRepl { .. } => Err(BootstrapError::Usage {
            reason: "chat repl requires interactive I/O".to_string(),
        }),
        Command::Tui { .. } => Err(BootstrapError::Usage {
            reason: "tui requires interactive terminal I/O".to_string(),
        }),
        Command::Daemon => Err(BootstrapError::Usage {
            reason: "daemon requires server mode I/O".to_string(),
        }),
        Command::MissionTick { now } => run_mission_tick(app, now),
        Command::SessionCreate { id, title } => create_session(&app.store()?, &id, &title),
        Command::SessionShow { id } => show_session(&app.store()?, &id),
        Command::MissionCreate {
            id,
            session_id,
            objective,
        } => create_mission(&app.store()?, &id, &session_id, &objective),
        Command::MissionShow { id } => show_mission(&app.store()?, &id),
        Command::RunShow { id } => show_run(&app.store()?, &id),
        Command::JobShow { id } => show_job(&app.store()?, &id),
        Command::JobExecute { id, now } => execute_job(app, &id, now),
        Command::ApprovalList { run_id } => list_approvals(&app.store()?, &run_id),
        Command::ApprovalApprove {
            run_id,
            approval_id,
        } => approve_run(app, &run_id, &approval_id),
        Command::DelegateList { run_id } => list_delegates(&app.store()?, &run_id),
        Command::VerificationShow { run_id } => show_verification(&app.store()?, &run_id),
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
        Command::Daemon => daemon::serve(app.clone()).map_err(BootstrapError::Stream),
        other => {
            let rendered = execute_command(app, other)?;
            writeln!(output, "{rendered}").map_err(BootstrapError::Stream)
        }
    }
}

impl Command {
    fn parse<I, S>(args: I) -> Result<Self, BootstrapError>
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
            [command] if command == "tui" => Ok(Self::Tui {
                host: None,
                port: None,
            }),
            [command, rest @ ..] if command == "tui" => parse_tui_command(rest),
            [command] if command == "daemon" => Ok(Self::Daemon),
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
            [scope, action, id] if scope == "session" && action == "show" => {
                Ok(Self::SessionShow { id: id.clone() })
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
                reason: "expected one of: status | tui | daemon | provider smoke | chat show/send/repl | mission create/show/tick | session create/show | run show | job show/execute | approval list/approve | delegate list | verification show".to_string(),
            }),
        }
    }
}

fn parse_tui_command(args: &[String]) -> Result<Command, BootstrapError> {
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

fn execute_command(app: &App, command: Command) -> Result<String, BootstrapError> {
    match command {
        Command::Status => render_status(app),
        Command::ProviderSmoke { prompt } => run_provider_smoke(app, &prompt),
        Command::ChatShow { session_id } => show_chat(app, &session_id),
        Command::ChatSend {
            session_id,
            message,
        } => send_chat(app, &session_id, &message),
        Command::ChatRepl { .. } => Err(BootstrapError::Usage {
            reason: "chat repl requires interactive I/O".to_string(),
        }),
        Command::Tui { .. } => Err(BootstrapError::Usage {
            reason: "tui requires interactive terminal I/O".to_string(),
        }),
        Command::Daemon => Err(BootstrapError::Usage {
            reason: "daemon requires server mode I/O".to_string(),
        }),
        Command::MissionTick { now } => run_mission_tick(app, now),
        Command::SessionCreate { id, title } => create_session(&app.store()?, &id, &title),
        Command::SessionShow { id } => show_session(&app.store()?, &id),
        Command::MissionCreate {
            id,
            session_id,
            objective,
        } => create_mission(&app.store()?, &id, &session_id, &objective),
        Command::MissionShow { id } => show_mission(&app.store()?, &id),
        Command::RunShow { id } => show_run(&app.store()?, &id),
        Command::JobShow { id } => show_job(&app.store()?, &id),
        Command::JobExecute { id, now } => execute_job(app, &id, now),
        Command::ApprovalList { run_id } => list_approvals(&app.store()?, &run_id),
        Command::ApprovalApprove {
            run_id,
            approval_id,
        } => approve_run(app, &run_id, &approval_id),
        Command::DelegateList { run_id } => list_delegates(&app.store()?, &run_id),
        Command::VerificationShow { run_id } => show_verification(&app.store()?, &run_id),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ChatSendOutcome {
    Completed {
        session_id: String,
        run_id: String,
        response_id: String,
        output_text: String,
    },
    WaitingApproval {
        session_id: String,
        run_id: String,
        approval_id: String,
    },
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct ReplPendingApproval {
    run_id: String,
    approval_id: String,
}

struct ReplRenderer<'a, W: Write> {
    output: &'a mut W,
    active_tool: Option<String>,
    reasoning_open: bool,
    assistant_open: bool,
    assistant_streamed_this_turn: bool,
}

impl<'a, W: Write> ReplRenderer<'a, W> {
    fn new(output: &'a mut W) -> Self {
        Self {
            output,
            active_tool: None,
            reasoning_open: false,
            assistant_open: false,
            assistant_streamed_this_turn: false,
        }
    }

    fn emit(&mut self, event: ChatExecutionEvent) -> Result<(), BootstrapError> {
        match event {
            ChatExecutionEvent::ReasoningDelta(delta) => self.write_reasoning_delta(&delta),
            ChatExecutionEvent::AssistantTextDelta(delta) => self.write_assistant_delta(&delta),
            ChatExecutionEvent::ToolStatus { tool_name, status } => {
                self.write_tool_status(&tool_name, status)
            }
        }
    }

    fn finish_turn(&mut self) -> Result<(), BootstrapError> {
        if self.reasoning_open {
            writeln!(self.output).map_err(BootstrapError::Stream)?;
            self.reasoning_open = false;
        }
        if self.assistant_open {
            writeln!(self.output).map_err(BootstrapError::Stream)?;
            self.assistant_open = false;
        }
        Ok(())
    }

    fn begin_turn(&mut self) {
        self.assistant_streamed_this_turn = false;
    }

    fn assistant_streamed_this_turn(&self) -> bool {
        self.assistant_streamed_this_turn
    }

    fn write_reasoning_delta(&mut self, delta: &str) -> Result<(), BootstrapError> {
        if self.assistant_open {
            writeln!(self.output).map_err(BootstrapError::Stream)?;
            self.assistant_open = false;
        }
        if self.reasoning_open {
            write!(self.output, "{delta}").map_err(BootstrapError::Stream)?;
        } else {
            write!(self.output, "reasoning: {delta}").map_err(BootstrapError::Stream)?;
            self.reasoning_open = true;
        }
        self.output.flush().map_err(BootstrapError::Stream)
    }

    fn write_assistant_delta(&mut self, delta: &str) -> Result<(), BootstrapError> {
        if self.reasoning_open {
            writeln!(self.output).map_err(BootstrapError::Stream)?;
            self.reasoning_open = false;
        }
        if self.assistant_open {
            write!(self.output, "{delta}").map_err(BootstrapError::Stream)?;
        } else {
            write!(self.output, "assistant: {delta}").map_err(BootstrapError::Stream)?;
            self.assistant_open = true;
        }
        self.assistant_streamed_this_turn = true;
        self.output.flush().map_err(BootstrapError::Stream)
    }

    fn write_tool_status(
        &mut self,
        tool_name: &str,
        status: ToolExecutionStatus,
    ) -> Result<(), BootstrapError> {
        self.finish_turn()?;
        let line = format!("tool: {tool_name} | {}", status.as_str());
        match &self.active_tool {
            Some(current) if current == tool_name => {
                write!(self.output, "\x1b[1A\r\x1b[2K{line}\n").map_err(BootstrapError::Stream)?;
            }
            _ => {
                writeln!(self.output, "{line}").map_err(BootstrapError::Stream)?;
            }
        }
        if matches!(
            status,
            ToolExecutionStatus::Completed | ToolExecutionStatus::Failed
        ) {
            self.active_tool = None;
        } else {
            self.active_tool = Some(tool_name.to_string());
        }
        self.output.flush().map_err(BootstrapError::Stream)
    }
}

fn run_chat_repl<R, W>(
    app: &App,
    session_id: &str,
    input: &mut R,
    output: &mut W,
) -> Result<(), BootstrapError>
where
    R: BufRead,
    W: Write,
{
    writeln!(output, "chat repl session_id={session_id}").map_err(BootstrapError::Stream)?;
    writeln!(output, "{REPL_HELP}").map_err(BootstrapError::Stream)?;

    let mut line = String::new();
    let mut renderer = ReplRenderer::new(output);

    loop {
        write!(renderer.output, "> ").map_err(BootstrapError::Stream)?;
        renderer.output.flush().map_err(BootstrapError::Stream)?;

        line.clear();
        let bytes = read_repl_line(input, &mut line).map_err(BootstrapError::Stream)?;
        if bytes == 0 {
            renderer.finish_turn()?;
            writeln!(renderer.output, "leaving chat repl session_id={session_id}")
                .map_err(BootstrapError::Stream)?;
            return Ok(());
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        match trimmed {
            "/exit" => {
                renderer.finish_turn()?;
                writeln!(renderer.output, "leaving chat repl session_id={session_id}")
                    .map_err(BootstrapError::Stream)?;
                return Ok(());
            }
            "/help" => {
                renderer.finish_turn()?;
                writeln!(renderer.output, "{REPL_HELP}").map_err(BootstrapError::Stream)?;
            }
            "/show" => {
                renderer.finish_turn()?;
                let transcript = show_chat(app, session_id)?;
                writeln!(renderer.output, "{transcript}").map_err(BootstrapError::Stream)?;
            }
            "/plan" => {
                renderer.finish_turn()?;
                let plan = app.render_plan(session_id)?;
                writeln!(renderer.output, "{plan}").map_err(BootstrapError::Stream)?;
            }
            _ if trimmed.starts_with("/approve") => {
                let requested = trimmed.split_whitespace().nth(1).map(ToString::to_string);
                let Some(current) = find_pending_approval(app, session_id, requested.as_deref())?
                else {
                    renderer.finish_turn()?;
                    writeln!(
                        renderer.output,
                        "no pending approval for session_id={session_id}"
                    )
                    .map_err(BootstrapError::Stream)?;
                    continue;
                };
                let approval_id = current.approval_id.clone();
                renderer.begin_turn();
                let mut emit_error = None;
                let mut emit = |event| {
                    if emit_error.is_none() {
                        emit_error = renderer.emit(event).err();
                    }
                };
                let report = app.approve_run_with_observer(
                    &current.run_id,
                    &approval_id,
                    unix_timestamp()?,
                    &mut emit,
                )?;
                if let Some(error) = emit_error {
                    return Err(error);
                }
                renderer.finish_turn()?;
                if let Some(text) = report.output_text.as_deref() {
                    if text.is_empty() || renderer.assistant_streamed_this_turn() {
                        continue;
                    }
                    writeln!(renderer.output, "assistant: {text}")
                        .map_err(BootstrapError::Stream)?;
                }
            }
            message => {
                if find_pending_approval(app, session_id, None)?.is_some() {
                    renderer.finish_turn()?;
                    writeln!(
                        renderer.output,
                        "finish the pending approval before sending another message"
                    )
                    .map_err(BootstrapError::Stream)?;
                    continue;
                }

                renderer.begin_turn();
                let mut emit_error = None;
                let mut emit = |event| {
                    if emit_error.is_none() {
                        emit_error = renderer.emit(event).err();
                    }
                };
                match send_chat_outcome_with_observer(app, session_id, message, &mut emit)? {
                    ChatSendOutcome::Completed { output_text, .. } => {
                        if let Some(error) = emit_error {
                            return Err(error);
                        }
                        renderer.finish_turn()?;
                        if output_text.is_empty() || renderer.assistant_streamed_this_turn() {
                            continue;
                        }
                        writeln!(renderer.output, "assistant: {output_text}")
                            .map_err(BootstrapError::Stream)?;
                    }
                    ChatSendOutcome::WaitingApproval { approval_id, .. } => {
                        if let Some(error) = emit_error {
                            return Err(error);
                        }
                        let _ = approval_id;
                        renderer.finish_turn()?;
                    }
                }
            }
        }
    }
}

fn find_pending_approval(
    app: &App,
    session_id: &str,
    requested_approval_id: Option<&str>,
) -> Result<Option<ReplPendingApproval>, BootstrapError> {
    Ok(app
        .latest_pending_approval(session_id, requested_approval_id)?
        .map(|pending| ReplPendingApproval {
            run_id: pending.run_id,
            approval_id: pending.approval_id,
        }))
}

fn create_session(
    store: &PersistenceStore,
    id: &str,
    title: &str,
) -> Result<String, BootstrapError> {
    let now = unix_timestamp()?;
    let session = Session {
        id: id.to_string(),
        title: title.to_string(),
        prompt_override: None,
        settings: SessionSettings::default(),
        active_mission_id: None,
        created_at: now,
        updated_at: now,
    };
    let record = SessionRecord::try_from(&session).map_err(BootstrapError::RecordConversion)?;
    store.put_session(&record)?;
    Ok(format!(
        "created session {} title={}",
        record.id, record.title
    ))
}

fn show_session(store: &PersistenceStore, id: &str) -> Result<String, BootstrapError> {
    let record = store
        .get_session(id)?
        .ok_or_else(|| BootstrapError::MissingRecord {
            kind: "session",
            id: id.to_string(),
        })?;

    Ok(format!(
        "session id={} title={} active_mission_id={} settings={}",
        record.id,
        record.title,
        record.active_mission_id.as_deref().unwrap_or("<none>"),
        record.settings_json
    ))
}

fn show_chat(app: &App, session_id: &str) -> Result<String, BootstrapError> {
    let transcript = app.session_transcript(session_id)?;
    let rendered = transcript.render();
    if rendered.is_empty() {
        return Ok("<empty>".to_string());
    }

    Ok(rendered)
}

fn send_chat(app: &App, session_id: &str, message: &str) -> Result<String, BootstrapError> {
    match send_chat_outcome(app, session_id, message)? {
        ChatSendOutcome::Completed {
            session_id,
            run_id,
            response_id,
            output_text,
        } => Ok(format!(
            "chat send session_id={} run_id={} response_id={} output={}",
            session_id, run_id, response_id, output_text
        )),
        ChatSendOutcome::WaitingApproval {
            session_id,
            run_id,
            approval_id,
        } => Ok(format!(
            "chat send session_id={} run_id={} status=waiting_approval approval_id={}",
            session_id, run_id, approval_id
        )),
    }
}

fn send_chat_outcome(
    app: &App,
    session_id: &str,
    message: &str,
) -> Result<ChatSendOutcome, BootstrapError> {
    let mut observer = None;
    send_chat_outcome_internal(app, session_id, message, &mut observer)
}

fn send_chat_outcome_with_observer(
    app: &App,
    session_id: &str,
    message: &str,
    observer: &mut dyn FnMut(ChatExecutionEvent),
) -> Result<ChatSendOutcome, BootstrapError> {
    let mut observer = Some(observer);
    send_chat_outcome_internal(app, session_id, message, &mut observer)
}

fn send_chat_outcome_internal(
    app: &App,
    session_id: &str,
    message: &str,
    observer: &mut Option<&mut dyn FnMut(ChatExecutionEvent)>,
) -> Result<ChatSendOutcome, BootstrapError> {
    let now = unix_timestamp()?;
    let run_id = format!("run-chat-{session_id}-{now}");
    let result = match observer.as_deref_mut() {
        Some(observer) => app.execute_chat_turn_with_observer(session_id, message, now, observer),
        None => app.execute_chat_turn(session_id, message, now),
    };
    let report = match result {
        Ok(report) => report,
        Err(BootstrapError::Execution(ExecutionError::ApprovalRequired {
            approval_id, ..
        })) => {
            return Ok(ChatSendOutcome::WaitingApproval {
                session_id: session_id.to_string(),
                run_id,
                approval_id,
            });
        }
        Err(error) => return Err(error),
    };
    Ok(ChatSendOutcome::Completed {
        session_id: report.session_id,
        run_id: report.run_id,
        response_id: report.response_id,
        output_text: report.output_text,
    })
}

fn create_mission(
    store: &PersistenceStore,
    id: &str,
    session_id: &str,
    objective: &str,
) -> Result<String, BootstrapError> {
    if store.get_session(session_id)?.is_none() {
        return Err(BootstrapError::MissingRecord {
            kind: "session",
            id: session_id.to_string(),
        });
    }

    let now = unix_timestamp()?;
    let mission = MissionSpec {
        id: id.to_string(),
        session_id: session_id.to_string(),
        objective: objective.to_string(),
        status: MissionStatus::Ready,
        execution_intent: MissionExecutionIntent::Autonomous,
        schedule: MissionSchedule::once(),
        acceptance_criteria: Vec::new(),
        created_at: now,
        updated_at: now,
        completed_at: None,
    };
    let record = MissionRecord::try_from(&mission).map_err(BootstrapError::RecordConversion)?;
    store.put_mission(&record)?;
    Ok(format!(
        "created mission {} session_id={} objective={}",
        record.id, record.session_id, record.objective
    ))
}

fn show_mission(store: &PersistenceStore, id: &str) -> Result<String, BootstrapError> {
    let record = store
        .get_mission(id)?
        .ok_or_else(|| BootstrapError::MissingRecord {
            kind: "mission",
            id: id.to_string(),
        })?;

    Ok(format!(
        "mission id={} session_id={} status={} execution_intent={} objective={} schedule={} acceptance={}",
        record.id,
        record.session_id,
        record.status,
        record.execution_intent,
        record.objective,
        record.schedule_json,
        record.acceptance_json
    ))
}

fn run_mission_tick(app: &App, now: i64) -> Result<String, BootstrapError> {
    let report = app.supervisor_tick(now, &[])?;
    let actions = if report.actions.is_empty() {
        "<none>".to_string()
    } else {
        report
            .actions
            .iter()
            .map(format_supervisor_action)
            .collect::<Vec<_>>()
            .join(" | ")
    };

    Ok(format!(
        "mission tick now={} queued_jobs={} dispatched_jobs={} blocked_jobs={} completed_missions={} budget_remaining={} actions={}",
        now,
        report.queued_jobs,
        report.dispatched_jobs,
        report.blocked_jobs,
        report.completed_missions,
        report.budget_remaining,
        actions
    ))
}

fn show_run(store: &PersistenceStore, id: &str) -> Result<String, BootstrapError> {
    let snapshot = load_run_snapshot(store, id)?;
    Ok(format!(
        "run id={} session_id={} mission_id={} status={} pending_approvals={} delegates={} evidence_refs={} error={}",
        snapshot.id,
        snapshot.session_id,
        snapshot.mission_id.as_deref().unwrap_or("<none>"),
        snapshot.status.as_str(),
        snapshot.pending_approvals.len(),
        snapshot.delegate_runs.len(),
        snapshot.evidence_refs.len(),
        snapshot.error.as_deref().unwrap_or("<none>")
    ))
}

fn show_job(store: &PersistenceStore, id: &str) -> Result<String, BootstrapError> {
    let record = store
        .get_job(id)?
        .ok_or_else(|| BootstrapError::MissingRecord {
            kind: "job",
            id: id.to_string(),
        })?;

    Ok(format!(
        "job id={} mission_id={} run_id={} kind={} status={} input={} result={}",
        record.id,
        record.mission_id,
        record.run_id.as_deref().unwrap_or("<none>"),
        record.kind,
        record.status,
        record.input_json.as_deref().unwrap_or("<none>"),
        record.result_json.as_deref().unwrap_or("<none>")
    ))
}

fn execute_job(app: &App, id: &str, now: i64) -> Result<String, BootstrapError> {
    let report = app.execute_mission_turn_job(id, now)?;
    Ok(format!(
        "job execute id={} run_id={} response_id={} output={}",
        report.job_id, report.run_id, report.response_id, report.output_text
    ))
}

fn list_approvals(store: &PersistenceStore, run_id: &str) -> Result<String, BootstrapError> {
    let snapshot = load_run_snapshot(store, run_id)?;
    if snapshot.pending_approvals.is_empty() {
        return Ok(format!("approval run_id={} none", run_id));
    }

    let approvals = snapshot
        .pending_approvals
        .iter()
        .map(|approval| {
            format!(
                "{} tool_call_id={} reason={}",
                approval.id, approval.tool_call_id, approval.reason
            )
        })
        .collect::<Vec<_>>()
        .join(" | ");
    Ok(format!("approval run_id={} {}", run_id, approvals))
}

fn approve_run(app: &App, run_id: &str, approval_id: &str) -> Result<String, BootstrapError> {
    let report = app.approve_run(run_id, approval_id, unix_timestamp()?)?;
    Ok(format!(
        "approved {} on run {} status={} response_id={} output={} next_approval={}",
        approval_id,
        report.run_id,
        report.run_status.as_str(),
        report.response_id.as_deref().unwrap_or("<none>"),
        report.output_text.as_deref().unwrap_or("<none>"),
        report.approval_id.as_deref().unwrap_or("<none>")
    ))
}

fn list_delegates(store: &PersistenceStore, run_id: &str) -> Result<String, BootstrapError> {
    let snapshot = load_run_snapshot(store, run_id)?;
    if snapshot.delegate_runs.is_empty() {
        return Ok(format!("delegate run_id={} none", run_id));
    }

    let delegates = snapshot
        .delegate_runs
        .iter()
        .map(|delegate| format!("{} label={}", delegate.id, delegate.label))
        .collect::<Vec<_>>()
        .join(" | ");
    Ok(format!("delegate run_id={} {}", run_id, delegates))
}

fn show_verification(store: &PersistenceStore, run_id: &str) -> Result<String, BootstrapError> {
    let snapshot = load_run_snapshot(store, run_id)?;
    let refs = if snapshot.evidence_refs.is_empty() {
        "<none>".to_string()
    } else {
        snapshot.evidence_refs.join(",")
    };
    Ok(format!("verification run_id={} refs={}", run_id, refs))
}

fn run_provider_smoke(app: &App, prompt: &str) -> Result<String, BootstrapError> {
    let driver = app.provider_driver()?;
    let response = driver.complete(&ProviderRequest {
        model: None,
        instructions: Some("Reply tersely.".to_string()),
        messages: vec![ProviderMessage::new(MessageRole::User, prompt)],
        previous_response_id: None,
        continuation_messages: Vec::new(),
        tools: Vec::new(),
        tool_outputs: Vec::new(),
        max_output_tokens: app.config.provider.max_output_tokens,
        stream: ProviderStreamMode::Disabled,
    })?;

    Ok(format!(
        "provider name={} response_id={} model={} finish_reason={} usage_total_tokens={} output={}",
        driver.descriptor().name,
        response.response_id,
        response.model,
        match response.finish_reason {
            FinishReason::Completed => "completed",
            FinishReason::Incomplete => "incomplete",
        },
        response
            .usage
            .map(|usage| usage.total_tokens)
            .unwrap_or_default(),
        response.output_text
    ))
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

fn format_supervisor_action(action: &agent_runtime::scheduler::SupervisorAction) -> String {
    match action {
        agent_runtime::scheduler::SupervisorAction::QueueJob(job) => {
            format!("queue_job:{}", job.id)
        }
        agent_runtime::scheduler::SupervisorAction::DispatchJob { job_id, .. } => {
            format!("dispatch_job:{job_id}")
        }
        agent_runtime::scheduler::SupervisorAction::RequestApproval { job_id, .. } => {
            format!("request_approval:{job_id}")
        }
        agent_runtime::scheduler::SupervisorAction::DeferMission { mission_id, .. } => {
            format!("defer_mission:{mission_id}")
        }
        agent_runtime::scheduler::SupervisorAction::CompleteMission { mission_id } => {
            format!("complete_mission:{mission_id}")
        }
    }
}

fn render_status(app: &App) -> Result<String, BootstrapError> {
    let connection = Connection::open(&app.persistence.stores.metadata_db)?;
    let session_count = count_rows(&connection, "sessions")?;
    let mission_count = count_rows(&connection, "missions")?;
    let run_count = count_rows(&connection, "runs")?;
    let job_count = count_rows(&connection, "jobs")?;

    Ok(format!(
        "status data_dir={} permission_mode={} sessions={} missions={} runs={} jobs={} components={} state_db={}",
        app.config.data_dir.display(),
        app.config.permissions.mode.as_str(),
        session_count,
        mission_count,
        run_count,
        job_count,
        app.runtime.component_count(),
        app.persistence.stores.metadata_db.display()
    ))
}

fn count_rows(connection: &Connection, table: &str) -> Result<i64, BootstrapError> {
    connection
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .map_err(BootstrapError::Sqlite)
}

fn load_run_snapshot(
    store: &PersistenceStore,
    run_id: &str,
) -> Result<RunSnapshot, BootstrapError> {
    let record = store
        .get_run(run_id)?
        .ok_or_else(|| BootstrapError::MissingRecord {
            kind: "run",
            id: run_id.to_string(),
        })?;
    RunSnapshot::try_from(record).map_err(BootstrapError::RecordConversion)
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
