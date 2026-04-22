use super::*;
use crate::bootstrap::SessionPreferencesPatch;
use crate::help::{HelpTopic, parse_help_topic, render_command_usage_error, render_help};

pub(super) trait ChatReplBackend {
    fn show_chat(&self, session_id: &str) -> Result<String, BootstrapError>;
    fn render_agents(&self) -> Result<String, BootstrapError>;
    fn render_agent(&self, identifier: Option<&str>) -> Result<String, BootstrapError>;
    fn select_agent(&self, identifier: &str) -> Result<String, BootstrapError>;
    fn create_agent(
        &self,
        name: &str,
        template_identifier: Option<&str>,
    ) -> Result<String, BootstrapError>;
    fn open_agent_home(&self, identifier: Option<&str>) -> Result<String, BootstrapError>;
    fn render_agent_schedules(&self) -> Result<String, BootstrapError>;
    fn render_agent_schedule(&self, id: &str) -> Result<String, BootstrapError>;
    fn create_agent_schedule(
        &self,
        id: &str,
        interval_seconds: u64,
        prompt: &str,
        agent_identifier: Option<&str>,
    ) -> Result<String, BootstrapError>;
    fn delete_agent_schedule(&self, id: &str) -> Result<String, BootstrapError>;
    fn render_version_info(&self) -> Result<String, BootstrapError>;
    fn update_runtime(&self) -> Result<String, BootstrapError>;
    fn render_system(&self, session_id: &str) -> Result<String, BootstrapError>;
    fn render_context(&self, session_id: &str) -> Result<String, BootstrapError>;
    fn render_plan(&self, session_id: &str) -> Result<String, BootstrapError>;
    fn render_artifacts(&self, session_id: &str) -> Result<String, BootstrapError>;
    fn read_artifact(&self, session_id: &str, artifact_id: &str) -> Result<String, BootstrapError>;
    fn render_active_run(&self, session_id: &str) -> Result<String, BootstrapError>;
    fn cancel_active_run(&self, session_id: &str) -> Result<String, BootstrapError>;
    fn render_active_jobs(&self, session_id: &str) -> Result<String, BootstrapError>;
    fn write_debug_bundle(&self, session_id: &str) -> Result<String, BootstrapError>;
    fn render_session_skills(&self, session_id: &str) -> Result<String, BootstrapError>;
    fn enable_session_skill(
        &self,
        session_id: &str,
        skill_name: &str,
    ) -> Result<String, BootstrapError>;
    fn disable_session_skill(
        &self,
        session_id: &str,
        skill_name: &str,
    ) -> Result<String, BootstrapError>;
    fn update_session_preferences(
        &self,
        session_id: &str,
        patch: SessionPreferencesPatch,
    ) -> Result<(), BootstrapError>;
    fn find_pending_approval(
        &self,
        session_id: &str,
        requested_approval_id: Option<&str>,
    ) -> Result<Option<ReplPendingApproval>, BootstrapError>;
    fn approve_run_with_observer(
        &self,
        run_id: &str,
        approval_id: &str,
        observer: &mut dyn FnMut(ChatExecutionEvent),
    ) -> Result<crate::execution::ApprovalContinuationReport, BootstrapError>;
    fn send_chat_with_observer(
        &self,
        session_id: &str,
        message: &str,
        observer: &mut dyn FnMut(ChatExecutionEvent),
    ) -> Result<ChatSendOutcome, BootstrapError>;
}

impl ChatReplBackend for App {
    fn show_chat(&self, session_id: &str) -> Result<String, BootstrapError> {
        render::show_chat(self, session_id)
    }

    fn render_agents(&self) -> Result<String, BootstrapError> {
        self.render_agents()
    }

    fn render_agent(&self, identifier: Option<&str>) -> Result<String, BootstrapError> {
        self.render_agent_profile(identifier)
    }

    fn select_agent(&self, identifier: &str) -> Result<String, BootstrapError> {
        let profile = self.select_agent_profile(identifier)?;
        Ok(format!("текущий агент: {} ({})", profile.name, profile.id))
    }

    fn create_agent(
        &self,
        name: &str,
        template_identifier: Option<&str>,
    ) -> Result<String, BootstrapError> {
        let profile = self.create_agent_from_template(name, template_identifier)?;
        Ok(format!(
            "создан агент {} ({}) из шаблона {}",
            profile.name,
            profile.id,
            profile.template_kind.as_str()
        ))
    }

    fn open_agent_home(&self, identifier: Option<&str>) -> Result<String, BootstrapError> {
        self.render_agent_home(identifier)
    }

    fn render_agent_schedules(&self) -> Result<String, BootstrapError> {
        App::render_agent_schedules(self)
    }

    fn render_agent_schedule(&self, id: &str) -> Result<String, BootstrapError> {
        App::render_agent_schedule(self, id)
    }

