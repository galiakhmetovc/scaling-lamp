use crate::agent::AgentScheduleDeliveryMode;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::error::Error;
use std::fmt;

use super::parse_repair::{
    CONTINUE_LATER_ENUM_REPAIRS, EnumLikeFieldRepair, KNOWLEDGE_READ_ENUM_REPAIRS,
    SCHEDULE_ENUM_REPAIRS, SESSION_READ_ENUM_REPAIRS, SESSION_WAIT_ENUM_REPAIRS,
    repair_bare_enum_like_values,
};
use super::{
    KnowledgeReadMode, McpCallInput, ProcessOutputStream, SessionReadMode, ToolCall, ToolName,
    format_exec_command_display, normalize_tool_path,
};

#[derive(Debug)]
pub enum ToolCallParseError {
    UnknownTool {
        name: String,
    },
    InvalidArguments {
        name: String,
        source: serde_json::Error,
    },
}

impl ToolCall {
    fn invalid_arguments_error(name: &str, source: serde_json::Error) -> ToolCallParseError {
        ToolCallParseError::InvalidArguments {
            name: name.to_string(),
            source,
        }
    }

    fn parse_arguments_with_enum_repair<T: DeserializeOwned>(
        name: &str,
        arguments: &str,
        repairs: &[EnumLikeFieldRepair],
    ) -> Result<T, ToolCallParseError> {
        match serde_json::from_str(arguments) {
            Ok(parsed) => Ok(parsed),
            Err(source) => {
                if let Some(repaired) = repair_bare_enum_like_values(arguments, repairs)
                    && let Ok(parsed) = serde_json::from_str(&repaired)
                {
                    return Ok(parsed);
                }
                Err(Self::invalid_arguments_error(name, source))
            }
        }
    }

