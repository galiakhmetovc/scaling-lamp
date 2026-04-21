use agent_runtime::delegation::DelegateRequest;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DelegationExecutorKind {
    LocalChildSession,
    RemoteA2A,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DelegationDispatch {
    pub kind: DelegationExecutorKind,
    pub owner_selector: String,
}

impl DelegationDispatch {
    pub fn blocked_reason(&self) -> Option<String> {
        match self.kind {
            DelegationExecutorKind::LocalChildSession => None,
            DelegationExecutorKind::RemoteA2A => Some(format!(
                "remote delegation executor is not configured for owner {}",
                self.owner_selector
            )),
        }
    }
}

pub fn resolve_delegate_dispatch(request: &DelegateRequest) -> DelegationDispatch {
    let owner_selector = request.owner.trim().to_string();
    let kind = if owner_selector.starts_with("a2a:") {
        DelegationExecutorKind::RemoteA2A
    } else {
        DelegationExecutorKind::LocalChildSession
    };
    DelegationDispatch {
        kind,
        owner_selector,
    }
}

pub fn a2a_peer_id(owner_selector: &str) -> Option<&str> {
    owner_selector
        .strip_prefix("a2a:")
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_runtime::delegation::DelegateWriteScope;

    fn request(owner: &str) -> DelegateRequest {
        DelegateRequest::new(
            "delegate-1",
            "run-parent",
            "run-child",
            "judge",
            "Review the artifacts and return a verdict.",
            vec!["reports/judge.md".to_string()],
            DelegateWriteScope::new(vec!["reports".to_string()]).expect("write scope"),
            "Short verdict",
            owner,
        )
        .expect("delegate request")
    }

    #[test]
    fn delegate_routing_resolves_local_child_owner_to_local_executor() {
        let dispatch = resolve_delegate_dispatch(&request("local-child"));
        assert_eq!(dispatch.kind, DelegationExecutorKind::LocalChildSession);
        assert_eq!(dispatch.blocked_reason(), None);
    }

    #[test]
    fn delegate_routing_resolves_a2a_owner_to_remote_executor() {
        let dispatch = resolve_delegate_dispatch(&request("a2a:judge"));
        assert_eq!(dispatch.kind, DelegationExecutorKind::RemoteA2A);
        assert_eq!(a2a_peer_id(&dispatch.owner_selector), Some("judge"));
        assert_eq!(
            dispatch.blocked_reason().as_deref(),
            Some("remote delegation executor is not configured for owner a2a:judge")
        );
    }

    #[test]
    fn delegate_routing_defaults_unknown_owner_to_local_executor() {
        let dispatch = resolve_delegate_dispatch(&request("judge-helper"));
        assert_eq!(dispatch.kind, DelegationExecutorKind::LocalChildSession);
        assert_eq!(dispatch.blocked_reason(), None);
    }
}
