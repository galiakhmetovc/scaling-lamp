use agent_runtime::session::{SessionSettings, TranscriptEntry};
use agent_runtime::skills::{
    SkillActivationMode, SkillCatalog, SkillSummary, resolve_session_skill_status,
};
use std::path::PathBuf;

fn skill(name: &str, description: &str) -> SkillSummary {
    SkillSummary {
        name: name.to_string(),
        description: description.to_string(),
        skill_dir: PathBuf::from(format!("skills/{name}")),
        skill_md_path: PathBuf::from(format!("skills/{name}/SKILL.md")),
    }
}

#[test]
fn skills_activation_auto_matches_user_context_and_manual_disable_wins() {
    let catalog = SkillCatalog {
        entries: vec![
            skill(
                "rust-debug",
                "Debug Rust compiler errors, cargo failures, and clippy regressions.",
            ),
            skill(
                "postgres",
                "Investigate PostgreSQL migrations, indexes, and query plans.",
            ),
        ],
        skipped: Vec::new(),
    };
    let settings = SessionSettings {
        disabled_skills: vec!["postgres".to_string()],
        ..SessionSettings::default()
    };
    let transcript = vec![TranscriptEntry::user(
        "user-1",
        "session-1",
        None,
        "Помоги отладить rust compile error и clippy warning",
        10,
    )];

    let statuses = resolve_session_skill_status(&catalog, &settings, "Compiler Help", &transcript);

    assert_eq!(statuses.len(), 2);
    assert_eq!(statuses[0].name, "postgres");
    assert_eq!(statuses[0].mode, SkillActivationMode::Disabled);
    assert_eq!(statuses[1].name, "rust-debug");
    assert_eq!(statuses[1].mode, SkillActivationMode::Automatic);
}

#[test]
fn skills_activation_manual_enable_is_session_scoped_and_overrides_missing_auto_match() {
    let catalog = SkillCatalog {
        entries: vec![skill(
            "release-checklist",
            "Ship production releases and verify rollout checklists.",
        )],
        skipped: Vec::new(),
    };
    let settings = SessionSettings {
        enabled_skills: vec!["release-checklist".to_string()],
        ..SessionSettings::default()
    };

    let statuses = resolve_session_skill_status(&catalog, &settings, "General Chat", &[]);

    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses[0].name, "release-checklist");
    assert_eq!(statuses[0].mode, SkillActivationMode::Manual);
}