    pub fn name(&self) -> ToolName {
        match self {
            Self::FsRead(_) => ToolName::FsRead,
            Self::FsWrite(_) => ToolName::FsWrite,
            Self::FsPatch(_) => ToolName::FsPatch,
            Self::FsReadText(_) => ToolName::FsReadText,
            Self::FsReadLines(_) => ToolName::FsReadLines,
            Self::FsSearchText(_) => ToolName::FsSearchText,
            Self::FsFindInFiles(_) => ToolName::FsFindInFiles,
            Self::FsWriteText(_) => ToolName::FsWriteText,
            Self::FsPatchText(_) => ToolName::FsPatchText,
            Self::FsReplaceLines(_) => ToolName::FsReplaceLines,
            Self::FsInsertText(_) => ToolName::FsInsertText,
            Self::FsMkdir(_) => ToolName::FsMkdir,
            Self::FsMove(_) => ToolName::FsMove,
            Self::FsTrash(_) => ToolName::FsTrash,
            Self::FsList(_) => ToolName::FsList,
            Self::FsGlob(_) => ToolName::FsGlob,
            Self::FsSearch(_) => ToolName::FsSearch,
            Self::WebFetch(_) => ToolName::WebFetch,
            Self::WebSearch(_) => ToolName::WebSearch,
            Self::ExecStart(_) => ToolName::ExecStart,
            Self::ExecReadOutput(_) => ToolName::ExecReadOutput,
            Self::ExecWait(_) => ToolName::ExecWait,
            Self::ExecKill(_) => ToolName::ExecKill,
            Self::PlanRead(_) => ToolName::PlanRead,
            Self::PlanWrite(_) => ToolName::PlanWrite,
            Self::InitPlan(_) => ToolName::InitPlan,
            Self::AddTask(_) => ToolName::AddTask,
            Self::SetTaskStatus(_) => ToolName::SetTaskStatus,
            Self::AddTaskNote(_) => ToolName::AddTaskNote,
            Self::EditTask(_) => ToolName::EditTask,
            Self::PlanSnapshot(_) => ToolName::PlanSnapshot,
            Self::PlanLint(_) => ToolName::PlanLint,
            Self::PromptBudgetRead(_) => ToolName::PromptBudgetRead,
            Self::PromptBudgetUpdate(_) => ToolName::PromptBudgetUpdate,
            Self::AutonomyStateRead(_) => ToolName::AutonomyStateRead,
            Self::SkillList(_) => ToolName::SkillList,
            Self::SkillRead(_) => ToolName::SkillRead,
            Self::SkillEnable(_) => ToolName::SkillEnable,
            Self::SkillDisable(_) => ToolName::SkillDisable,
            Self::ArtifactRead(_) => ToolName::ArtifactRead,
            Self::ArtifactSearch(_) => ToolName::ArtifactSearch,
            Self::ArtifactPin(_) => ToolName::ArtifactPin,
            Self::ArtifactUnpin(_) => ToolName::ArtifactUnpin,
            Self::DeliverFile(_) => ToolName::DeliverFile,
            Self::KnowledgeSearch(_) => ToolName::KnowledgeSearch,
            Self::KnowledgeRead(_) => ToolName::KnowledgeRead,
            Self::SessionSearch(_) => ToolName::SessionSearch,
            Self::SessionRead(_) => ToolName::SessionRead,
            Self::SessionWait(_) => ToolName::SessionWait,
            Self::McpCall(_) => ToolName::McpCall,
            Self::McpSearchResources(_) => ToolName::McpSearchResources,
            Self::McpReadResource(_) => ToolName::McpReadResource,
            Self::McpSearchPrompts(_) => ToolName::McpSearchPrompts,
            Self::McpGetPrompt(_) => ToolName::McpGetPrompt,
            Self::AgentList(_) => ToolName::AgentList,
            Self::AgentRead(_) => ToolName::AgentRead,
            Self::AgentCreate(_) => ToolName::AgentCreate,
            Self::ContinueLater(_) => ToolName::ContinueLater,
            Self::ScheduleList(_) => ToolName::ScheduleList,
            Self::ScheduleRead(_) => ToolName::ScheduleRead,
            Self::ScheduleCreate(_) => ToolName::ScheduleCreate,
            Self::ScheduleUpdate(_) => ToolName::ScheduleUpdate,
            Self::ScheduleDelete(_) => ToolName::ScheduleDelete,
            Self::MessageAgent(_) => ToolName::MessageAgent,
            Self::GrantAgentChainContinuation(_) => ToolName::GrantAgentChainContinuation,
        }
    }

