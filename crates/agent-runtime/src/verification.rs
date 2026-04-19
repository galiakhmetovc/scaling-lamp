use std::error::Error;
use std::fmt;

const DEFAULT_REQUIRED_CHECKS: [&str; 3] = ["fmt", "clippy", "test"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceBundle {
    pub id: String,
    pub run_id: String,
    pub required_checks: Vec<VerificationCheck>,
    pub command_outputs: Vec<CommandEvidence>,
    pub test_results: Vec<TestEvidence>,
    pub artifact_refs: Vec<String>,
    pub scoring_signals: Vec<ScoringSignal>,
    pub residual_risks: Vec<ResidualRisk>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationCheck {
    pub id: String,
    pub outcome: CheckOutcome,
    pub summary: Option<String>,
    pub recorded_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandEvidence {
    pub id: String,
    pub command: String,
    pub output: String,
    pub exit_code: i32,
    pub recorded_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestEvidence {
    pub id: String,
    pub suite: String,
    pub passed: u32,
    pub failed: u32,
    pub summary: String,
    pub recorded_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoringSignal {
    pub id: String,
    pub description: String,
    pub score: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResidualRisk {
    pub id: String,
    pub description: String,
    pub severity: RiskSeverity,
    pub accepted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckOutcome {
    Pending,
    Passed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskSeverity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationStatus {
    Pending,
    Passed,
    Failed,
    NeedsReview,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationReport {
    pub status: VerificationStatus,
    pub missing_required_checks: Vec<String>,
    pub failing_checks: Vec<String>,
    pub open_risks: Vec<String>,
    pub total_score: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerificationError {
    EmptyIdentifier,
}

impl Default for EvidenceBundle {
    fn default() -> Self {
        Self::new("bundle-bootstrap", "run-bootstrap", 0)
    }
}

impl EvidenceBundle {
    pub fn new(id: impl Into<String>, run_id: impl Into<String>, created_at: i64) -> Self {
        Self {
            id: id.into(),
            run_id: run_id.into(),
            required_checks: DEFAULT_REQUIRED_CHECKS
                .into_iter()
                .map(|id| VerificationCheck {
                    id: id.to_string(),
                    outcome: CheckOutcome::Pending,
                    summary: None,
                    recorded_at: None,
                })
                .collect(),
            command_outputs: Vec::new(),
            test_results: Vec::new(),
            artifact_refs: Vec::new(),
            scoring_signals: Vec::new(),
            residual_risks: Vec::new(),
            created_at,
            updated_at: created_at,
        }
    }

    pub fn record_check(
        &mut self,
        id: &str,
        outcome: CheckOutcome,
        summary: Option<&str>,
        at: i64,
    ) -> Result<(), VerificationError> {
        validate_identifier(id)?;
        if let Some(existing) = self.required_checks.iter_mut().find(|check| check.id == id) {
            existing.outcome = outcome;
            existing.summary = summary.map(str::to_owned);
            existing.recorded_at = Some(at);
        } else {
            self.required_checks.push(VerificationCheck {
                id: id.to_string(),
                outcome,
                summary: summary.map(str::to_owned),
                recorded_at: Some(at),
            });
        }

        self.touch(at);
        Ok(())
    }

    pub fn record_command(
        &mut self,
        id: &str,
        command: &str,
        output: &str,
        exit_code: i32,
        at: i64,
    ) {
        if let Some(existing) = self.command_outputs.iter_mut().find(|entry| entry.id == id) {
            existing.command = command.to_string();
            existing.output = output.to_string();
            existing.exit_code = exit_code;
            existing.recorded_at = at;
        } else {
            self.command_outputs.push(CommandEvidence {
                id: id.to_string(),
                command: command.to_string(),
                output: output.to_string(),
                exit_code,
                recorded_at: at,
            });
        }

        self.touch(at);
    }

    pub fn record_test_result(
        &mut self,
        id: &str,
        suite: &str,
        passed: u32,
        failed: u32,
        summary: &str,
        at: i64,
    ) {
        if let Some(existing) = self.test_results.iter_mut().find(|entry| entry.id == id) {
            existing.suite = suite.to_string();
            existing.passed = passed;
            existing.failed = failed;
            existing.summary = summary.to_string();
            existing.recorded_at = at;
        } else {
            self.test_results.push(TestEvidence {
                id: id.to_string(),
                suite: suite.to_string(),
                passed,
                failed,
                summary: summary.to_string(),
                recorded_at: at,
            });
        }

        self.touch(at);
    }

    pub fn add_artifact_ref(&mut self, artifact_ref: &str) {
        if !self
            .artifact_refs
            .iter()
            .any(|existing| existing == artifact_ref)
        {
            self.artifact_refs.push(artifact_ref.to_string());
        }
    }

    pub fn record_scoring_signal(&mut self, id: &str, description: &str, score: i32) {
        if let Some(existing) = self
            .scoring_signals
            .iter_mut()
            .find(|signal| signal.id == id)
        {
            existing.description = description.to_string();
            existing.score = score;
        } else {
            self.scoring_signals.push(ScoringSignal {
                id: id.to_string(),
                description: description.to_string(),
                score,
            });
        }
    }

    pub fn add_residual_risk(
        &mut self,
        id: &str,
        description: &str,
        severity: RiskSeverity,
        accepted: bool,
    ) {
        if let Some(existing) = self.residual_risks.iter_mut().find(|risk| risk.id == id) {
            existing.description = description.to_string();
            existing.severity = severity;
            existing.accepted = accepted;
        } else {
            self.residual_risks.push(ResidualRisk {
                id: id.to_string(),
                description: description.to_string(),
                severity,
                accepted,
            });
        }
    }

    pub fn evaluate(&self) -> VerificationReport {
        let missing_required_checks = self
            .required_checks
            .iter()
            .filter(|check| matches!(check.outcome, CheckOutcome::Pending | CheckOutcome::Skipped))
            .map(|check| check.id.clone())
            .collect::<Vec<_>>();
        let mut failing_checks = self
            .required_checks
            .iter()
            .filter(|check| check.outcome == CheckOutcome::Failed)
            .map(|check| check.id.clone())
            .collect::<Vec<_>>();
        let open_risks = self
            .residual_risks
            .iter()
            .filter(|risk| !risk.accepted)
            .map(|risk| risk.id.clone())
            .collect::<Vec<_>>();

        for suite in self.test_results.iter().filter(|suite| suite.failed > 0) {
            let suite_ref = format!("test:{}", suite.id);
            if !failing_checks.contains(&suite_ref) {
                failing_checks.push(suite_ref);
            }
        }

        let status = if !failing_checks.is_empty() {
            VerificationStatus::Failed
        } else if !missing_required_checks.is_empty() || !open_risks.is_empty() {
            if self
                .required_checks
                .iter()
                .all(|check| check.outcome == CheckOutcome::Pending)
            {
                VerificationStatus::Pending
            } else {
                VerificationStatus::NeedsReview
            }
        } else {
            VerificationStatus::Passed
        };

        VerificationReport {
            status,
            missing_required_checks,
            failing_checks,
            open_risks,
            total_score: self.scoring_signals.iter().map(|signal| signal.score).sum(),
        }
    }

    pub fn refs(&self) -> Vec<String> {
        let mut refs = vec![format!("bundle:{}", self.id)];

        refs.extend(
            self.required_checks
                .iter()
                .filter(|check| check.recorded_at.is_some())
                .map(|check| format!("check:{}", check.id)),
        );
        refs.extend(
            self.command_outputs
                .iter()
                .map(|command| format!("command:{}", command.id)),
        );
        refs.extend(
            self.test_results
                .iter()
                .map(|test| format!("test:{}", test.id)),
        );
        refs.extend(self.artifact_refs.iter().cloned());
        refs.extend(
            self.residual_risks
                .iter()
                .map(|risk| format!("risk:{}", risk.id)),
        );

        refs
    }

    fn touch(&mut self, at: i64) {
        self.updated_at = at;
    }
}

fn validate_identifier(id: &str) -> Result<(), VerificationError> {
    if id.trim().is_empty() {
        return Err(VerificationError::EmptyIdentifier);
    }

    Ok(())
}

impl fmt::Display for VerificationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyIdentifier => write!(formatter, "verification identifiers cannot be blank"),
        }
    }
}

impl Error for VerificationError {}

#[cfg(test)]
mod tests {
    use super::{CheckOutcome, EvidenceBundle, RiskSeverity, VerificationStatus};

    #[test]
    fn evidence_bundle_passes_only_when_required_checks_are_green_and_risks_are_closed() {
        let mut bundle = EvidenceBundle::new("bundle-1", "run-1", 10);

        bundle
            .record_check("fmt", CheckOutcome::Passed, Some("rustfmt clean"), 11)
            .expect("record fmt");
        bundle
            .record_check("clippy", CheckOutcome::Passed, Some("clippy clean"), 12)
            .expect("record clippy");
        bundle
            .record_check("test", CheckOutcome::Passed, Some("all tests green"), 13)
            .expect("record test");
        bundle.record_command(
            "cmd-1",
            "cargo test --workspace --all-features",
            "workspace ok",
            0,
            13,
        );
        bundle.record_test_result("test-1", "workspace", 52, 0, "all tests green", 13);
        bundle.add_artifact_ref("artifact:verification-report");
        bundle.record_scoring_signal("scope", "changes stayed inside owned crates", 5);

        let report = bundle.evaluate();

        assert_eq!(report.status, VerificationStatus::Passed);
        assert!(report.missing_required_checks.is_empty());
        assert!(report.failing_checks.is_empty());
        assert!(report.open_risks.is_empty());
        assert_eq!(report.total_score, 5);
        assert!(bundle.refs().contains(&"bundle:bundle-1".to_string()));
        assert!(
            bundle
                .refs()
                .contains(&"artifact:verification-report".to_string())
        );
    }

    #[test]
    fn evidence_bundle_surfaces_missing_checks_failed_checks_and_open_risks() {
        let mut bundle = EvidenceBundle::new("bundle-1", "run-1", 10);

        bundle
            .record_check("fmt", CheckOutcome::Passed, Some("rustfmt clean"), 11)
            .expect("record fmt");
        bundle
            .record_check("clippy", CheckOutcome::Failed, Some("unused imports"), 12)
            .expect("record clippy");
        bundle.add_residual_risk(
            "risk-1",
            "manual browser flow was not executed",
            RiskSeverity::Medium,
            false,
        );

        let report = bundle.evaluate();

        assert_eq!(report.status, VerificationStatus::Failed);
        assert_eq!(report.missing_required_checks, vec!["test".to_string()]);
        assert_eq!(report.failing_checks, vec!["clippy".to_string()]);
        assert_eq!(report.open_risks, vec!["risk-1".to_string()]);
    }
}
