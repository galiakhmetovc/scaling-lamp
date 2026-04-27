use super::{
    ExecStartInput, FsFindInFilesInput, FsGlobInput, FsInsertTextInput, FsListInput, FsMkdirInput,
    FsMoveInput, FsPatchTextInput, FsReadLinesInput, FsReadTextInput, FsReplaceLinesInput,
    FsSearchTextInput, FsTrashInput, FsWriteMode, FsWriteTextInput, KnowledgeReadInput,
    KnowledgeReadMode, KnowledgeRoot, KnowledgeSearchInput, KnowledgeSourceKind, ProcessKillInput,
    ProcessOutputStatus, ProcessOutputStream, ProcessReadOutputInput, ProcessResultStatus,
    ProcessWaitInput, PromptBudgetLayerPercentagesInput, SessionReadInput, SessionReadMode,
    SessionSearchInput, SessionWaitInput, SharedProcessRegistry, ToolCall, ToolCatalog, ToolFamily,
    ToolName, ToolRuntime, WebFetchInput, WebSearchBackend, WebSearchInput, WebToolClient,
};
use crate::memory::SessionRetentionTier;
use crate::workspace::WorkspaceRef;
use std::io::{Read, Write};
use std::net::TcpListener;
#[cfg(unix)]
use std::os::unix::net::UnixListener;
use std::thread;
use std::time::{Duration, Instant};

#[test]
fn catalog_exposes_distinct_families_and_policy_flags() {
    let catalog = ToolCatalog::default();
    let artifact_read = catalog
        .definition(ToolName::ArtifactRead)
        .expect("artifact_read");
    let artifact_search = catalog
        .definition(ToolName::ArtifactSearch)
        .expect("artifact_search");
    let exec_start = catalog.definition(ToolName::ExecStart).expect("exec_start");
    let fs_glob = catalog.definition(ToolName::FsGlob).expect("fs_glob");
    let fs_patch = catalog
        .definition(ToolName::FsPatchText)
        .expect("fs_patch_text");
    let fs_read_lines = catalog
        .definition(ToolName::FsReadLines)
        .expect("fs_read_lines");
    let fs_find_in_files = catalog
        .definition(ToolName::FsFindInFiles)
        .expect("fs_find_in_files");
    let fs_mkdir = catalog.definition(ToolName::FsMkdir).expect("fs_mkdir");
    let fs_move = catalog.definition(ToolName::FsMove).expect("fs_move");
    let fs_trash = catalog.definition(ToolName::FsTrash).expect("fs_trash");
    let plan_read = catalog.definition(ToolName::PlanRead).expect("plan_read");
    let plan_write = catalog.definition(ToolName::PlanWrite).expect("plan_write");
    let agent_list = catalog.definition(ToolName::AgentList).expect("agent_list");
    let agent_create = catalog
        .definition(ToolName::AgentCreate)
        .expect("agent_create");
    let schedule_read = catalog
        .definition(ToolName::ScheduleRead)
        .expect("schedule_read");
    let web_fetch = catalog.definition(ToolName::WebFetch).expect("web_fetch");
    let web_search = catalog.definition(ToolName::WebSearch).expect("web_search");
    let fs_read = catalog.definition(ToolName::FsRead).expect("fs_read");
    let fs_write = catalog.definition(ToolName::FsWrite).expect("fs_write");

    assert_eq!(
        catalog.families,
        [
            "fs", "web", "exec", "plan", "offload", "memory", "mcp", "agent"
        ]
    );
    assert_eq!(artifact_read.family, ToolFamily::Offload);
    assert_eq!(artifact_search.family, ToolFamily::Offload);
    assert_eq!(
        catalog
            .definition(ToolName::SessionSearch)
            .expect("session_search")
            .family,
        ToolFamily::Memory
    );
    assert_eq!(agent_list.family, ToolFamily::Agent);
    assert_eq!(agent_create.family, ToolFamily::Agent);
    assert_eq!(
        catalog
            .definition(ToolName::McpSearchResources)
            .expect("mcp_search_resources")
            .family,
        ToolFamily::Mcp
    );
    assert_eq!(exec_start.family, ToolFamily::Exec);
    assert_eq!(fs_glob.family, ToolFamily::Filesystem);
    assert_eq!(fs_read_lines.family, ToolFamily::Filesystem);
    assert_eq!(fs_find_in_files.family, ToolFamily::Filesystem);
    assert_eq!(fs_mkdir.family, ToolFamily::Filesystem);
    assert_eq!(fs_move.family, ToolFamily::Filesystem);
    assert_eq!(fs_trash.family, ToolFamily::Filesystem);
    assert_eq!(fs_patch.family, ToolFamily::Filesystem);
    assert_eq!(plan_read.family, ToolFamily::Planning);
    assert_eq!(plan_write.family, ToolFamily::Planning);
    assert_eq!(schedule_read.family, ToolFamily::Agent);
    assert_eq!(web_fetch.family, ToolFamily::Web);
    assert_eq!(web_search.family, ToolFamily::Web);
    assert!(agent_list.policy.read_only);
    assert!(!agent_create.policy.read_only);
    assert!(agent_create.policy.requires_approval);
    assert!(artifact_read.policy.read_only);
    assert!(artifact_search.policy.read_only);
    assert!(exec_start.policy.requires_approval);
    assert!(fs_glob.policy.read_only);
    assert!(fs_read_lines.policy.read_only);
    assert!(fs_find_in_files.policy.read_only);
    assert!(fs_patch.policy.destructive);
    assert!(fs_mkdir.policy.destructive);
    assert!(fs_move.policy.destructive);
    assert!(fs_trash.policy.destructive);
    assert!(plan_read.policy.read_only);
    assert!(!plan_write.policy.read_only);
    assert!(!plan_write.policy.requires_approval);
    assert!(web_fetch.policy.read_only);
    assert!(web_search.policy.read_only);
    assert!(fs_read.policy.read_only);
    assert!(fs_write.policy.destructive);
}

#[test]
fn automatic_model_definitions_include_structured_exec_tools() {
    let catalog = ToolCatalog::default();
    let names = catalog
        .automatic_model_definitions()
        .into_iter()
        .map(|definition| definition.name)
        .collect::<Vec<_>>();

    assert!(names.contains(&ToolName::ExecStart));
    assert!(names.contains(&ToolName::ExecReadOutput));
    assert!(names.contains(&ToolName::ExecWait));
    assert!(names.contains(&ToolName::ExecKill));
}

#[test]
fn automatic_model_definitions_include_granular_planning_tools() {
    let catalog = ToolCatalog::default();
    let names = catalog
        .automatic_model_definitions()
        .into_iter()
        .map(|definition| definition.name)
        .collect::<Vec<_>>();

    assert!(names.contains(&ToolName::InitPlan));
    assert!(names.contains(&ToolName::AddTask));
    assert!(names.contains(&ToolName::SetTaskStatus));
    assert!(names.contains(&ToolName::AddTaskNote));
    assert!(names.contains(&ToolName::EditTask));
    assert!(names.contains(&ToolName::PlanSnapshot));
    assert!(names.contains(&ToolName::PlanLint));
}

