use crate::session::{MessageRole, SessionSettings, TranscriptEntry};
use crate::skills::catalog::{SkillCatalog, SkillSummary};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillActivationMode {
    Inactive,
    Automatic,
    Manual,
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SessionSkillStatus {
    pub name: String,
    pub description: String,
    pub mode: SkillActivationMode,
}

pub fn resolve_session_skill_status(
    catalog: &SkillCatalog,
    settings: &SessionSettings,
    session_title: &str,
    transcript: &[TranscriptEntry],
) -> Vec<SessionSkillStatus> {
    let context = build_activation_context(session_title, transcript);
    let context_tokens = tokenize(&context);
    let enabled = normalized_names(&settings.enabled_skills);
    let disabled = normalized_names(&settings.disabled_skills);

    let mut statuses = catalog
        .entries
        .iter()
        .map(|skill| {
            let normalized_name = normalize_token(&skill.name);
            let mode = if disabled.iter().any(|existing| existing == &normalized_name) {
                SkillActivationMode::Disabled
            } else if enabled.iter().any(|existing| existing == &normalized_name) {
                SkillActivationMode::Manual
            } else if auto_matches(skill, &context_tokens) {
                SkillActivationMode::Automatic
            } else {
                SkillActivationMode::Inactive
            };
            SessionSkillStatus {
                name: skill.name.clone(),
                description: skill.description.clone(),
                mode,
            }
        })
        .collect::<Vec<_>>();
    statuses.sort_by(|left, right| left.name.cmp(&right.name));
    statuses
}

fn build_activation_context(session_title: &str, transcript: &[TranscriptEntry]) -> String {
    let mut lines = Vec::new();
    if !session_title.trim().is_empty() {
        lines.push(session_title.trim().to_string());
    }
    let mut recent_user_lines = transcript
        .iter()
        .filter(|entry| entry.role == MessageRole::User)
        .rev()
        .take(6)
        .map(|entry| entry.content.trim().to_string())
        .collect::<Vec<_>>();
    recent_user_lines.reverse();
    lines.extend(recent_user_lines);
    lines.join("\n")
}

fn auto_matches(skill: &SkillSummary, context_tokens: &[String]) -> bool {
    let name_tokens = tokenize(&skill.name);
    let description_tokens = tokenize(&skill.description);
    let name_matches = overlap_count(&name_tokens, context_tokens);
    let description_matches = overlap_count(&description_tokens, context_tokens);

    name_matches >= 1 || description_matches >= 2 || name_matches + description_matches >= 2
}

fn overlap_count(skill_tokens: &[String], context_tokens: &[String]) -> usize {
    skill_tokens
        .iter()
        .filter(|token| context_tokens.iter().any(|candidate| candidate == *token))
        .count()
}

fn normalized_names(raw: &[String]) -> Vec<String> {
    let mut names = raw
        .iter()
        .map(|value| normalize_token(value))
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names
}

fn tokenize(input: &str) -> Vec<String> {
    let mut tokens = input
        .split(|ch: char| !ch.is_alphanumeric())
        .map(normalize_token)
        .filter(|token| token.len() >= 3)
        .collect::<Vec<_>>();
    tokens.sort();
    tokens.dedup();
    tokens
}

fn normalize_token(input: &str) -> String {
    input.trim().to_lowercase()
}