    fn create_agent_schedule(
        &self,
        id: &str,
        interval_seconds: u64,
        prompt: &str,
        agent_identifier: Option<&str>,
    ) -> Result<String, BootstrapError> {
        let schedule =
            App::create_agent_schedule(self, id, interval_seconds, prompt, agent_identifier)?;
        Ok(format!(
            "создано расписание {} agent={} interval={}s",
            schedule.id, schedule.agent_profile_id, schedule.interval_seconds
        ))
    }

    fn delete_agent_schedule(&self, id: &str) -> Result<String, BootstrapError> {
        if App::delete_agent_schedule(self, id)? {
            Ok(format!("расписание {id} удалено"))
        } else {
            Err(BootstrapError::MissingRecord {
                kind: "agent schedule",
                id: id.to_string(),
            })
        }
    }

    fn render_version_info(&self) -> Result<String, BootstrapError> {
        App::render_version_info(self)
    }

    fn update_runtime(&self) -> Result<String, BootstrapError> {
        App::update_runtime_binary(self)
    }

    fn render_system(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.render_system_blocks(session_id)
    }

    fn render_context(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.render_context_state(session_id)
    }

    fn render_plan(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.render_plan(session_id)
    }

    fn render_artifacts(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.render_session_artifacts(session_id)
    }

    fn read_artifact(&self, session_id: &str, artifact_id: &str) -> Result<String, BootstrapError> {
        self.read_session_artifact(session_id, artifact_id)
    }

    fn render_active_run(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.render_active_run(session_id)
    }

    fn cancel_active_run(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.cancel_latest_session_run(session_id, unix_timestamp()?)
    }

    fn render_active_jobs(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.render_session_background_jobs(session_id)
    }

    fn write_debug_bundle(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.write_debug_bundle(session_id)
            .map(|path| path.display().to_string())
    }

    fn render_session_skills(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.render_session_skills(session_id)
    }

    fn enable_session_skill(
        &self,
        session_id: &str,
        skill_name: &str,
    ) -> Result<String, BootstrapError> {
        self.enable_session_skill(session_id, skill_name)?;
        self.render_session_skills(session_id)
    }

    fn disable_session_skill(
        &self,
        session_id: &str,
        skill_name: &str,
    ) -> Result<String, BootstrapError> {
        self.disable_session_skill(session_id, skill_name)?;
        self.render_session_skills(session_id)
    }

    fn update_session_preferences(
        &self,
        session_id: &str,
        patch: SessionPreferencesPatch,
    ) -> Result<(), BootstrapError> {
        App::update_session_preferences(self, session_id, patch).map(|_| ())
    }

    fn find_pending_approval(
        &self,
        session_id: &str,
        requested_approval_id: Option<&str>,
    ) -> Result<Option<ReplPendingApproval>, BootstrapError> {
        find_pending_approval(self, session_id, requested_approval_id)
    }

    fn approve_run_with_observer(
        &self,
        run_id: &str,
        approval_id: &str,
        observer: &mut dyn FnMut(ChatExecutionEvent),
    ) -> Result<crate::execution::ApprovalContinuationReport, BootstrapError> {
        self.approve_run_with_observer(run_id, approval_id, unix_timestamp()?, observer)
    }

    fn send_chat_with_observer(
        &self,
        session_id: &str,
        message: &str,
        observer: &mut dyn FnMut(ChatExecutionEvent),
    ) -> Result<ChatSendOutcome, BootstrapError> {
        send_chat_outcome_with_observer(self, session_id, message, observer)
    }
}

impl ChatReplBackend for DaemonClient {
    fn show_chat(&self, session_id: &str) -> Result<String, BootstrapError> {
        render::show_chat_via_client(self, session_id)
    }

    fn render_agents(&self) -> Result<String, BootstrapError> {
        DaemonClient::render_agents(self)
    }

    fn render_agent(&self, identifier: Option<&str>) -> Result<String, BootstrapError> {
        DaemonClient::render_agent(self, identifier)
    }

    fn select_agent(&self, identifier: &str) -> Result<String, BootstrapError> {
        DaemonClient::select_agent(self, identifier)
    }

    fn create_agent(
        &self,
        name: &str,
        template_identifier: Option<&str>,
    ) -> Result<String, BootstrapError> {
        DaemonClient::create_agent(self, name, template_identifier)
    }

    fn open_agent_home(&self, identifier: Option<&str>) -> Result<String, BootstrapError> {
        DaemonClient::open_agent_home(self, identifier)
    }

    fn render_agent_schedules(&self) -> Result<String, BootstrapError> {
        DaemonClient::render_agent_schedules(self)
    }

    fn render_agent_schedule(&self, id: &str) -> Result<String, BootstrapError> {
        DaemonClient::render_agent_schedule(self, id)
    }