#[test]
fn automatic_model_definitions_include_prompt_budget_tools() {
    let catalog = ToolCatalog::default();
    let names = catalog
        .automatic_model_definitions()
        .into_iter()
        .map(|definition| definition.name)
        .collect::<Vec<_>>();

    assert!(names.contains(&ToolName::PromptBudgetRead));
    assert!(names.contains(&ToolName::PromptBudgetUpdate));
}

#[test]
fn automatic_model_definitions_include_agent_and_schedule_tools() {
    let catalog = ToolCatalog::default();
    let names = catalog
        .automatic_model_definitions()
        .into_iter()
        .map(|definition| definition.name)
        .collect::<Vec<_>>();

    assert!(names.contains(&ToolName::AgentList));
    assert!(names.contains(&ToolName::AgentRead));
    assert!(names.contains(&ToolName::AgentCreate));
    assert!(names.contains(&ToolName::ContinueLater));
    assert!(names.contains(&ToolName::ScheduleList));
    assert!(names.contains(&ToolName::ScheduleRead));
    assert!(names.contains(&ToolName::ScheduleCreate));
    assert!(names.contains(&ToolName::ScheduleUpdate));
    assert!(names.contains(&ToolName::ScheduleDelete));
    assert!(names.contains(&ToolName::SessionWait));
}

#[test]
fn automatic_model_definitions_include_mcp_utility_tools() {
    let catalog = ToolCatalog::default();
    let names = catalog
        .automatic_model_definitions()
        .into_iter()
        .map(|definition| definition.name)
        .collect::<Vec<_>>();

    assert!(names.contains(&ToolName::McpSearchResources));
    assert!(names.contains(&ToolName::McpReadResource));
    assert!(names.contains(&ToolName::McpSearchPrompts));
    assert!(names.contains(&ToolName::McpGetPrompt));
}

#[test]
fn tool_call_parses_mcp_utility_inputs() {
    let search_resources = ToolCall::from_openai_function(
        "mcp_search_resources",
        r#"{"connector_id":"docs","query":"onboarding","limit":5}"#,
    )
    .expect("parse mcp_search_resources");
    let read_resource = ToolCall::from_openai_function(
        "mcp_read_resource",
        r#"{"connector_id":"docs","uri":"file:///guides/onboarding.md"}"#,
    )
    .expect("parse mcp_read_resource");
    let search_prompts = ToolCall::from_openai_function(
        "mcp_search_prompts",
        r#"{"connector_id":"docs","query":"incident","limit":3}"#,
    )
    .expect("parse mcp_search_prompts");
    let get_prompt = ToolCall::from_openai_function(
        "mcp_get_prompt",
        r#"{"connector_id":"docs","name":"onboarding","arguments":{"audience":"operator"}}"#,
    )
    .expect("parse mcp_get_prompt");

    assert_eq!(
        search_resources,
        ToolCall::McpSearchResources(super::McpSearchResourcesInput {
            connector_id: Some("docs".to_string()),
            query: Some("onboarding".to_string()),
            limit: Some(5),
            offset: None,
        })
    );
    assert_eq!(
        read_resource,
        ToolCall::McpReadResource(super::McpReadResourceInput {
            connector_id: "docs".to_string(),
            uri: "file:///guides/onboarding.md".to_string(),
        })
    );
    assert_eq!(
        search_prompts,
        ToolCall::McpSearchPrompts(super::McpSearchPromptsInput {
            connector_id: Some("docs".to_string()),
            query: Some("incident".to_string()),
            limit: Some(3),
            offset: None,
        })
    );
    assert_eq!(
        get_prompt,
        ToolCall::McpGetPrompt(super::McpGetPromptInput {
            connector_id: "docs".to_string(),
            name: "onboarding".to_string(),
            arguments: Some(std::collections::BTreeMap::from([(
                "audience".to_string(),
                "operator".to_string(),
            )])),
        })
    );
}

#[test]
fn tool_call_parses_continue_later_inputs() {
    let call = ToolCall::from_openai_function(
        "continue_later",
        r#"{"delay_seconds":900,"handoff_payload":"resume from this handoff","delivery_mode":"existing_session"}"#,
    )
    .expect("parse continue_later");

    assert_eq!(
        call,
        ToolCall::ContinueLater(super::ContinueLaterInput {
            delay_seconds: 900,
            handoff_payload: "resume from this handoff".to_string(),
            delivery_mode: Some(crate::agent::AgentScheduleDeliveryMode::ExistingSession),
        })
    );
}

#[test]
fn tool_call_parses_continue_later_inputs_with_bare_delivery_mode() {
    let call = ToolCall::from_openai_function(
        "continue_later",
        r#"{"delay_seconds":900,"handoff_payload":"resume from this handoff","delivery_mode":existing_session}"#,
    )
    .expect("parse continue_later with bare enum");

    assert_eq!(
        call,
        ToolCall::ContinueLater(super::ContinueLaterInput {
            delay_seconds: 900,
            handoff_payload: "resume from this handoff".to_string(),
            delivery_mode: Some(crate::agent::AgentScheduleDeliveryMode::ExistingSession),
        })
    );
}

#[test]
fn automatic_model_definitions_include_session_memory_tools() {
    let catalog = ToolCatalog::default();
    let names = catalog
        .automatic_model_definitions()
        .into_iter()
        .map(|definition| definition.name)
        .collect::<Vec<_>>();

    assert!(names.contains(&ToolName::SessionSearch));
    assert!(names.contains(&ToolName::SessionRead));
    assert!(names.contains(&ToolName::KnowledgeSearch));
    assert!(names.contains(&ToolName::KnowledgeRead));
}

#[test]
fn automatic_model_definitions_include_skill_tools() {
    let catalog = ToolCatalog::default();
    let names = catalog
        .automatic_model_definitions()
        .into_iter()
        .map(|definition| definition.name)
        .collect::<Vec<_>>();

    assert!(names.contains(&ToolName::SkillList));
    assert!(names.contains(&ToolName::SkillRead));
    assert!(names.contains(&ToolName::SkillEnable));
    assert!(names.contains(&ToolName::SkillDisable));
}

#[test]
fn tool_call_parses_session_memory_inputs() {
    let search = ToolCall::from_openai_function(
        "session_search",
        r#"{"query":"ADQM","tiers":["warm"],"limit":5}"#,
    )
    .expect("parse session_search");
    let read = ToolCall::from_openai_function(
        "session_read",
        r#"{"session_id":"session-1","mode":"transcript","cursor":3,"max_items":10}"#,
    )
    .expect("parse session_read");

    assert_eq!(
        search,
        ToolCall::SessionSearch(SessionSearchInput {
            query: "ADQM".to_string(),
            limit: Some(5),
            offset: None,
            tiers: Some(vec![SessionRetentionTier::Warm]),
            agent_identifier: None,
            updated_after: None,
            updated_before: None,
        })
    );
    assert_eq!(
        read,
        ToolCall::SessionRead(SessionReadInput {
            session_id: "session-1".to_string(),
            mode: Some(SessionReadMode::Transcript),
            cursor: Some(3),
            max_items: Some(10),
            max_bytes: None,
            include_tools: None,
        })
    );
}

