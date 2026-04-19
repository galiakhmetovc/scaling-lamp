use agent_persistence::TranscriptRecord;
use agent_runtime::context::{ContextSummary, approximate_token_count};
use agent_runtime::prompt::SessionHead;
use agent_runtime::run::{RunSnapshot, RunStatus};
use agent_runtime::session::Session;

pub(crate) fn build_session_head(
    session: &Session,
    transcripts: &[TranscriptRecord],
    context_summary: Option<&ContextSummary>,
    runs: &[RunSnapshot],
) -> SessionHead {
    let message_count = transcripts.len();
    let covered_message_count = context_summary
        .map(|summary| summary.covered_message_count as usize)
        .unwrap_or(0)
        .min(message_count);
    let uncovered_transcript_tokens = transcripts
        .iter()
        .skip(covered_message_count)
        .map(|record| approximate_token_count(record.content.as_str()))
        .sum::<u32>();
    let summary_tokens = context_summary
        .map(|summary| summary.summary_token_estimate)
        .unwrap_or(0);
    let pending_approval_count = runs
        .iter()
        .filter(|run| {
            run.session_id == session.id
                && run.status == RunStatus::WaitingApproval
                && !run.pending_approvals.is_empty()
        })
        .map(|run| run.pending_approvals.len())
        .sum::<usize>();

    SessionHead {
        session_id: session.id.clone(),
        title: session.title.clone(),
        message_count,
        context_tokens: uncovered_transcript_tokens + summary_tokens,
        compactifications: session.settings.compactifications,
        summary_covered_message_count: covered_message_count as u32,
        pending_approval_count,
        last_user_preview: transcripts
            .iter()
            .rev()
            .find(|record| record.kind == "user")
            .map(|record| preview_text(record.content.as_str(), 96)),
        last_assistant_preview: transcripts
            .iter()
            .rev()
            .find(|record| record.kind == "assistant")
            .map(|record| preview_text(record.content.as_str(), 96)),
    }
}

pub(crate) fn preview_text(content: &str, limit: usize) -> String {
    let collapsed = content.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= limit {
        return collapsed;
    }
    let mut preview = collapsed
        .chars()
        .take(limit.saturating_sub(1))
        .collect::<String>();
    preview.push('…');
    preview
}