    fn create_agent_schedule(
        &self,
        id: &str,
        interval_seconds: u64,
        prompt: &str,
        agent_identifier: Option<&str>,
    ) -> Result<String, BootstrapError> {
        DaemonClient::create_agent_schedule(self, id, interval_seconds, prompt, agent_identifier)
    }

    fn delete_agent_schedule(&self, id: &str) -> Result<String, BootstrapError> {
        DaemonClient::delete_agent_schedule(self, id)
    }

    fn render_version_info(&self) -> Result<String, BootstrapError> {
        DaemonClient::about(self)
    }

    fn update_runtime(&self) -> Result<String, BootstrapError> {
        DaemonClient::update_runtime(self)
    }

    fn render_system(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.render_system_blocks(session_id)
    }

    fn render_context(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.render_context_state(session_id)
    }

    fn render_plan(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.render_plan(session_id)
    }

    fn render_artifacts(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.render_session_artifacts(session_id)
    }

    fn read_artifact(&self, session_id: &str, artifact_id: &str) -> Result<String, BootstrapError> {
        self.read_session_artifact(session_id, artifact_id)
    }

    fn render_active_run(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.render_active_run(session_id)
    }

    fn cancel_active_run(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.cancel_active_run(session_id)
    }

    fn render_active_jobs(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.render_session_background_jobs(session_id)
    }

    fn write_debug_bundle(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.write_debug_bundle(session_id)
    }

    fn render_session_skills(&self, session_id: &str) -> Result<String, BootstrapError> {
        render::render_session_skills_list(self.session_skills(session_id)?)
    }

    fn enable_session_skill(
        &self,
        session_id: &str,
        skill_name: &str,
    ) -> Result<String, BootstrapError> {
        let skills = self.enable_session_skill(session_id, skill_name)?;
        render::render_session_skills_list(skills)
    }

    fn disable_session_skill(
        &self,
        session_id: &str,
        skill_name: &str,
    ) -> Result<String, BootstrapError> {
        let skills = self.disable_session_skill(session_id, skill_name)?;
        render::render_session_skills_list(skills)
    }

    fn update_session_preferences(
        &self,
        session_id: &str,
        patch: SessionPreferencesPatch,
    ) -> Result<(), BootstrapError> {
        DaemonClient::update_session_preferences(self, session_id, patch).map(|_| ())
    }

    fn find_pending_approval(
        &self,
        session_id: &str,
        requested_approval_id: Option<&str>,
    ) -> Result<Option<ReplPendingApproval>, BootstrapError> {
        Ok(self
            .latest_pending_approval(session_id, requested_approval_id)?
            .map(|pending| ReplPendingApproval {
                run_id: pending.run_id,
                approval_id: pending.approval_id,
            }))
    }

    fn approve_run_with_observer(
        &self,
        run_id: &str,
        approval_id: &str,
        observer: &mut dyn FnMut(ChatExecutionEvent),
    ) -> Result<crate::execution::ApprovalContinuationReport, BootstrapError> {
        self.approve_run_with_control_and_observer(
            run_id,
            approval_id,
            unix_timestamp()?,
            None,
            observer,
        )
    }

    fn send_chat_with_observer(
        &self,
        session_id: &str,
        message: &str,
        observer: &mut dyn FnMut(ChatExecutionEvent),
    ) -> Result<ChatSendOutcome, BootstrapError> {
        send_chat_outcome_via_client_with_observer(self, session_id, message, observer)
    }
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
            ChatExecutionEvent::ProviderLoopProgress { .. } => Ok(()),
            ChatExecutionEvent::ToolStatus {
                tool_name,
                summary,
                status,
            } => self.write_tool_status(&tool_name, &summary, status),
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
            write!(self.output, "размышления: {delta}").map_err(BootstrapError::Stream)?;
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
            write!(self.output, "ассистент: {delta}").map_err(BootstrapError::Stream)?;
            self.assistant_open = true;
        }
        self.assistant_streamed_this_turn = true;
        self.output.flush().map_err(BootstrapError::Stream)
    }

    fn write_tool_status(
        &mut self,
        tool_name: &str,
        summary: &str,
        status: ToolExecutionStatus,
    ) -> Result<(), BootstrapError> {
        self.finish_turn()?;
        let line = if summary.is_empty() || summary == tool_name {
            format!(
                "инструмент: {tool_name} | {}",
                translate_tool_status(&status)
            )
        } else {
            format!(
                "инструмент: {tool_name} | {} | {summary}",
                translate_tool_status(&status)
            )
        };
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

fn translate_tool_status(status: &ToolExecutionStatus) -> &'static str {
    match status {
        ToolExecutionStatus::Requested => "запрошен",
        ToolExecutionStatus::WaitingApproval => "ожидает апрува",
        ToolExecutionStatus::Approved => "подтверждён",
        ToolExecutionStatus::Running => "выполняется",
        ToolExecutionStatus::Completed => "завершён",
        ToolExecutionStatus::Failed => "ошибка",
    }
}

