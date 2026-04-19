#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContextSummary {
    pub session_id: String,
    pub summary_text: String,
    pub covered_message_count: u32,
    pub summary_token_estimate: u32,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompactionPolicy {
    pub min_messages: usize,
    pub keep_tail_messages: usize,
    pub max_output_tokens: u32,
    pub max_summary_chars: usize,
}

impl ContextSummary {
    pub fn system_message_text(&self) -> String {
        format!("Compacted session summary:\n{}", self.summary_text)
    }
}

impl Default for CompactionPolicy {
    fn default() -> Self {
        Self {
            min_messages: 8,
            keep_tail_messages: 6,
            max_output_tokens: 1024,
            max_summary_chars: 4_096,
        }
    }
}

impl CompactionPolicy {
    pub fn should_compact(self, total_messages: usize) -> bool {
        total_messages >= self.min_messages && total_messages > self.keep_tail_messages
    }

    pub fn covered_message_count(self, total_messages: usize) -> usize {
        total_messages.saturating_sub(self.keep_tail_messages)
    }

    pub fn trim_summary_text(self, summary_text: &str) -> String {
        let trimmed = summary_text.trim();
        if trimmed.chars().count() <= self.max_summary_chars {
            return trimmed.to_string();
        }

        let mut compact = trimmed
            .chars()
            .take(self.max_summary_chars.saturating_sub(1))
            .collect::<String>();
        compact.push('…');
        compact
    }
}

pub fn approximate_token_count(content: &str) -> u32 {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return 0;
    }

    ((trimmed.chars().count() as u32) / 4).saturating_add(1)
}

#[cfg(test)]
mod tests {
    use super::{CompactionPolicy, ContextSummary, approximate_token_count};

    #[test]
    fn policy_uses_expected_defaults() {
        let policy = CompactionPolicy::default();
        assert_eq!(policy.min_messages, 8);
        assert_eq!(policy.keep_tail_messages, 6);
        assert_eq!(policy.max_output_tokens, 1024);
        assert_eq!(policy.max_summary_chars, 4_096);
    }

    #[test]
    fn policy_only_compacts_when_threshold_is_met() {
        let policy = CompactionPolicy::default();
        assert!(!policy.should_compact(7));
        assert!(policy.should_compact(8));
        assert_eq!(policy.covered_message_count(8), 2);
    }

    #[test]
    fn context_summary_formats_system_message_text() {
        let summary = ContextSummary {
            session_id: "session-1".to_string(),
            summary_text: "Current goal: finish the feature.".to_string(),
            covered_message_count: 2,
            summary_token_estimate: 8,
            updated_at: 10,
        };

        assert_eq!(
            summary.system_message_text(),
            "Compacted session summary:\nCurrent goal: finish the feature."
        );
    }

    #[test]
    fn approximate_token_count_is_zero_for_blank_text() {
        assert_eq!(approximate_token_count("   "), 0);
        assert!(approximate_token_count("hello world") > 0);
    }
}