#[test]
fn tool_call_parses_session_wait_inputs() {
    let wait = ToolCall::from_openai_function(
        "session_wait",
        r#"{"session_id":"session-agentmsg-1","wait_timeout_ms":1500,"mode":"transcript","max_items":10}"#,
    )
    .expect("parse session_wait");

    assert_eq!(
        wait,
        ToolCall::SessionWait(SessionWaitInput {
            session_id: "session-agentmsg-1".to_string(),
            wait_timeout_ms: Some(1500),
            mode: Some(SessionReadMode::Transcript),
            cursor: None,
            max_items: Some(10),
            max_bytes: None,
            include_tools: None,
        })
    );
}

#[test]
fn tool_call_parses_session_wait_inputs_with_bare_mode() {
    let wait = ToolCall::from_openai_function(
        "session_wait",
        r#"{"session_id":"session-agentmsg-1","wait_timeout_ms":1500,"mode":transcript,"max_items":10}"#,
    )
    .expect("parse session_wait with bare enum");

    assert_eq!(
        wait,
        ToolCall::SessionWait(SessionWaitInput {
            session_id: "session-agentmsg-1".to_string(),
            wait_timeout_ms: Some(1500),
            mode: Some(SessionReadMode::Transcript),
            cursor: None,
            max_items: Some(10),
            max_bytes: None,
            include_tools: None,
        })
    );
}

#[test]
fn tool_call_parses_schedule_create_inputs_with_bare_enums() {
    let create = ToolCall::from_openai_function(
        "schedule_create",
        r#"{"id":"t486-cycle-152","prompt":"Continue cycle 152","interval_seconds":300,"mode":once,"delivery_mode":fresh_session}"#,
    )
    .expect("parse schedule_create with bare enums");

    assert_eq!(
        create,
        ToolCall::ScheduleCreate(super::ScheduleCreateInput {
            id: "t486-cycle-152".to_string(),
            agent_identifier: None,
            prompt: "Continue cycle 152".to_string(),
            mode: Some(crate::agent::AgentScheduleMode::Once),
            delivery_mode: Some(crate::agent::AgentScheduleDeliveryMode::FreshSession),
            target_session_id: None,
            interval_seconds: 300,
            enabled: None,
        })
    );
}

#[test]
fn tool_call_parses_schedule_update_inputs_with_bare_enums() {
    let update = ToolCall::from_openai_function(
        "schedule_update",
        r#"{"id":"t486-cycle-152","mode":once,"delivery_mode":existing_session,"enabled":true}"#,
    )
    .expect("parse schedule_update with bare enums");

    assert_eq!(
        update,
        ToolCall::ScheduleUpdate(super::ScheduleUpdateInput {
            id: "t486-cycle-152".to_string(),
            agent_identifier: None,
            prompt: None,
            mode: Some(crate::agent::AgentScheduleMode::Once),
            delivery_mode: Some(crate::agent::AgentScheduleDeliveryMode::ExistingSession),
            target_session_id: None,
            interval_seconds: None,
            enabled: Some(true),
        })
    );
}