    pub fn scope_target(&self) -> Option<String> {
        match self {
            Self::FsRead(input) => Some(normalize_tool_path(&input.path)),
            Self::FsWrite(input) => Some(normalize_tool_path(&input.path)),
            Self::FsPatch(input) => Some(normalize_tool_path(&input.path)),
            Self::FsReadText(input) => Some(normalize_tool_path(&input.path)),
            Self::FsReadLines(input) => Some(normalize_tool_path(&input.path)),
            Self::FsSearchText(input) => Some(normalize_tool_path(&input.path)),
            Self::FsFindInFiles(_) => None,
            Self::FsWriteText(input) => Some(normalize_tool_path(&input.path)),
            Self::FsPatchText(input) => Some(normalize_tool_path(&input.path)),
            Self::FsReplaceLines(input) => Some(normalize_tool_path(&input.path)),
            Self::FsInsertText(input) => Some(normalize_tool_path(&input.path)),
            Self::FsMkdir(input) => Some(normalize_tool_path(&input.path)),
            Self::FsMove(input) => Some(normalize_tool_path(&input.dest)),
            Self::FsTrash(input) => Some(normalize_tool_path(&input.path)),
            Self::FsList(input) => Some(normalize_tool_path(&input.path)),
            Self::FsGlob(input) => Some(normalize_tool_path(&input.path)),
            Self::FsSearch(input) => Some(normalize_tool_path(&input.path)),
            Self::WebFetch(input) => Some(input.url.clone()),
            Self::WebSearch(_) => None,
            Self::ExecStart(input) => input.cwd.clone(),
            Self::ExecReadOutput(_) | Self::ExecWait(_) | Self::ExecKill(_) => None,
            Self::PlanRead(_)
            | Self::PlanWrite(_)
            | Self::InitPlan(_)
            | Self::AddTask(_)
            | Self::SetTaskStatus(_)
            | Self::AddTaskNote(_)
            | Self::EditTask(_)
            | Self::PlanSnapshot(_)
            | Self::PlanLint(_)
            | Self::PromptBudgetRead(_)
            | Self::PromptBudgetUpdate(_)
            | Self::AutonomyStateRead(_)
            | Self::SkillList(_)
            | Self::SkillRead(_)
            | Self::SkillEnable(_)
            | Self::SkillDisable(_)
            | Self::DeliverFile(_)
            | Self::KnowledgeSearch(_)
            | Self::KnowledgeRead(_)
            | Self::SessionSearch(_)
            | Self::SessionRead(_)
            | Self::SessionWait(_)
            | Self::McpSearchResources(_)
            | Self::McpSearchPrompts(_)
            | Self::AgentList(_)
            | Self::AgentRead(_)
            | Self::AgentCreate(_)
            | Self::ContinueLater(_)
            | Self::ScheduleList(_)
            | Self::ScheduleRead(_)
            | Self::ScheduleCreate(_)
            | Self::ScheduleUpdate(_)
            | Self::ScheduleDelete(_)
            | Self::MessageAgent(_)
            | Self::GrantAgentChainContinuation(_) => None,
            Self::McpCall(input) => Some(input.exposed_name.clone()),
            Self::McpReadResource(input) => Some(format!("{}:{}", input.connector_id, input.uri)),
            Self::McpGetPrompt(input) => Some(format!("{}:{}", input.connector_id, input.name)),
            Self::ArtifactRead(input) => Some(input.artifact_id.clone()),
            Self::ArtifactPin(input) | Self::ArtifactUnpin(input) => {
                Some(input.artifact_id.clone())
            }
            Self::ArtifactSearch(_) => None,
        }
    }

