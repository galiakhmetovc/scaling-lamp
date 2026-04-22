use agent_runtime::prompt::{PromptAssembly, PromptAssemblyInput, SessionHead};
use agent_runtime::provider::ProviderMessage;
use agent_runtime::session::MessageRole;

#[test]
fn skills_prompt_places_active_skill_blocks_between_agents_and_session_head() {
    let messages = PromptAssembly::build_messages(PromptAssemblyInput {
        system_prompt: Some("system".to_string()),
        agents_prompt: Some("agents".to_string()),
        active_skill_prompts: vec![
            "# skill one\nUse cargo first.".to_string(),
            "# skill two\nPrefer focused diffs.".to_string(),
        ],
        session_head: Some(SessionHead {
            session_id: "session-1".to_string(),
            title: "Skill Prompt".to_string(),
            message_count: 1,
            context_tokens: 5,
            compactifications: 0,
            summary_covered_message_count: 0,
            pending_approval_count: 0,
            last_user_preview: Some("hello".to_string()),
            last_assistant_preview: None,
            recent_filesystem_activity: Vec::new(),
            recent_process_activity: Vec::new(),
            workspace_tree: Vec::new(),
            workspace_tree_truncated: false,
        }),
        plan_snapshot: None,
        context_summary: None,
        context_offload: None,
        transcript_messages: vec![ProviderMessage {
            role: MessageRole::User,
            content: "hello".to_string(),
        }],
    });

    assert_eq!(messages.len(), 6);
    assert_eq!(messages[0].content, "system");
    assert_eq!(messages[1].content, "agents");
    assert!(messages[2].content.contains("# skill one"));
    assert!(messages[3].content.contains("# skill two"));
    assert!(messages[4].content.contains("Session: Skill Prompt"));
    assert_eq!(messages[5].role, MessageRole::User);
}