#[test]
fn interagent_tool_definitions_are_explicit_about_async_follow_up_flow() {
    let catalog = ToolCatalog::default();
    let message_agent = catalog
        .definition(ToolName::MessageAgent)
        .expect("message_agent");
    let session_wait = catalog
        .definition(ToolName::SessionWait)
        .expect("session_wait");
    let message_schema = message_agent.openai_function_schema().to_string();
    let session_wait_schema = session_wait.openai_function_schema().to_string();

    assert_eq!(message_agent.family, ToolFamily::Agent);
    assert_eq!(session_wait.family, ToolFamily::Agent);
    assert!(
        message_agent
            .description
            .contains("does not wait for the reply")
    );
    assert!(
        session_wait
            .description
            .contains("use this after message_agent")
    );
    assert!(session_wait.description.contains(r#""mode":"transcript""#));
    assert!(message_schema.contains("does not wait for the reply"));
    assert!(session_wait_schema.contains("recipient_session_id"));
}

#[test]
fn scheduling_tool_definitions_steer_reminders_to_continue_later() {
    let catalog = ToolCatalog::default();
    let continue_later = catalog
        .definition(ToolName::ContinueLater)
        .expect("continue_later");
    let schedule_create = catalog
        .definition(ToolName::ScheduleCreate)
        .expect("schedule_create");
    let continue_schema = continue_later.openai_function_schema().to_string();
    let schedule_schema = schedule_create.openai_function_schema().to_string();

    assert!(
        continue_later
            .description
            .contains("user asks you to remind or message them later")
    );
    assert!(
        schedule_create
            .description
            .contains("For simple one-shot reminders, prefer continue_later")
    );
    assert!(
        continue_later
            .description
            .contains(r#""delivery_mode":"existing_session""#)
    );
    assert!(schedule_create.description.contains(r#""mode":"once""#));
    assert!(continue_schema.contains("same session"));
    assert!(continue_schema.contains("what to say or do when the timer fires"));
    assert!(schedule_schema.contains("advanced or recurring"));
    assert!(schedule_schema.contains("existing_session"));
}

#[test]
fn enum_like_tool_parameters_require_quoted_json_strings_in_schema() {
    let catalog = ToolCatalog::default();
    let knowledge_read = catalog
        .definition(ToolName::KnowledgeRead)
        .expect("knowledge_read");
    let continue_later = catalog
        .definition(ToolName::ContinueLater)
        .expect("continue_later");
    let schedule_create = catalog
        .definition(ToolName::ScheduleCreate)
        .expect("schedule_create");
    let schedule_update = catalog
        .definition(ToolName::ScheduleUpdate)
        .expect("schedule_update");

    let knowledge_read_schema = knowledge_read.openai_function_schema().to_string();
    let continue_later_schema = continue_later.openai_function_schema().to_string();
    let schedule_create_schema = schedule_create.openai_function_schema().to_string();
    let schedule_update_schema = schedule_update.openai_function_schema().to_string();

    assert!(knowledge_read_schema.contains("quoted JSON string"));
    assert!(continue_later_schema.contains("quoted JSON string"));
    assert!(schedule_create_schema.contains("quoted JSON string"));
    assert!(schedule_update_schema.contains("quoted JSON string"));
}

#[test]
fn prompt_budget_tool_definitions_and_parsing_are_explicit() {
    let catalog = ToolCatalog::default();
    let read = catalog
        .definition(ToolName::PromptBudgetRead)
        .expect("prompt_budget_read");
    let update = catalog
        .definition(ToolName::PromptBudgetUpdate)
        .expect("prompt_budget_update");
    let update_schema = update.openai_function_schema().to_string();

    assert_eq!(read.family, ToolFamily::Planning);
    assert!(read.policy.read_only);
    assert!(!update.policy.read_only);
    assert!(!update.policy.requires_approval);
    assert!(update.description.contains("session-scoped"));
    assert!(update_schema.contains("percentages"));
    assert!(update_schema.contains("reset"));
    assert!(update_schema.contains("sum to 100"));

    let read_call = ToolCall::from_openai_function("prompt_budget_read", "{}").expect("parse read");
    assert!(matches!(read_call, ToolCall::PromptBudgetRead(_)));

    let update_call = ToolCall::from_openai_function(
        "prompt_budget_update",
        r#"{"percentages":{"system":5,"agents":8,"active_skills":12,"session_head":5,"autonomy_state":5,"plan":8,"context_summary":15,"offload_refs":15,"recent_tool_activity":7,"transcript_tail":20},"reason":"need more transcript tail"}"#,
    )
    .expect("parse update");
    assert_eq!(
        update_call,
        ToolCall::PromptBudgetUpdate(super::PromptBudgetUpdateInput {
            reset: false,
            percentages: Some(PromptBudgetLayerPercentagesInput {
                system: Some(5),
                agents: Some(8),
                active_skills: Some(12),
                session_head: Some(5),
                autonomy_state: Some(5),
                plan: Some(8),
                context_summary: Some(15),
                offload_refs: Some(15),
                recent_tool_activity: Some(7),
                transcript_tail: Some(20),
            }),
            reason: Some("need more transcript tail".to_string()),
        })
    );
}

#[test]
fn skill_tool_definitions_and_parsing_are_explicit() {
    let catalog = ToolCatalog::default();
    let list = catalog.definition(ToolName::SkillList).expect("skill_list");
    let read = catalog.definition(ToolName::SkillRead).expect("skill_read");
    let enable = catalog
        .definition(ToolName::SkillEnable)
        .expect("skill_enable");
    let disable = catalog
        .definition(ToolName::SkillDisable)
        .expect("skill_disable");

    assert_eq!(list.family, ToolFamily::Memory);
    assert!(list.policy.read_only);
    assert!(read.policy.read_only);
    assert!(!enable.policy.read_only);
    assert!(!enable.policy.requires_approval);
    assert!(!disable.policy.read_only);
    assert!(
        read.openai_function_schema()
            .to_string()
            .contains("max_bytes")
    );

    let list_call =
        ToolCall::from_openai_function("skill_list", r#"{"include_inactive":false,"limit":5}"#)
            .expect("parse list");
    assert_eq!(
        list_call,
        ToolCall::SkillList(super::SkillListInput {
            include_inactive: Some(false),
            limit: Some(5),
            offset: None,
        })
    );

    let read_call =
        ToolCall::from_openai_function("skill_read", r#"{"name":"rust-debug","max_bytes":512}"#)
            .expect("parse read");
    assert_eq!(
        read_call,
        ToolCall::SkillRead(super::SkillReadInput {
            name: "rust-debug".to_string(),
            max_bytes: Some(512),
        })
    );

    let enable_call = ToolCall::from_openai_function("skill_enable", r#"{"name":"rust-debug"}"#)
        .expect("parse enable");
    assert_eq!(
        enable_call,
        ToolCall::SkillEnable(super::SkillActivationInput {
            name: "rust-debug".to_string(),
        })
    );

    let disable_call = ToolCall::from_openai_function("skill_disable", r#"{"name":"rust-debug"}"#)
        .expect("parse disable");
    assert_eq!(
        disable_call,
        ToolCall::SkillDisable(super::SkillActivationInput {
            name: "rust-debug".to_string(),
        })
    );
}

#[test]
fn autonomy_state_read_definition_and_parsing_are_explicit() {
    let catalog = ToolCatalog::default();
    let definition = catalog
        .definition(ToolName::AutonomyStateRead)
        .expect("autonomy_state_read");
    let automatic_names = catalog
        .automatic_model_definitions()
        .into_iter()
        .map(|definition| definition.name)
        .collect::<Vec<_>>();

    assert!(automatic_names.contains(&ToolName::AutonomyStateRead));
    assert_eq!(definition.family, ToolFamily::Memory);
    assert!(definition.policy.read_only);
    let schema = definition.openai_function_schema().to_string();
    assert!(schema.contains("max_items"));
    assert!(schema.contains("include_inactive_schedules"));

    let call = ToolCall::from_openai_function(
        "autonomy_state_read",
        r#"{"max_items":5,"include_inactive_schedules":false}"#,
    )
    .expect("parse autonomy_state_read");
    assert_eq!(
        call,
        ToolCall::AutonomyStateRead(super::AutonomyStateReadInput {
            max_items: Some(5),
            include_inactive_schedules: Some(false),
        })
    );
}

#[test]
fn fs_patch_text_definition_is_explicit_about_search_and_replace_fields() {
    let catalog = ToolCatalog::default();
    let fs_patch_text = catalog
        .definition(ToolName::FsPatchText)
        .expect("fs_patch_text");
    let schema = fs_patch_text.openai_function_schema().to_string();

    assert!(fs_patch_text.description.contains("Use JSON fields"));
    assert!(fs_patch_text.description.contains("search"));
    assert!(fs_patch_text.description.contains("replace"));
    assert!(schema.contains("search"));
    assert!(schema.contains("replace"));
    assert!(schema.contains("Do not send old/new"));
}

#[test]
fn tool_call_parses_knowledge_memory_inputs() {
    let search = ToolCall::from_openai_function(
        "knowledge_search",
        r#"{"query":"architecture","kinds":["project_doc"],"roots":["docs"],"limit":5}"#,
    )
    .expect("parse knowledge_search");
    let read = ToolCall::from_openai_function(
        "knowledge_read",
        r#"{"path":"docs/architecture.md","mode":"excerpt","cursor":10,"max_lines":20}"#,
    )
    .expect("parse knowledge_read");

    assert_eq!(
        search,
        ToolCall::KnowledgeSearch(KnowledgeSearchInput {
            query: "architecture".to_string(),
            limit: Some(5),
            offset: None,
            kinds: Some(vec![KnowledgeSourceKind::ProjectDoc]),
            roots: Some(vec![KnowledgeRoot::Docs]),
        })
    );
    assert_eq!(
        read,
        ToolCall::KnowledgeRead(KnowledgeReadInput {
            path: "docs/architecture.md".to_string(),
            mode: Some(KnowledgeReadMode::Excerpt),
            cursor: Some(10),
            max_bytes: None,
            max_lines: Some(20),
        })
    );
}

#[test]
fn filesystem_tools_read_write_list_and_search_within_workspace() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = WorkspaceRef::new(temp.path());
    let mut runtime = ToolRuntime::new(workspace.clone());

    runtime
        .invoke(ToolCall::FsWriteText(FsWriteTextInput {
            path: "docs/notes.txt".to_string(),
            content: "alpha\nbeta\n".to_string(),
            mode: FsWriteMode::Upsert,
        }))
        .expect("fs_write_text");
    runtime
        .invoke(ToolCall::FsWriteText(FsWriteTextInput {
            path: "docs/summary.txt".to_string(),
            content: "beta\ngamma\n".to_string(),
            mode: FsWriteMode::Upsert,
        }))
        .expect("fs_write_text summary");

    let read = runtime
        .invoke(ToolCall::FsReadText(FsReadTextInput {
            path: "docs/notes.txt".to_string(),
        }))
        .expect("fs_read_text")
        .into_fs_read_text()
        .expect("fs_read_text output");
    let list = runtime
        .invoke(ToolCall::FsList(FsListInput {
            path: "docs".to_string(),
            recursive: true,
            limit: None,
            offset: None,
        }))
        .expect("fs_list")
        .into_fs_list()
        .expect("fs_list output");
    let file_search = runtime
        .invoke(ToolCall::FsSearchText(FsSearchTextInput {
            path: "docs/notes.txt".to_string(),
            query: "beta".to_string(),
        }))
        .expect("fs_search_text")
        .into_fs_search_text()
        .expect("fs_search_text output");
    let multi_search = runtime
        .invoke(ToolCall::FsFindInFiles(FsFindInFilesInput {
            query: "beta".to_string(),
            glob: Some("docs/*.txt".to_string()),
            limit: Some(10),
        }))
        .expect("fs_find_in_files")
        .into_fs_find_in_files()
        .expect("fs_find_in_files output");

    assert_eq!(read.path, "docs/notes.txt");
    assert_eq!(read.content, "alpha\nbeta\n");
    assert_eq!(
        list.entries
            .iter()
            .filter(|entry| entry.kind == crate::workspace::WorkspaceEntryKind::File)
            .map(|entry| entry.path.as_str())
            .collect::<Vec<_>>(),
        vec!["docs/notes.txt", "docs/summary.txt"]
    );
    assert_eq!(file_search.matches.len(), 1);
    assert_eq!(file_search.matches[0].path, "docs/notes.txt");
    assert_eq!(multi_search.matches.len(), 2);
    assert_eq!(multi_search.matches[0].path, "docs/notes.txt");
    assert_eq!(multi_search.matches[1].path, "docs/summary.txt");
}

#[test]
fn filesystem_tools_reject_paths_that_escape_workspace() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = WorkspaceRef::new(temp.path());
    let mut runtime = ToolRuntime::new(workspace);

    assert!(
        runtime
            .invoke(ToolCall::FsReadText(FsReadTextInput {
                path: "../secret.txt".to_string(),
            }))
            .is_err()
    );
}

#[test]
fn filesystem_tools_glob_and_patch_files_with_exact_edits() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = WorkspaceRef::new(temp.path());
    let mut runtime = ToolRuntime::new(workspace.clone());

    runtime
        .invoke(ToolCall::FsWriteText(FsWriteTextInput {
            path: "src/main.rs".to_string(),
            content: "fn main() {\n    println!(\"old\");\n}\n".to_string(),
            mode: FsWriteMode::Upsert,
        }))
        .expect("fs_write_text main");
    runtime
        .invoke(ToolCall::FsWriteText(FsWriteTextInput {
            path: "src/lib.rs".to_string(),
            content: "pub fn helper() {}\n".to_string(),
            mode: FsWriteMode::Upsert,
        }))
        .expect("fs_write_text lib");

    let globbed = runtime
        .invoke(ToolCall::FsGlob(FsGlobInput {
            path: "src".to_string(),
            pattern: "**/*.rs".to_string(),
            limit: None,
            offset: None,
        }))
        .expect("fs_glob")
        .into_fs_glob()
        .expect("fs_glob output");
    let patched = runtime
        .invoke(ToolCall::FsPatchText(FsPatchTextInput {
            path: "src/main.rs".to_string(),
            search: "println!(\"old\");".to_string(),
            replace: "println!(\"new\");".to_string(),
        }))
        .expect("fs_patch_text");
    let read = runtime
        .invoke(ToolCall::FsReadText(FsReadTextInput {
            path: "src/main.rs".to_string(),
        }))
        .expect("fs_read_text patched")
        .into_fs_read_text()
        .expect("fs_read_text output");

    assert_eq!(
        globbed
            .entries
            .iter()
            .filter(|entry| entry.kind == crate::workspace::WorkspaceEntryKind::File)
            .map(|entry| entry.path.as_str())
            .collect::<Vec<_>>(),
        vec!["src/lib.rs", "src/main.rs"]
    );
    assert_eq!(patched.summary(), "fs_patch_text path=src/main.rs");
    assert!(read.content.contains("println!(\"new\");"));
}

#[cfg(unix)]
#[test]
fn fs_find_in_files_ignores_unix_sockets_and_returns_regular_file_matches() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = WorkspaceRef::new(temp.path());
    let mut runtime = ToolRuntime::new(workspace);

    std::fs::write(temp.path().join("notes.txt"), "alpha\nneedle\n").expect("write notes");
    let _socket = UnixListener::bind(temp.path().join("agent.sock")).expect("bind unix socket");

    let search = runtime
        .invoke(ToolCall::FsFindInFiles(FsFindInFilesInput {
            query: "needle".to_string(),
            glob: None,
            limit: Some(10),
        }))
        .expect("fs_find_in_files with socket present")
        .into_fs_find_in_files()
        .expect("fs_find_in_files output");

    assert_eq!(search.matches.len(), 1);
    assert_eq!(search.matches[0].path, "notes.txt");
    assert_eq!(search.matches[0].line_number, 2);
}

#[test]
fn filesystem_tools_read_lines_report_file_bounds_and_replace_by_line_range() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = WorkspaceRef::new(temp.path());
    let mut runtime = ToolRuntime::new(workspace);

    runtime
        .invoke(ToolCall::FsWriteText(FsWriteTextInput {
            path: "docs/repeated.txt".to_string(),
            content: "zero\none\ntwo\nthree\n".to_string(),
            mode: FsWriteMode::Upsert,
        }))
        .expect("fs_write_text repeated");

    let chunk = runtime
        .invoke(ToolCall::FsReadLines(FsReadLinesInput {
            path: "docs/repeated.txt".to_string(),
            start_line: 2,
            end_line: 3,
        }))
        .expect("fs_read_lines")
        .into_fs_read_lines()
        .expect("fs_read_lines output");

    assert_eq!(chunk.start_line, 2);
    assert_eq!(chunk.end_line, 3);
    assert_eq!(chunk.total_lines, 4);
    assert!(!chunk.eof);
    assert_eq!(chunk.next_start_line, Some(4));
    assert_eq!(chunk.content, "one\ntwo\n");

    runtime
        .invoke(ToolCall::FsReplaceLines(FsReplaceLinesInput {
            path: "docs/repeated.txt".to_string(),
            start_line: 2,
            end_line: 3,
            content: "updated-one\nupdated-two\n".to_string(),
        }))
        .expect("fs_replace_lines");

    let read = runtime
        .invoke(ToolCall::FsReadText(FsReadTextInput {
            path: "docs/repeated.txt".to_string(),
        }))
        .expect("fs_read_text")
        .into_fs_read_text()
        .expect("fs_read_text output");

    assert_eq!(read.content, "zero\nupdated-one\nupdated-two\nthree\n");
}

#[test]
fn filesystem_tools_support_write_modes_and_directory_lifecycle() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = WorkspaceRef::new(temp.path());
    let mut runtime = ToolRuntime::new(workspace.clone());

    runtime
        .invoke(ToolCall::FsMkdir(FsMkdirInput {
            path: "workspace/tmp".to_string(),
        }))
        .expect("fs_mkdir");
    runtime
        .invoke(ToolCall::FsWriteText(FsWriteTextInput {
            path: "workspace/tmp/note.txt".to_string(),
            content: "first\n".to_string(),
            mode: FsWriteMode::Create,
        }))
        .expect("fs_write_text create");
    assert!(
        runtime
            .invoke(ToolCall::FsWriteText(FsWriteTextInput {
                path: "workspace/tmp/note.txt".to_string(),
                content: "again\n".to_string(),
                mode: FsWriteMode::Create,
            }))
            .is_err()
    );

    runtime
        .invoke(ToolCall::FsMove(FsMoveInput {
            src: "workspace/tmp/note.txt".to_string(),
            dest: "workspace/archive/note.txt".to_string(),
        }))
        .expect("fs_move");
    runtime
        .invoke(ToolCall::FsTrash(FsTrashInput {
            path: "workspace/archive/note.txt".to_string(),
        }))
        .expect("fs_trash");

    let listed = runtime
        .invoke(ToolCall::FsList(FsListInput {
            path: "".to_string(),
            recursive: true,
            limit: None,
            offset: None,
        }))
        .expect("fs_list")
        .into_fs_list()
        .expect("fs_list output");

    assert!(
        listed
            .entries
            .iter()
            .any(|entry| entry.path.starts_with(".trash/"))
    );
}

#[test]
fn filesystem_tools_list_results_are_bounded_and_paginated() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = WorkspaceRef::new(temp.path());
    let mut runtime = ToolRuntime::new(workspace);

    for index in 0..250 {
        runtime
            .invoke(ToolCall::FsWriteText(FsWriteTextInput {
                path: format!("big/file-{index:03}.txt"),
                content: format!("payload-{index}\n"),
                mode: FsWriteMode::Upsert,
            }))
            .expect("seed file");
    }

    let first_page = runtime
        .invoke(ToolCall::FsList(FsListInput {
            path: "big".to_string(),
            recursive: true,
            limit: None,
            offset: None,
        }))
        .expect("fs_list first page")
        .into_fs_list()
        .expect("fs_list first page output");

    assert_eq!(first_page.entries.len(), super::DEFAULT_FS_LIST_LIMIT);
    assert!(first_page.truncated);
    assert_eq!(first_page.offset, 0);
    assert_eq!(first_page.limit, super::DEFAULT_FS_LIST_LIMIT);
    assert_eq!(first_page.total_entries, 250);
    assert_eq!(first_page.next_offset, Some(super::DEFAULT_FS_LIST_LIMIT));

    let second_page = runtime
        .invoke(ToolCall::FsList(FsListInput {
            path: "big".to_string(),
            recursive: true,
            limit: Some(25),
            offset: first_page.next_offset,
        }))
        .expect("fs_list second page")
        .into_fs_list()
        .expect("fs_list second page output");

    assert_eq!(second_page.entries.len(), 25);
    assert!(second_page.truncated);
    assert_eq!(second_page.offset, super::DEFAULT_FS_LIST_LIMIT);
    assert_eq!(second_page.limit, 25);
    assert_eq!(second_page.total_entries, 250);
    assert_eq!(
        second_page.next_offset,
        Some(super::DEFAULT_FS_LIST_LIMIT + 25)
    );
}

#[test]
fn filesystem_tools_glob_results_are_bounded_and_paginated() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = WorkspaceRef::new(temp.path());
    let mut runtime = ToolRuntime::new(workspace);

    for index in 0..230 {
        runtime
            .invoke(ToolCall::FsWriteText(FsWriteTextInput {
                path: format!("globbed/file-{index:03}.txt"),
                content: format!("payload-{index}\n"),
                mode: FsWriteMode::Upsert,
            }))
            .expect("seed file");
    }

    let first_page = runtime
        .invoke(ToolCall::FsGlob(FsGlobInput {
            path: "globbed".to_string(),
            pattern: "**/*.txt".to_string(),
            limit: None,
            offset: None,
        }))
        .expect("fs_glob first page")
        .into_fs_glob()
        .expect("fs_glob first page output");

    assert_eq!(first_page.entries.len(), super::DEFAULT_FS_LIST_LIMIT);
    assert!(first_page.truncated);
    assert_eq!(first_page.offset, 0);
    assert_eq!(first_page.limit, super::DEFAULT_FS_LIST_LIMIT);
    assert_eq!(first_page.total_entries, 230);
    assert_eq!(first_page.next_offset, Some(super::DEFAULT_FS_LIST_LIMIT));

    let second_page = runtime
        .invoke(ToolCall::FsGlob(FsGlobInput {
            path: "globbed".to_string(),
            pattern: "**/*.txt".to_string(),
            limit: Some(15),
            offset: first_page.next_offset,
        }))
        .expect("fs_glob second page")
        .into_fs_glob()
        .expect("fs_glob second page output");

    assert_eq!(second_page.entries.len(), 15);
    assert!(second_page.truncated);
    assert_eq!(second_page.offset, super::DEFAULT_FS_LIST_LIMIT);
    assert_eq!(second_page.limit, 15);
    assert_eq!(second_page.total_entries, 230);
    assert_eq!(
        second_page.next_offset,
        Some(super::DEFAULT_FS_LIST_LIMIT + 15)
    );
}

#[test]
fn filesystem_tools_insert_text_around_line_positions() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = WorkspaceRef::new(temp.path());
    let mut runtime = ToolRuntime::new(workspace);

    runtime
        .invoke(ToolCall::FsWriteText(FsWriteTextInput {
            path: "docs/insert.txt".to_string(),
            content: "alpha\ngamma\n".to_string(),
            mode: FsWriteMode::Upsert,
        }))
        .expect("fs_write_text");

    runtime
        .invoke(ToolCall::FsInsertText(FsInsertTextInput {
            path: "docs/insert.txt".to_string(),
            line: Some(2),
            position: "before".to_string(),
            content: "beta\n".to_string(),
        }))
        .expect("fs_insert_text");

    let read = runtime
        .invoke(ToolCall::FsReadText(FsReadTextInput {
            path: "docs/insert.txt".to_string(),
        }))
        .expect("fs_read_text")
        .into_fs_read_text()
        .expect("fs_read_text output");

    assert_eq!(read.content, "alpha\nbeta\ngamma\n");
}