pub(super) fn run_chat_repl<R, W>(
    app: &App,
    session_id: &str,
    input: &mut R,
    output: &mut W,
) -> Result<(), BootstrapError>
where
    R: BufRead,
    W: Write,
{
    run_chat_repl_with_backend(app, session_id, input, output)
}

pub(super) fn run_chat_repl_with_backend<B, R, W>(
    backend: &B,
    session_id: &str,
    input: &mut R,
    output: &mut W,
) -> Result<(), BootstrapError>
where
    B: ChatReplBackend,
    R: BufRead,
    W: Write,
{
    writeln!(output, "{}", crate::about::short_version_label()).map_err(BootstrapError::Stream)?;
    writeln!(output, "чатовый режим session_id={session_id}").map_err(BootstrapError::Stream)?;
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
            writeln!(
                renderer.output,
                "выход из чатового режима session_id={session_id}"
            )
            .map_err(BootstrapError::Stream)?;
            return Ok(());
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let command_result = (|| -> Result<Option<()>, BootstrapError> {
            match canonical_repl_command(trimmed) {
                Some("/exit") => {
                    renderer.finish_turn()?;
                    writeln!(
                        renderer.output,
                        "выход из чатового режима session_id={session_id}"
                    )
                    .map_err(BootstrapError::Stream)?;
                    return Ok(Some(()));
                }
                Some("/help") => {
                    renderer.finish_turn()?;
                    let topic = parse_help_topic(split_command_arg(trimmed)).map_err(|reason| {
                        BootstrapError::Usage {
                            reason: render_command_usage_error("/help", reason.as_str()),
                        }
                    })?;
                    writeln!(renderer.output, "{}", render_help(topic))
                        .map_err(BootstrapError::Stream)?;
                }
                Some("/agents") => {
                    renderer.finish_turn()?;
                    let agents = backend.render_agents()?;
                    writeln!(renderer.output, "{agents}").map_err(BootstrapError::Stream)?;
                }
                Some("/agent") => {
                    renderer.finish_turn()?;
                    let message = handle_agent_command(backend, split_command_arg(trimmed))?;
                    writeln!(renderer.output, "{message}").map_err(BootstrapError::Stream)?;
                }
                Some("/schedules") => {
                    renderer.finish_turn()?;
                    let rendered = backend.render_agent_schedules()?;
                    writeln!(renderer.output, "{rendered}").map_err(BootstrapError::Stream)?;
                }
                Some("/schedule") => {
                    renderer.finish_turn()?;
                    let message = handle_schedule_command(backend, split_command_arg(trimmed))?;
                    writeln!(renderer.output, "{message}").map_err(BootstrapError::Stream)?;
                }
                Some("/version") => {
                    renderer.finish_turn()?;
                    let about = backend.render_version_info()?;
                    writeln!(renderer.output, "{about}").map_err(BootstrapError::Stream)?;
                }
                Some("/update") => {
                    renderer.finish_turn()?;
                    let message = backend.update_runtime()?;
                    writeln!(renderer.output, "{message}").map_err(BootstrapError::Stream)?;
                }
                Some("/show") => {
                    renderer.finish_turn()?;
                    let transcript = backend.show_chat(session_id)?;
                    writeln!(renderer.output, "{transcript}").map_err(BootstrapError::Stream)?;
                }
                Some("/system") => {
                    renderer.finish_turn()?;
                    let system = backend.render_system(session_id)?;
                    writeln!(renderer.output, "{system}").map_err(BootstrapError::Stream)?;
                }
                Some("/context") => {
                    renderer.finish_turn()?;
                    let context = backend.render_context(session_id)?;
                    writeln!(renderer.output, "{context}").map_err(BootstrapError::Stream)?;
                }
                Some("/plan") => {
                    renderer.finish_turn()?;
                    let plan = backend.render_plan(session_id)?;
                    writeln!(renderer.output, "{plan}").map_err(BootstrapError::Stream)?;
                }
                Some("/status") => {
                    renderer.finish_turn()?;
                    let run = backend.render_active_run(session_id)?;
                    writeln!(renderer.output, "{run}").map_err(BootstrapError::Stream)?;
                }
                Some("/processes") => {
                    renderer.finish_turn()?;
                    let run = backend.render_active_run(session_id)?;
                    writeln!(renderer.output, "{run}").map_err(BootstrapError::Stream)?;
                }
                Some("/pause") => {
                    renderer.finish_turn()?;
                    let message = backend.cancel_active_run(session_id)?;
                    writeln!(
                        renderer.output,
                        "пауза пока реализована как операторская остановка: {message}"
                    )
                    .map_err(BootstrapError::Stream)?;
                }
                Some("/stop") => {
                    renderer.finish_turn()?;
                    let message = backend.cancel_active_run(session_id)?;
                    writeln!(renderer.output, "{message}").map_err(BootstrapError::Stream)?;
                }
                Some("/jobs") => {
                    renderer.finish_turn()?;
                    let jobs = backend.render_active_jobs(session_id)?;
                    writeln!(renderer.output, "{jobs}").map_err(BootstrapError::Stream)?;
                }
                Some("/artifacts") => {
                    renderer.finish_turn()?;
                    let artifacts = backend.render_artifacts(session_id)?;
                    writeln!(renderer.output, "{artifacts}").map_err(BootstrapError::Stream)?;
                }
                Some("/artifact") => {
                    renderer.finish_turn()?;
                    let artifact_id =
                        split_command_arg(trimmed).ok_or_else(|| BootstrapError::Usage {
                            reason: render_command_usage_error(
                                "/artifact",
                                "не хватает аргументов",
                            ),
                        })?;
                    let artifact = backend.read_artifact(session_id, artifact_id)?;
                    writeln!(renderer.output, "{artifact}").map_err(BootstrapError::Stream)?;
                }
                Some("/debug") => {
                    renderer.finish_turn()?;
                    let path = backend.write_debug_bundle(session_id)?;
                    writeln!(renderer.output, "отладочный пакет сохранён: {path}")
                        .map_err(BootstrapError::Stream)?;
                }
                Some("/settings") => {
                    renderer.finish_turn()?;
                    writeln!(renderer.output, "{}", render_help(HelpTopic::Settings))
                        .map_err(BootstrapError::Stream)?;
                }
                Some("/skills") => {
                    renderer.finish_turn()?;
                    let skills = backend.render_session_skills(session_id)?;
                    writeln!(renderer.output, "{skills}").map_err(BootstrapError::Stream)?;
                }
                Some("/completion") => {
                    renderer.finish_turn()?;
                    let value =
                        split_command_arg(trimmed).ok_or_else(|| BootstrapError::Usage {
                            reason: render_command_usage_error(
                                "/completion",
                                "не хватает аргументов",
                            ),
                        })?;
                    let completion_nudges = parse_completion_nudges(value)?;
                    backend.update_session_preferences(
                        session_id,
                        SessionPreferencesPatch {
                            completion_nudges: Some(completion_nudges),
                            ..SessionPreferencesPatch::default()
                        },
                    )?;
                    writeln!(
                        renderer.output,
                        "режим доводки: {}",
                        describe_completion_mode(completion_nudges)
                    )
                    .map_err(BootstrapError::Stream)?;
                }
                Some("/autoapprove") => {
                    renderer.finish_turn()?;
                    let value =
                        split_command_arg(trimmed).ok_or_else(|| BootstrapError::Usage {
                            reason: render_command_usage_error(
                                "/autoapprove",
                                "не хватает аргументов",
                            ),
                        })?;
                    let auto_approve = parse_auto_approve(value)?;
                    backend.update_session_preferences(
                        session_id,
                        SessionPreferencesPatch {
                            auto_approve: Some(auto_approve),
                            ..SessionPreferencesPatch::default()
                        },
                    )?;
                    writeln!(
                        renderer.output,
                        "автоапрув {}",
                        if auto_approve {
                            "включён"
                        } else {
                            "выключен"
                        }
                    )
                    .map_err(BootstrapError::Stream)?;
                }
                Some("/enable") => {
                    renderer.finish_turn()?;
                    let skill_name =
                        split_command_arg(trimmed).ok_or_else(|| BootstrapError::Usage {
                            reason: render_command_usage_error("/enable", "не хватает аргументов"),
                        })?;
                    let skills = backend.enable_session_skill(session_id, skill_name)?;
                    writeln!(renderer.output, "{skills}").map_err(BootstrapError::Stream)?;
                }
                Some("/disable") => {
                    renderer.finish_turn()?;
                    let skill_name =
                        split_command_arg(trimmed).ok_or_else(|| BootstrapError::Usage {
                            reason: render_command_usage_error("/disable", "не хватает аргументов"),
                        })?;
                    let skills = backend.disable_session_skill(session_id, skill_name)?;
                    writeln!(renderer.output, "{skills}").map_err(BootstrapError::Stream)?;
                }
                Some("/approve") => {
                    let requested = trimmed.split_whitespace().nth(1).map(ToString::to_string);
                    let current =
                        match backend.find_pending_approval(session_id, requested.as_deref())? {
                            Some(current) => current,
                            None => {
                                renderer.finish_turn()?;
                                writeln!(
                                    renderer.output,
                                    "для session_id={session_id} нет ожидающего апрува"
                                )
                                .map_err(BootstrapError::Stream)?;
                                return Ok(None);
                            }
                        };
                    let approval_id = current.approval_id.clone();
                    renderer.begin_turn();
                    let mut emit_error = None;
                    let mut emit = |event| {
                        if emit_error.is_none() {
                            emit_error = renderer.emit(event).err();
                        }
                    };
                    let report = backend.approve_run_with_observer(
                        &current.run_id,
                        &approval_id,
                        &mut emit,
                    )?;
                    if let Some(error) = emit_error {
                        return Err(error);
                    }
                    renderer.finish_turn()?;
                    if let Some(text) = report.output_text.as_deref() {
                        if text.is_empty() || renderer.assistant_streamed_this_turn() {
                            return Ok(None);
                        }
                        writeln!(renderer.output, "ассистент: {text}")
                            .map_err(BootstrapError::Stream)?;
                    }
                }
                _ => {
                    let message = trimmed;
                    if backend.find_pending_approval(session_id, None)?.is_some() {
                        renderer.finish_turn()?;
                        writeln!(
                            renderer.output,
                            "сначала завершите ожидающий апрув, потом отправляйте новое сообщение"
                        )
                        .map_err(BootstrapError::Stream)?;
                        return Ok(None);
                    }

                    renderer.begin_turn();
                    let mut emit_error = None;
                    let mut emit = |event| {
                        if emit_error.is_none() {
                            emit_error = renderer.emit(event).err();
                        }
                    };
                    match backend.send_chat_with_observer(session_id, message, &mut emit)? {
                        ChatSendOutcome::Completed { output_text, .. } => {
                            if let Some(error) = emit_error {
                                return Err(error);
                            }
                            renderer.finish_turn()?;
                            if output_text.is_empty() || renderer.assistant_streamed_this_turn() {
                                return Ok(None);
                            }
                            writeln!(renderer.output, "ассистент: {output_text}")
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
            Ok(None)
        })();

        match command_result {
            Ok(Some(())) => return Ok(()),
            Ok(None) => {}
            Err(BootstrapError::Usage { reason }) => {
                renderer.finish_turn()?;
                writeln!(renderer.output, "{reason}").map_err(BootstrapError::Stream)?;
            }
            Err(error) => return Err(error),
        }
    }
}

pub(super) fn send_chat_outcome(
    app: &App,
    session_id: &str,
    message: &str,
) -> Result<ChatSendOutcome, BootstrapError> {
    let mut observer = None;
    send_chat_outcome_internal(app, session_id, message, &mut observer)
}

pub(super) fn send_chat_outcome_with_observer(
    app: &App,
    session_id: &str,
    message: &str,
    observer: &mut dyn FnMut(ChatExecutionEvent),
) -> Result<ChatSendOutcome, BootstrapError> {
    let mut observer = Some(observer);
    send_chat_outcome_internal(app, session_id, message, &mut observer)
}

pub(super) fn send_chat_outcome_via_client(
    client: &DaemonClient,
    session_id: &str,
    message: &str,
) -> Result<ChatSendOutcome, BootstrapError> {
    let mut observer = None;
    send_chat_outcome_via_client_internal(client, session_id, message, &mut observer)
}

pub(super) fn send_chat_outcome_via_client_with_observer(
    client: &DaemonClient,
    session_id: &str,
    message: &str,
    observer: &mut dyn FnMut(ChatExecutionEvent),
) -> Result<ChatSendOutcome, BootstrapError> {
    let mut observer = Some(observer);
    send_chat_outcome_via_client_internal(client, session_id, message, &mut observer)
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
                run_id: Some(run_id),
                approval_id,
            });
        }
        Err(error) => return Err(error),
    };
    Ok(ChatSendOutcome::Completed {
        session_id: report.session_id,
        run_id: Some(report.run_id),
        response_id: report.response_id,
        output_text: report.output_text,
    })
}

