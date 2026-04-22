use agent_runtime::skills::{scan_skill_catalog, scan_skill_catalog_with_overrides};
use std::fs;

#[test]
fn skills_catalog_scans_skill_md_and_parses_name_and_description() {
    let temp = tempfile::tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    let skill_dir = skills_dir.join("rust-debug");
    fs::create_dir_all(&skill_dir).expect("create skill dir");
    fs::write(
        skill_dir.join("SKILL.md"),
        r#"---
name: Rust Debug
description: Diagnose Rust runtime failures
---

# Rust Debug

Use this when debugging Rust runtime failures.
"#,
    )
    .expect("write skill");

    let catalog = scan_skill_catalog(&skills_dir).expect("scan catalog");

    assert_eq!(catalog.entries.len(), 1);
    let entry = &catalog.entries[0];
    assert_eq!(entry.name, "Rust Debug");
    assert_eq!(entry.description, "Diagnose Rust runtime failures");
    assert_eq!(entry.skill_dir, skill_dir);
    assert!(entry.skill_md_path.ends_with("SKILL.md"));
}

#[test]
fn skills_catalog_skips_malformed_frontmatter_but_keeps_valid_entries() {
    let temp = tempfile::tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    let valid_dir = skills_dir.join("valid-skill");
    let invalid_dir = skills_dir.join("broken-skill");
    fs::create_dir_all(&valid_dir).expect("create valid dir");
    fs::create_dir_all(&invalid_dir).expect("create invalid dir");
    fs::write(
        valid_dir.join("SKILL.md"),
        r#"---
name: Valid Skill
description: Works correctly
---
"#,
    )
    .expect("write valid skill");
    fs::write(
        invalid_dir.join("SKILL.md"),
        r#"---
name = "Broken"
description = "invalid yaml frontmatter"
---
"#,
    )
    .expect("write invalid skill");

    let catalog = scan_skill_catalog(&skills_dir).expect("scan catalog");

    assert_eq!(catalog.entries.len(), 1);
    assert_eq!(catalog.entries[0].name, "Valid Skill");
    assert_eq!(catalog.skipped.len(), 1);
    assert!(catalog.skipped[0].reason.contains("frontmatter"));
}

#[test]
fn skills_catalog_only_loads_summary_metadata_and_ignores_missing_resources() {
    let temp = tempfile::tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    let skill_dir = skills_dir.join("ops-triage");
    fs::create_dir_all(skill_dir.join("references")).expect("create skill tree");
    fs::write(
        skill_dir.join("SKILL.md"),
        r#"---
name: Ops Triage
description: Triage production incidents
---

See [Missing Reference](references/missing.md).
"#,
    )
    .expect("write skill");

    let catalog = scan_skill_catalog(&skills_dir).expect("scan catalog");

    assert_eq!(catalog.entries.len(), 1);
    assert!(catalog.skipped.is_empty());
}

#[test]
fn skills_catalog_prefers_agent_local_override_by_skill_name() {
    let temp = tempfile::tempdir().expect("tempdir");
    let global_dir = temp.path().join("global-skills");
    let local_dir = temp.path().join("agent-skills");
    let global_skill_dir = global_dir.join("rust-debug");
    let local_skill_dir = local_dir.join("rust-debug-local");
    fs::create_dir_all(&global_skill_dir).expect("create global skill dir");
    fs::create_dir_all(&local_skill_dir).expect("create local skill dir");

    fs::write(
        global_skill_dir.join("SKILL.md"),
        r#"---
name: Rust Debug
description: Global version
---
"#,
    )
    .expect("write global skill");
    fs::write(
        local_skill_dir.join("SKILL.md"),
        r#"---
name: Rust Debug
description: Agent-local override
---
"#,
    )
    .expect("write local skill");

    let catalog = scan_skill_catalog_with_overrides(&global_dir, Some(&local_dir))
        .expect("scan merged catalog");

    assert_eq!(catalog.entries.len(), 1);
    assert_eq!(catalog.entries[0].description, "Agent-local override");
    assert_eq!(catalog.entries[0].skill_dir, local_skill_dir);
}