#[test]
fn structured_exec_treats_shell_tokens_as_literal_args() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = WorkspaceRef::new(temp.path());
    let mut runtime = ToolRuntime::new(workspace);

    let started = runtime
        .invoke(ToolCall::ExecStart(ExecStartInput {
            executable: "/bin/echo".to_string(),
            args: vec!["left|right".to_string()],
            cwd: None,
        }))
        .expect("exec_start")
        .into_process_start()
        .expect("process start");
    assert_eq!(started.command_display, "/bin/echo 'left|right'");
    assert_eq!(started.cwd, temp.path().display().to_string());
    let waited = runtime
        .invoke(ToolCall::ExecWait(ProcessWaitInput {
            process_id: started.process_id.clone(),
        }))
        .expect("exec_wait")
        .into_process_result()
        .expect("process result");

    assert_eq!(waited.status, ProcessResultStatus::Exited);
    assert_eq!(waited.exit_code, Some(0));
    assert_eq!(waited.stdout, "left|right\n");
}

#[test]
fn exec_kill_terminates_structured_processes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = WorkspaceRef::new(temp.path());
    let mut runtime = ToolRuntime::new(workspace);

    let exec_started = runtime
        .invoke(ToolCall::ExecStart(ExecStartInput {
            executable: "/bin/sleep".to_string(),
            args: vec!["5".to_string()],
            cwd: None,
        }))
        .expect("exec_start sleep")
        .into_process_start()
        .expect("sleep start");
    let killed = runtime
        .invoke(ToolCall::ExecKill(ProcessKillInput {
            process_id: exec_started.process_id,
        }))
        .expect("exec_kill")
        .into_process_result()
        .expect("killed process result");

    assert_eq!(killed.status, ProcessResultStatus::Killed);
}