fn send_chat_outcome_via_client_internal(
    client: &DaemonClient,
    session_id: &str,
    message: &str,
    observer: &mut Option<&mut dyn FnMut(ChatExecutionEvent)>,
) -> Result<ChatSendOutcome, BootstrapError> {
    let now = unix_timestamp()?;
    let result = match observer.as_deref_mut() {
        Some(observer) => client
            .execute_chat_turn_with_control_and_observer(session_id, message, now, None, observer),
        None => {
            let mut noop = |_| {};
            client.execute_chat_turn_with_control_and_observer(
                session_id, message, now, None, &mut noop,
            )
        }
    };
    let report = match result {
        Ok(report) => report,
        Err(BootstrapError::Execution(ExecutionError::ApprovalRequired {
            approval_id, ..
        })) => {
            return Ok(ChatSendOutcome::WaitingApproval {
                session_id: session_id.to_string(),
                run_id: None,
                approval_id,
            });
        }
        Err(error) => return Err(error),
    };
    Ok(ChatSendOutcome::Completed {
        session_id: report.session_id,
        run_id: Some(report.run_id),
        response_id: report.response_id,
        output_text: report.output_text,
    })
}

fn canonical_repl_command(raw: &str) -> Option<&'static str> {
    let command = raw
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim_end_matches(['\\', '/']);
    match command {
        "/exit" | "\\выход" => Some("/exit"),
        "/help" | "\\помощь" => Some("/help"),
        "/agents" | "\\агенты" => Some("/agents"),
        "/agent" | "\\агент" => Some("/agent"),
        "/schedules" | "\\расписания" => Some("/schedules"),
        "/schedule" | "\\расписание" => Some("/schedule"),
        "/version" | "/версия" | "\\версия" => Some("/version"),
        "/update" | "/обновить" | "\\обновить" => Some("/update"),
        "/settings" | "\\настройки" => Some("/settings"),
        "/show" | "\\показать" => Some("/show"),
        "/system" | "/система" | "\\система" => Some("/system"),
        "/context" | "\\контекст" => Some("/context"),
        "/plan" | "\\план" => Some("/plan"),
        "/status" | "\\статус" => Some("/status"),
        "/processes" | "\\процессы" => Some("/processes"),
        "/pause" | "\\пауза" => Some("/pause"),
        "/stop" | "\\стоп" => Some("/stop"),
        "/jobs" | "\\задачи" => Some("/jobs"),
        "/artifacts" | "/артефакты" | "\\артефакты" => Some("/artifacts"),
        "/artifact" | "/артефакт" | "\\артефакт" => Some("/artifact"),
        "/debug" | "\\отладка" => Some("/debug"),
        "/completion" | "\\доводка" => Some("/completion"),
        "/autoapprove" | "\\автоапрув" => Some("/autoapprove"),
        "/skills" | "\\скиллы" => Some("/skills"),
        "/enable" | "\\включить" => Some("/enable"),
        "/disable" | "\\выключить" => Some("/disable"),
        "/approve" | "\\апрув" => Some("/approve"),
        _ => None,
    }
}