    pub fn summary(&self) -> String {
        match self {
            Self::FsRead(input) => format!("fs_read path={}", normalize_tool_path(&input.path)),
            Self::FsWrite(input) => format!(
                "fs_write path={} bytes={}",
                normalize_tool_path(&input.path),
                input.content.len()
            ),
            Self::FsPatch(input) => format!(
                "fs_patch path={} edits={}",
                normalize_tool_path(&input.path),
                input.edits.len()
            ),
            Self::FsReadText(input) => {
                format!("fs_read_text path={}", normalize_tool_path(&input.path))
            }
            Self::FsReadLines(input) => format!(
                "fs_read_lines path={} start_line={} end_line={}",
                normalize_tool_path(&input.path),
                input.start_line,
                input.end_line
            ),
            Self::FsSearchText(input) => format!(
                "fs_search_text path={} query={}",
                normalize_tool_path(&input.path),
                input.query
            ),
            Self::FsFindInFiles(input) => format!(
                "fs_find_in_files query={} glob={} limit={}",
                input.query,
                input.glob.as_deref().unwrap_or("*"),
                input.limit.unwrap_or(0)
            ),
            Self::FsWriteText(input) => format!(
                "fs_write_text path={} mode={} bytes={}",
                normalize_tool_path(&input.path),
                input.mode.as_str(),
                input.content.len()
            ),
            Self::FsPatchText(input) => {
                format!("fs_patch_text path={}", normalize_tool_path(&input.path))
            }
            Self::FsReplaceLines(input) => format!(
                "fs_replace_lines path={} start_line={} end_line={}",
                normalize_tool_path(&input.path),
                input.start_line,
                input.end_line
            ),
            Self::FsInsertText(input) => format!(
                "fs_insert_text path={} position={} line={}",
                normalize_tool_path(&input.path),
                input.position,
                input
                    .line
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string())
            ),
            Self::FsMkdir(input) => format!("fs_mkdir path={}", normalize_tool_path(&input.path)),
            Self::FsMove(input) => format!(
                "fs_move src={} dest={}",
                normalize_tool_path(&input.src),
                normalize_tool_path(&input.dest)
            ),
            Self::FsTrash(input) => format!("fs_trash path={}", normalize_tool_path(&input.path)),
            Self::FsList(input) => format!(
                "fs_list path={} recursive={}",
                normalize_tool_path(&input.path),
                input.recursive
            ),
            Self::FsGlob(input) => format!(
                "fs_glob path={} pattern={}",
                normalize_tool_path(&input.path),
                input.pattern
            ),
            Self::FsSearch(input) => format!(
                "fs_search path={} query={}",
                normalize_tool_path(&input.path),
                input.query
            ),
            Self::WebFetch(input) => format!("web_fetch url={}", input.url),
            Self::WebSearch(input) => {
                format!("web_search query={} limit={}", input.query, input.limit)
            }
            Self::ExecStart(input) => {
                format!(
                    "exec_start cwd={} command={}",
                    input.cwd.as_deref().unwrap_or("."),
                    format_exec_command_display(input.executable.as_str(), &input.args)
                )
            }
            Self::ExecReadOutput(input) => format!(
                "exec_read_output process_id={} stream={} cursor={} max_bytes={} max_lines={}",
                input.process_id,
                input.stream.unwrap_or(ProcessOutputStream::Merged).as_str(),
                input
                    .cursor
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                input
                    .max_bytes
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                input
                    .max_lines
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string())
            ),
            Self::ExecWait(input) => format!("exec_wait process_id={}", input.process_id),
            Self::ExecKill(input) => format!("exec_kill process_id={}", input.process_id),
            Self::PlanRead(_) => "plan_read".to_string(),
            Self::PlanWrite(input) => format!("plan_write items={}", input.items.len()),
            Self::InitPlan(input) => format!("init_plan goal={}", input.goal),
            Self::AddTask(input) => format!(
                "add_task description={} depends_on={}",
                input.description,
                input.depends_on.len()
            ),
            Self::SetTaskStatus(input) => format!(
                "set_task_status task_id={} status={}",
                input.task_id, input.new_status
            ),
            Self::AddTaskNote(input) => format!("add_task_note task_id={}", input.task_id),
            Self::EditTask(input) => format!("edit_task task_id={}", input.task_id),
            Self::PlanSnapshot(_) => "plan_snapshot".to_string(),
            Self::PlanLint(_) => "plan_lint".to_string(),
            Self::PromptBudgetRead(_) => "prompt_budget_read".to_string(),
            Self::PromptBudgetUpdate(input) => format!(
                "prompt_budget_update scope={} reset={} percentages={}",
                input.scope.as_str(),
                input.reset,
                input.percentages.is_some()
            ),
            Self::AutonomyStateRead(input) => format!(
                "autonomy_state_read max_items={} include_inactive_schedules={}",
                input
                    .max_items
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "default".to_string()),
                input.include_inactive_schedules.unwrap_or(false)
            ),
            Self::SkillList(input) => format!(
                "skill_list include_inactive={} offset={} limit={}",
                input.include_inactive.unwrap_or(true),
                input.offset.unwrap_or(0),
                input.limit.unwrap_or(20)
            ),
            Self::SkillRead(input) => format!(
                "skill_read name={} max_bytes={}",
                input.name,
                input
                    .max_bytes
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "default".to_string())
            ),
            Self::SkillEnable(input) => format!("skill_enable name={}", input.name),
            Self::SkillDisable(input) => format!("skill_disable name={}", input.name),
            Self::ArtifactRead(input) => format!("artifact_read artifact_id={}", input.artifact_id),
            Self::ArtifactSearch(input) => {
                format!(
                    "artifact_search query={} limit={}",
                    input.query, input.limit
                )
            }
            Self::ArtifactPin(input) => format!("artifact_pin artifact_id={}", input.artifact_id),
            Self::ArtifactUnpin(input) => {
                format!("artifact_unpin artifact_id={}", input.artifact_id)
            }
            Self::DeliverFile(input) => {
                if let Some(artifact_id) = &input.artifact_id {
                    format!("deliver_file artifact_id={artifact_id}")
                } else {
                    format!(
                        "deliver_file workspace_path={}",
                        input.workspace_path.as_deref().unwrap_or("<missing>")
                    )
                }
            }
            Self::KnowledgeSearch(input) => format!(
                "knowledge_search query={} offset={} limit={}",
                input.query,
                input.offset.unwrap_or(0),
                input.limit.unwrap_or(0)
            ),
            Self::KnowledgeRead(input) => format!(
                "knowledge_read path={} mode={}",
                normalize_tool_path(&input.path),
                input.mode.unwrap_or(KnowledgeReadMode::Excerpt).as_str()
            ),
            Self::SessionSearch(input) => format!(
                "session_search query={} offset={} limit={}",
                input.query,
                input.offset.unwrap_or(0),
                input.limit.unwrap_or(0)
            ),
            Self::SessionRead(input) => format!(
                "session_read session_id={} mode={}",
                input.session_id,
                input.mode.unwrap_or(SessionReadMode::Summary).as_str()
            ),
            Self::SessionWait(input) => format!(
                "session_wait session_id={} timeout_ms={} mode={}",
                input.session_id,
                input.wait_timeout_ms.unwrap_or(0),
                input.mode.unwrap_or(SessionReadMode::Transcript).as_str()
            ),
            Self::McpCall(input) => format!("mcp_call exposed_name={}", input.exposed_name),
            Self::McpSearchResources(input) => format!(
                "mcp_search_resources connector={} query={} offset={} limit={}",
                input.connector_id.as_deref().unwrap_or("*"),
                input.query.as_deref().unwrap_or("*"),
                input.offset.unwrap_or(0),
                input.limit.unwrap_or(0)
            ),
            Self::McpReadResource(input) => format!(
                "mcp_read_resource connector_id={} uri={}",
                input.connector_id, input.uri
            ),
            Self::McpSearchPrompts(input) => format!(
                "mcp_search_prompts connector={} query={} offset={} limit={}",
                input.connector_id.as_deref().unwrap_or("*"),
                input.query.as_deref().unwrap_or("*"),
                input.offset.unwrap_or(0),
                input.limit.unwrap_or(0)
            ),
            Self::McpGetPrompt(input) => format!(
                "mcp_get_prompt connector_id={} name={}",
                input.connector_id, input.name
            ),
            Self::AgentList(input) => format!(
                "agent_list offset={} limit={}",
                input.offset.unwrap_or(0),
                input.limit.unwrap_or(0)
            ),
            Self::AgentRead(input) => format!("agent_read identifier={}", input.identifier),
            Self::AgentCreate(input) => format!(
                "agent_create name={} template={}",
                input.name,
                input.template_identifier.as_deref().unwrap_or("current")
            ),
            Self::ContinueLater(input) => format!(
                "continue_later delay_seconds={} delivery_mode={}",
                input.delay_seconds,
                input
                    .delivery_mode
                    .unwrap_or(AgentScheduleDeliveryMode::ExistingSession)
                    .as_str()
            ),
            Self::ScheduleList(input) => format!(
                "schedule_list agent={} offset={} limit={}",
                input.agent_identifier.as_deref().unwrap_or("*"),
                input.offset.unwrap_or(0),
                input.limit.unwrap_or(0)
            ),
            Self::ScheduleRead(input) => format!("schedule_read id={}", input.id),
            Self::ScheduleCreate(input) => format!(
                "schedule_create id={} interval_seconds={}",
                input.id, input.interval_seconds
            ),
            Self::ScheduleUpdate(input) => format!("schedule_update id={}", input.id),
            Self::ScheduleDelete(input) => format!("schedule_delete id={}", input.id),
            Self::MessageAgent(input) => format!(
                "message_agent target_agent_id={} message_bytes={}",
                input.target_agent_id,
                input.message.len()
            ),
            Self::GrantAgentChainContinuation(input) => {
                format!("grant_agent_chain_continuation chain_id={}", input.chain_id)
            }
        }
    }

    pub fn from_openai_function(name: &str, arguments: &str) -> Result<Self, ToolCallParseError> {
        match name {
            "fs_read" => serde_json::from_str(arguments)
                .map(Self::FsRead)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "fs_write" => serde_json::from_str(arguments)
                .map(Self::FsWrite)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "fs_patch" => serde_json::from_str(arguments)
                .map(Self::FsPatch)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "fs_read_text" => serde_json::from_str(arguments)
                .map(Self::FsReadText)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "fs_read_lines" => serde_json::from_str(arguments)
                .map(Self::FsReadLines)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "fs_search_text" => serde_json::from_str(arguments)
                .map(Self::FsSearchText)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "fs_find_in_files" => serde_json::from_str(arguments)
                .map(Self::FsFindInFiles)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "fs_write_text" => serde_json::from_str(arguments)
                .map(Self::FsWriteText)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "fs_patch_text" => serde_json::from_str(arguments)
                .map(Self::FsPatchText)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "fs_replace_lines" => serde_json::from_str(arguments)
                .map(Self::FsReplaceLines)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "fs_insert_text" => serde_json::from_str(arguments)
                .map(Self::FsInsertText)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "fs_mkdir" => serde_json::from_str(arguments)
                .map(Self::FsMkdir)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "fs_move" => serde_json::from_str(arguments)
                .map(Self::FsMove)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "fs_trash" => serde_json::from_str(arguments)
                .map(Self::FsTrash)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "fs_list" => serde_json::from_str(arguments)
                .map(Self::FsList)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "fs_glob" => serde_json::from_str(arguments)
                .map(Self::FsGlob)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "fs_search" => serde_json::from_str(arguments)
                .map(Self::FsSearch)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "web_fetch" => serde_json::from_str(arguments)
                .map(Self::WebFetch)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "web_search" => serde_json::from_str(arguments)
                .map(Self::WebSearch)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "exec_start" => serde_json::from_str(arguments)
                .map(Self::ExecStart)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "exec_read_output" => serde_json::from_str(arguments)
                .map(Self::ExecReadOutput)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "exec_wait" => serde_json::from_str(arguments)
                .map(Self::ExecWait)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "exec_kill" => serde_json::from_str(arguments)
                .map(Self::ExecKill)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "plan_read" => serde_json::from_str(arguments)
                .map(Self::PlanRead)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "plan_write" => serde_json::from_str(arguments)
                .map(Self::PlanWrite)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "init_plan" => serde_json::from_str(arguments)
                .map(Self::InitPlan)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "add_task" => serde_json::from_str(arguments)
                .map(Self::AddTask)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "set_task_status" => serde_json::from_str(arguments)
                .map(Self::SetTaskStatus)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "add_task_note" => serde_json::from_str(arguments)
                .map(Self::AddTaskNote)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "edit_task" => serde_json::from_str(arguments)
                .map(Self::EditTask)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "plan_snapshot" => serde_json::from_str(arguments)
                .map(Self::PlanSnapshot)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "plan_lint" => serde_json::from_str(arguments)
                .map(Self::PlanLint)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "prompt_budget_read" => serde_json::from_str(arguments)
                .map(Self::PromptBudgetRead)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "prompt_budget_update" => serde_json::from_str(arguments)
                .map(Self::PromptBudgetUpdate)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "autonomy_state_read" => serde_json::from_str(arguments)
                .map(Self::AutonomyStateRead)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "skill_list" => serde_json::from_str(arguments)
                .map(Self::SkillList)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "skill_read" => serde_json::from_str(arguments)
                .map(Self::SkillRead)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "skill_enable" => serde_json::from_str(arguments)
                .map(Self::SkillEnable)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "skill_disable" => serde_json::from_str(arguments)
                .map(Self::SkillDisable)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "artifact_read" => serde_json::from_str(arguments)
                .map(Self::ArtifactRead)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "artifact_search" => serde_json::from_str(arguments)
                .map(Self::ArtifactSearch)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "artifact_pin" => serde_json::from_str(arguments)
                .map(Self::ArtifactPin)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "artifact_unpin" => serde_json::from_str(arguments)
                .map(Self::ArtifactUnpin)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "deliver_file" => serde_json::from_str(arguments)
                .map(Self::DeliverFile)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "knowledge_search" => serde_json::from_str(arguments)
                .map(Self::KnowledgeSearch)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "knowledge_read" => {
                Self::parse_arguments_with_enum_repair(name, arguments, KNOWLEDGE_READ_ENUM_REPAIRS)
                    .map(Self::KnowledgeRead)
            }
            "session_search" => serde_json::from_str(arguments)
                .map(Self::SessionSearch)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "session_read" => {
                Self::parse_arguments_with_enum_repair(name, arguments, SESSION_READ_ENUM_REPAIRS)
                    .map(Self::SessionRead)
            }
            "session_wait" => {
                Self::parse_arguments_with_enum_repair(name, arguments, SESSION_WAIT_ENUM_REPAIRS)
                    .map(Self::SessionWait)
            }
            "mcp_search_resources" => serde_json::from_str(arguments)
                .map(Self::McpSearchResources)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "mcp_read_resource" => serde_json::from_str(arguments)
                .map(Self::McpReadResource)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "mcp_search_prompts" => serde_json::from_str(arguments)
                .map(Self::McpSearchPrompts)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "mcp_get_prompt" => serde_json::from_str(arguments)
                .map(Self::McpGetPrompt)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "agent_list" => serde_json::from_str(arguments)
                .map(Self::AgentList)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "agent_read" => serde_json::from_str(arguments)
                .map(Self::AgentRead)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "agent_create" => serde_json::from_str(arguments)
                .map(Self::AgentCreate)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "continue_later" => {
                Self::parse_arguments_with_enum_repair(name, arguments, CONTINUE_LATER_ENUM_REPAIRS)
                    .map(Self::ContinueLater)
            }
            "schedule_list" => serde_json::from_str(arguments)
                .map(Self::ScheduleList)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "schedule_read" => serde_json::from_str(arguments)
                .map(Self::ScheduleRead)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "schedule_create" => {
                Self::parse_arguments_with_enum_repair(name, arguments, SCHEDULE_ENUM_REPAIRS)
                    .map(Self::ScheduleCreate)
            }
            "schedule_update" => {
                Self::parse_arguments_with_enum_repair(name, arguments, SCHEDULE_ENUM_REPAIRS)
                    .map(Self::ScheduleUpdate)
            }
            "schedule_delete" => serde_json::from_str(arguments)
                .map(Self::ScheduleDelete)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "message_agent" => serde_json::from_str(arguments)
                .map(Self::MessageAgent)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            "grant_agent_chain_continuation" => serde_json::from_str(arguments)
                .map(Self::GrantAgentChainContinuation)
                .map_err(|source| ToolCallParseError::InvalidArguments {
                    name: name.to_string(),
                    source,
                }),
            _ if name.starts_with("mcp__") => {
                let parsed = if arguments.trim().is_empty() {
                    json!({})
                } else {
                    serde_json::from_str::<Value>(arguments).map_err(|source| {
                        ToolCallParseError::InvalidArguments {
                            name: name.to_string(),
                            source,
                        }
                    })?
                };
                if !parsed.is_object() {
                    return Err(ToolCallParseError::InvalidArguments {
                        name: name.to_string(),
                        source: serde_json::Error::io(std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            "dynamic MCP tool arguments must be a JSON object",
                        )),
                    });
                }
                Ok(Self::McpCall(McpCallInput {
                    exposed_name: name.to_string(),
                    arguments_json: parsed.to_string(),
                }))
            }
            _ => Err(ToolCallParseError::UnknownTool {
                name: name.to_string(),
            }),
        }
    }
}

impl fmt::Display for ToolCallParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownTool { name } => write!(formatter, "unknown tool call {name}"),
            Self::InvalidArguments { name, source } => {
                write!(formatter, "invalid arguments for {name}: {source}")
            }
        }
    }
}

impl Error for ToolCallParseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidArguments { source, .. } => Some(source),
            Self::UnknownTool { .. } => None,
        }
    }
}