#[test]
fn structured_exec_processes_survive_runtime_recreation_with_shared_registry() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = WorkspaceRef::new(temp.path());
    let registry = SharedProcessRegistry::default();
    let mut first_runtime =
        ToolRuntime::with_shared_process_registry(workspace.clone(), registry.clone());
    let mut second_runtime = ToolRuntime::with_shared_process_registry(workspace, registry);

    let started = first_runtime
        .invoke(ToolCall::ExecStart(ExecStartInput {
            executable: "/bin/sh".to_string(),
            args: vec!["-c".to_string(), "printf shared-registry".to_string()],
            cwd: None,
        }))
        .expect("exec_start")
        .into_process_start()
        .expect("process start");
    let waited = second_runtime
        .invoke(ToolCall::ExecWait(ProcessWaitInput {
            process_id: started.process_id,
        }))
        .expect("exec_wait")
        .into_process_result()
        .expect("process result");

    assert_eq!(waited.status, ProcessResultStatus::Exited);
    assert_eq!(waited.exit_code, Some(0));
    assert_eq!(waited.stdout, "shared-registry");
}

#[test]
fn exec_wait_returns_when_shell_exits_even_if_background_child_keeps_pipe_open() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = WorkspaceRef::new(temp.path());
    let mut runtime = ToolRuntime::new(workspace);

    let started = runtime
        .invoke(ToolCall::ExecStart(ExecStartInput {
            executable: "/bin/sh".to_string(),
            args: vec![
                "-c".to_string(),
                "printf 'shell-done\\n'; sleep 2 &".to_string(),
            ],
            cwd: None,
        }))
        .expect("exec_start")
        .into_process_start()
        .expect("process start");

    let started_at = Instant::now();
    let waited = runtime
        .invoke(ToolCall::ExecWait(ProcessWaitInput {
            process_id: started.process_id,
        }))
        .expect("exec_wait")
        .into_process_result()
        .expect("process result");

    assert!(
        started_at.elapsed() < Duration::from_secs(1),
        "exec_wait should not wait for background descendants that inherited stdout/stderr"
    );
    assert_eq!(waited.status, ProcessResultStatus::Exited);
    assert_eq!(waited.exit_code, Some(0));
    assert!(waited.stdout.contains("shell-done"));
}