fn handle_agent_command<B>(backend: &B, raw: Option<&str>) -> Result<String, BootstrapError>
where
    B: ChatReplBackend,
{
    let raw = raw.unwrap_or_default().trim();
    let (action, tail) = match raw.split_once(' ') {
        Some((action, tail)) => (action.trim(), tail.trim()),
        None => (raw, ""),
    };

    match action {
        "показать" | "show" | "" => backend.render_agent(split_command_arg(raw)),
        "выбрать" | "select" => {
            backend.select_agent(split_command_arg(raw).ok_or_else(|| BootstrapError::Usage {
                reason: render_command_usage_error("/agent", "не хватает аргументов"),
            })?)
        }
        "создать" | "create" => {
            let spec = tail.trim();
            if spec.is_empty() {
                return Err(BootstrapError::Usage {
                    reason: render_command_usage_error("/agent", "не хватает аргументов"),
                });
            }
            let (name, template_identifier) = parse_agent_create_spec(spec)?;
            backend.create_agent(&name, template_identifier.as_deref())
        }
        "открыть" | "open" => backend.open_agent_home(split_command_arg(raw)),
        _ => Err(BootstrapError::Usage {
            reason: render_command_usage_error(
                "/agent",
                "неизвестная подкоманда агента; ожидается показать|выбрать|создать|открыть",
            ),
        }),
    }
}

