pub mod delegation;
pub mod memory;
pub mod mission;
pub mod plan;
pub mod provider;
pub mod run;
pub mod session;
pub mod tool;
pub mod verification;
pub mod workspace;

#[derive(Debug, Clone, Default)]
pub struct RuntimeScaffold {
    pub delegation: delegation::DelegateHandle,
    pub memory: memory::MemoryIndex,
    pub mission: mission::MissionSpec,
    pub plan: plan::PlanSnapshot,
    pub provider: provider::ProviderDescriptor,
    pub run: run::RunSnapshot,
    pub session: session::Session,
    pub tools: tool::ToolCatalog,
    pub verification: verification::EvidenceBundle,
    pub workspace: workspace::WorkspaceRef,
}

impl RuntimeScaffold {
    pub const COMPONENTS: [&str; 10] = [
        "delegation",
        "memory",
        "mission",
        "plan",
        "provider",
        "run",
        "session",
        "tools",
        "verification",
        "workspace",
    ];

    pub fn component_count(&self) -> usize {
        Self::COMPONENTS.len()
    }
}

#[cfg(test)]
mod tests {
    use super::RuntimeScaffold;
    use crate::run::RunStatus;
    use std::path::PathBuf;

    #[test]
    fn scaffold_exposes_expected_defaults() {
        let scaffold = RuntimeScaffold::default();
        let mut workspace = scaffold.workspace.clone();

        assert_eq!(
            scaffold.component_count(),
            RuntimeScaffold::COMPONENTS.len()
        );
        assert_eq!(scaffold.run.status, RunStatus::Queued);
        assert_eq!(scaffold.tools.families, ["fs", "exec", "shell_snippet"]);
        workspace.root.push("runs");
        assert_eq!(workspace.root, PathBuf::from("./runs"));
    }
}