#[test]
fn structured_exec_can_read_live_output_with_cursor_and_merged_stream() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = WorkspaceRef::new(temp.path());
    let registry = SharedProcessRegistry::default();
    let mut first_runtime =
        ToolRuntime::with_shared_process_registry(workspace.clone(), registry.clone());
    let mut second_runtime = ToolRuntime::with_shared_process_registry(workspace, registry);

    let started = first_runtime
        .invoke(ToolCall::ExecStart(ExecStartInput {
            executable: "/bin/sh".to_string(),
            args: vec![
                "-c".to_string(),
                "printf 'stdout-1\\n'; printf 'stderr-1\\n' >&2; sleep 1; printf 'stdout-2\\n'; printf 'stderr-2\\n' >&2"
                    .to_string(),
            ],
            cwd: None,
        }))
        .expect("exec_start")
        .into_process_start()
        .expect("process start");

    let deadline = Instant::now() + Duration::from_secs(3);
    let first_read = loop {
        let read = second_runtime
            .invoke(ToolCall::ExecReadOutput(ProcessReadOutputInput {
                process_id: started.process_id.clone(),
                stream: Some(ProcessOutputStream::Merged),
                cursor: None,
                max_bytes: Some(1024),
                max_lines: Some(10),
            }))
            .expect("exec_read_output")
            .into_process_output_read()
            .expect("process output read");
        if read.text.contains("stdout-1") && read.text.contains("stderr-1") {
            break read;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for live output"
        );
        thread::sleep(Duration::from_millis(25));
    };

    assert_eq!(first_read.status, ProcessOutputStatus::Running);
    assert!(first_read.next_cursor >= first_read.cursor);
    assert!(first_read.text.contains("stdout-1"));
    assert!(first_read.text.contains("stderr-1"));

    let waited = second_runtime
        .invoke(ToolCall::ExecWait(ProcessWaitInput {
            process_id: started.process_id.clone(),
        }))
        .expect("exec_wait")
        .into_process_result()
        .expect("process result");

    assert_eq!(waited.status, ProcessResultStatus::Exited);
    assert!(waited.stdout.contains("stdout-1"));
    assert!(waited.stdout.contains("stdout-2"));
    assert!(waited.stderr.contains("stderr-1"));
    assert!(waited.stderr.contains("stderr-2"));
}

#[cfg(target_os = "linux")]
#[test]
fn structured_exec_disconnects_child_stdin_from_the_terminal() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = WorkspaceRef::new(temp.path());
    let mut runtime = ToolRuntime::new(workspace);

    let started = runtime
        .invoke(ToolCall::ExecStart(ExecStartInput {
            executable: "/bin/sh".to_string(),
            args: vec!["-c".to_string(), "readlink /proc/self/fd/0".to_string()],
            cwd: None,
        }))
        .expect("exec_start")
        .into_process_start()
        .expect("process start");
    let waited = runtime
        .invoke(ToolCall::ExecWait(ProcessWaitInput {
            process_id: started.process_id,
        }))
        .expect("exec_wait")
        .into_process_result()
        .expect("process result");

    assert_eq!(waited.status, ProcessResultStatus::Exited);
    assert_eq!(waited.exit_code, Some(0));
    assert_eq!(waited.stdout.trim(), "/dev/null");
}

#[cfg(all(unix, target_os = "linux"))]
#[test]
fn structured_exec_detaches_from_the_parent_controlling_tty_when_one_exists() {
    // This assertion only makes sense when the test process itself has a terminal.
    if unsafe { libc::isatty(0) } != 1 {
        return;
    }

    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = WorkspaceRef::new(temp.path());
    let mut runtime = ToolRuntime::new(workspace);

    let started = runtime
        .invoke(ToolCall::ExecStart(ExecStartInput {
            executable: "/bin/sh".to_string(),
            args: vec![
                "-c".to_string(),
                "if (: >/dev/tty) 2>/dev/null; then printf tty-present; else printf tty-absent; fi"
                    .to_string(),
            ],
            cwd: None,
        }))
        .expect("exec_start")
        .into_process_start()
        .expect("process start");
    let waited = runtime
        .invoke(ToolCall::ExecWait(ProcessWaitInput {
            process_id: started.process_id,
        }))
        .expect("exec_wait")
        .into_process_result()
        .expect("process result");

    assert_eq!(waited.status, ProcessResultStatus::Exited);
    assert_eq!(waited.exit_code, Some(0));
    assert_eq!(waited.stdout, "tty-absent");
}