fn handle_schedule_command<B>(backend: &B, raw: Option<&str>) -> Result<String, BootstrapError>
where
    B: ChatReplBackend,
{
    let raw = raw.unwrap_or_default().trim();
    let (action, tail) = match raw.split_once(' ') {
        Some((action, tail)) => (action.trim(), tail.trim()),
        None => (raw, ""),
    };

    match action {
        "" => backend.render_agent_schedules(),
        "показать" | "show" => {
            backend.render_agent_schedule(split_command_arg(raw).ok_or_else(|| {
                BootstrapError::Usage {
                    reason: render_command_usage_error("/schedule", "не хватает аргументов"),
                }
            })?)
        }
        "создать" | "create" => {
            let spec = tail.trim();
            if spec.is_empty() {
                return Err(BootstrapError::Usage {
                    reason: render_command_usage_error("/schedule", "не хватает аргументов"),
                });
            }
            let (id, interval_seconds, agent_identifier, prompt) =
                parse_schedule_create_spec(spec)?;
            backend.create_agent_schedule(
                &id,
                interval_seconds,
                &prompt,
                agent_identifier.as_deref(),
            )
        }
        "удалить" | "delete" | "remove" => {
            backend.delete_agent_schedule(split_command_arg(raw).ok_or_else(|| {
                BootstrapError::Usage {
                    reason: render_command_usage_error("/schedule", "не хватает аргументов"),
                }
            })?)
        }
        _ => Err(BootstrapError::Usage {
            reason: render_command_usage_error(
                "/schedule",
                "неизвестная подкоманда расписания; ожидается показать|создать|удалить",
            ),
        }),
    }
}

fn parse_agent_create_spec(raw: &str) -> Result<(String, Option<String>), BootstrapError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(BootstrapError::Usage {
            reason: render_command_usage_error("/agent", "не хватает аргументов"),
        });
    }

    for delimiter in [" из ", " from "] {
        if let Some((name, template)) = trimmed.split_once(delimiter) {
            let name = name.trim().to_string();
            let template = template.trim().to_string();
            if !name.is_empty() && !template.is_empty() {
                return Ok((name, Some(template)));
            }
        }
    }

    Ok((trimmed.to_string(), None))
}

fn parse_schedule_create_spec(
    raw: &str,
) -> Result<(String, u64, Option<String>, String), BootstrapError> {
    let trimmed = raw.trim();
    let Some((head, prompt)) = trimmed.split_once("::") else {
        return Err(BootstrapError::Usage {
            reason: render_command_usage_error(
                "/schedule",
                "не хватает prompt; используйте формат с разделителем ::",
            ),
        });
    };
    let prompt = prompt.trim().to_string();
    if prompt.is_empty() {
        return Err(BootstrapError::Usage {
            reason: render_command_usage_error("/schedule", "prompt не должен быть пустым"),
        });
    }

    let mut parts = head.split_whitespace();
    let Some(id) = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Err(BootstrapError::Usage {
            reason: render_command_usage_error("/schedule", "не хватает id расписания"),
        });
    };
    let Some(interval_raw) = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Err(BootstrapError::Usage {
            reason: render_command_usage_error("/schedule", "не хватает interval_seconds"),
        });
    };
    let interval_seconds = interval_raw
        .parse::<u64>()
        .map_err(|_| BootstrapError::Usage {
            reason: render_command_usage_error(
                "/schedule",
                "interval_seconds должен быть положительным целым числом",
            ),
        })?;
    if interval_seconds == 0 {
        return Err(BootstrapError::Usage {
            reason: render_command_usage_error(
                "/schedule",
                "interval_seconds должен быть больше нуля",
            ),
        });
    }

    let remainder = parts.collect::<Vec<_>>();
    let agent_identifier = match remainder.as_slice() {
        [] => None,
        [value] => parse_schedule_agent_override(value)?,
        _ => {
            return Err(BootstrapError::Usage {
                reason: render_command_usage_error(
                    "/schedule",
                    "лишние аргументы; после interval_seconds допускается только агент=<id>",
                ),
            });
        }
    };

    Ok((id.to_string(), interval_seconds, agent_identifier, prompt))
}

fn parse_schedule_agent_override(raw: &str) -> Result<Option<String>, BootstrapError> {
    for prefix in ["agent=", "агент="] {
        if let Some(value) = raw.strip_prefix(prefix) {
            let value = value.trim();
            if value.is_empty() {
                return Err(BootstrapError::Usage {
                    reason: render_command_usage_error(
                        "/schedule",
                        "после agent= должен быть id или имя агента",
                    ),
                });
            }
            return Ok(Some(value.to_string()));
        }
    }

    Err(BootstrapError::Usage {
        reason: render_command_usage_error(
            "/schedule",
            "неподдерживаемый override агента; используйте agent=<id> или агент=<id>",
        ),
    })
}

fn split_command_arg(raw: &str) -> Option<&str> {
    raw.split_once(char::is_whitespace)
        .map(|(_, rest)| rest.trim())
        .filter(|value| !value.is_empty())
}

fn parse_completion_nudges(raw: &str) -> Result<Option<u32>, BootstrapError> {
    let trimmed = raw.trim();
    if matches!(trimmed, "off" | "выкл" | "disable") {
        return Ok(None);
    }
    trimmed
        .parse::<u32>()
        .map(Some)
        .map_err(|_| BootstrapError::Usage {
            reason: render_command_usage_error(
                "/completion",
                &format!(
                    "неподдерживаемый режим доводки {trimmed}; ожидается выкл или неотрицательное число"
                ),
            ),
        })
}

fn describe_completion_mode(completion_nudges: Option<u32>) -> String {
    match completion_nudges {
        None => "выключен".to_string(),
        Some(0) => "включён: после первой ранней остановки сразу нужен апрув оператора".to_string(),
        Some(value) => format!("включён: {value} автоматических пинка перед апрувом"),
    }
}

fn parse_auto_approve(raw: &str) -> Result<bool, BootstrapError> {
    match raw.trim() {
        "on" | "1" | "yes" | "да" | "вкл" | "enable" => Ok(true),
        "off" | "0" | "no" | "нет" | "выкл" | "disable" => Ok(false),
        value => Err(BootstrapError::Usage {
            reason: render_command_usage_error(
                "/autoapprove",
                &format!("неподдерживаемый режим автоапрува {value}; ожидается вкл|выкл"),
            ),
        }),
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