#[test]
fn web_tools_fetch_pages_and_return_search_results() {
    let server = TestHttpServer::spawn();
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = WorkspaceRef::new(temp.path());
    let mut runtime = ToolRuntime::with_web_client(
        workspace,
        WebToolClient::for_tests(server.base_url(), server.search_url()),
    );

    let fetched = runtime
        .invoke(ToolCall::WebFetch(WebFetchInput {
            url: server.page_url(),
        }))
        .expect("web_fetch")
        .into_web_fetch()
        .expect("web_fetch output");
    let searched = runtime
        .invoke(ToolCall::WebSearch(WebSearchInput {
            query: "agent runtime".to_string(),
            limit: 5,
        }))
        .expect("web_search")
        .into_web_search()
        .expect("web_search output");

    assert_eq!(fetched.url, server.page_url());
    assert_eq!(fetched.status_code, 200);
    assert_eq!(fetched.title.as_deref(), Some("Agent runtime page"));
    assert!(fetched.extracted_from_html);
    assert!(fetched.body.contains("# Agent runtime page"));
    assert!(fetched.body.contains("Agent runtime page body & notes."));
    assert!(fetched.body.contains("Second paragraph."));
    assert!(
        fetched
            .body
            .contains("[Reference spec](https://example.test/spec)")
    );
    assert!(!fetched.body.contains("<html"));
    assert!(!fetched.body.contains("console.log"));
    assert_eq!(searched.results.len(), 2);
    assert_eq!(searched.results[0].title, "Agent runtime docs");
    assert_eq!(searched.results[0].url, "https://example.test/docs");
}

#[test]
fn web_fetch_preserves_plain_text_responses() {
    let server = TestHttpServer::spawn_with_requests(1);
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = WorkspaceRef::new(temp.path());
    let mut runtime = ToolRuntime::with_web_client(
        workspace,
        WebToolClient::for_tests(server.base_url(), server.search_url()),
    );

    let fetched = runtime
        .invoke(ToolCall::WebFetch(WebFetchInput {
            url: server.plain_url(),
        }))
        .expect("web_fetch")
        .into_web_fetch()
        .expect("web_fetch output");

    assert_eq!(fetched.url, server.plain_url());
    assert_eq!(fetched.status_code, 200);
    assert_eq!(fetched.title, None);
    assert!(!fetched.extracted_from_html);
    assert_eq!(fetched.body, "plain weather text");
}

#[test]
fn web_search_parses_duckduckgo_html_with_extra_anchor_attributes() {
    let html = "\
        <html><body>\
        <a rel=\"nofollow\" class=\"result__a\" href=\"//duckduckgo.com/l/?uddg=https%3A%2F%2Fyandex.ru%2Fpogoda%2Fru%2Fmoscow&amp;rut=abc\">Погода в Москве — Прогноз</a>\
        <a class=\"result__snippet\" href=\"//duckduckgo.com/l/?uddg=https%3A%2F%2Fyandex.ru%2Fpogoda%2Fru%2Fmoscow&amp;rut=abc\">Сейчас <b>в</b> <b>Москве</b> +7°</a>\
        </body></html>";

    let results = super::parse_search_results(html, "https://duckduckgo.com/html/?q=weather")
        .expect("parse search results");

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Погода в Москве — Прогноз");
    assert_eq!(results[0].url, "https://yandex.ru/pogoda/ru/moscow");
    assert_eq!(results[0].snippet.as_deref(), Some("Сейчас в Москве +7°"));
}

#[test]
fn web_search_can_use_searxng_json_backend() {
    let server = TestHttpServer::spawn();
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = WorkspaceRef::new(temp.path());
    let mut runtime = ToolRuntime::with_web_client(
        workspace,
        WebToolClient::for_tests_with_search_backend(
            WebSearchBackend::SearxngJson,
            server.base_url(),
            server.search_url(),
        ),
    );

    let searched = runtime
        .invoke(ToolCall::WebSearch(WebSearchInput {
            query: "agent runtime".to_string(),
            limit: 1,
        }))
        .expect("web_search")
        .into_web_search()
        .expect("web_search output");

    assert_eq!(searched.results.len(), 1);
    assert_eq!(searched.results[0].title, "SearXNG result");
    assert_eq!(searched.results[0].url, "https://example.test/searxng");
    assert_eq!(
        searched.results[0].snippet.as_deref(),
        Some("JSON search result")
    );
}

struct TestHttpServer {
    base_url: String,
    search_url: String,
}

impl TestHttpServer {
    fn spawn() -> Self {
        Self::spawn_with_requests(2)
    }

    fn spawn_with_requests(request_count: usize) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let address = listener.local_addr().expect("local addr");
        let base_url = format!("http://{}", address);
        let search_url = format!("{}/search", base_url);

        thread::spawn(move || {
            for _ in 0..request_count {
                let (mut stream, _) = listener.accept().expect("accept");
                let mut buffer = [0_u8; 4096];
                let bytes = stream.read(&mut buffer).expect("read request");
                let request = String::from_utf8_lossy(&buffer[..bytes]);
                let path = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("/");

                let (content_type, body) = if path.starts_with("/search")
                    && path.contains("format=json")
                {
                    (
                        "application/json",
                        "{\"results\":[{\"title\":\"SearXNG result\",\"url\":\"https://example.test/searxng\",\"content\":\"JSON search result\"},{\"title\":\"Second\",\"url\":\"https://example.test/second\",\"content\":\"Ignored by limit\"}]}",
                    )
                } else if path.starts_with("/search") {
                    (
                        "text/html; charset=utf-8",
                        "<html><body>\
                     <a class=\"result__a\" href=\"https://example.test/docs\">Agent runtime docs</a>\
                     <a class=\"result__snippet\">Typed tools and run engine</a>\
                     <a class=\"result__a\" href=\"https://example.test/blog\">Blog post</a>\
                     <a class=\"result__snippet\">Web tool coverage</a>\
                     </body></html>",
                    )
                } else if path == "/plain" {
                    ("text/plain; charset=utf-8", "plain weather text")
                } else {
                    (
                        "text/html; charset=utf-8",
                        "<html><head><title>Agent runtime page</title>\
                     <style>.hidden{display:none}</style>\
                     <script>console.log('skip me')</script></head>\
                     <body><main><h1>Agent runtime page</h1>\
                     <p>Agent runtime page body &amp; notes.</p><p><a href=\"https://example.test/spec\">Reference spec</a></p>\
                     <p>Second paragraph.</p></main></body></html>",
                    )
                };

                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    content_type,
                    body.len(),
                    body
                )
                .expect("write response");
            }
        });

        Self {
            base_url,
            search_url,
        }
    }

    fn base_url(&self) -> &str {
        &self.base_url
    }

    fn search_url(&self) -> &str {
        &self.search_url
    }

    fn page_url(&self) -> String {
        format!("{}/page", self.base_url)
    }

    fn plain_url(&self) -> String {
        format!("{}/plain", self.base_url)
    }
}
