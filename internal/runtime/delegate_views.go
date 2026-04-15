package runtime

import "teamd/internal/runtime/projections"

func (a *Agent) delegateProjection() *projections.DelegateProjection {
	for _, projection := range a.Projections {
		delegateProjection, ok := projection.(*projections.DelegateProjection)
		if ok {
			return delegateProjection
		}
	}
	return nil
}

func (a *Agent) delegateView(delegateID string) (DelegateView, bool) {
	projection := a.delegateProjection()
	if projection == nil {
		return DelegateView{}, false
	}
	view, ok := projection.View(delegateID)
	if !ok {
		return DelegateView{}, false
	}
	return toRuntimeDelegateView(view), true
}

func (a *Agent) delegateHandoff(delegateID string) (DelegateHandoff, bool) {
	projection := a.delegateProjection()
	if projection == nil {
		return DelegateHandoff{}, false
	}
	handoff, ok := projection.Handoff(delegateID)
	if !ok {
		return DelegateHandoff{}, false
	}
	return toRuntimeDelegateHandoff(handoff), true
}

func toRuntimeDelegateView(view projections.DelegateView) DelegateView {
	return DelegateView{
		DelegateID:        view.DelegateID,
		Backend:           DelegateBackend(view.Backend),
		OwnerSessionID:    view.OwnerSessionID,
		LastRunID:         view.LastRunID,
		Status:            DelegateStatus(view.Status),
		ArtifactRefs:      toRuntimeDelegateArtifacts(view.ArtifactRefs),
		EventRefs:         toRuntimeDelegateEvents(view.EventRefs),
		PolicySnapshot:    cloneAnyMap(view.PolicySnapshot),
		LastError:         view.LastError,
		CreatedAt:         view.CreatedAt,
		UpdatedAt:         view.UpdatedAt,
		LastMessageAt:     view.LastMessageAt,
		ClosedAt:          view.ClosedAt,
	}
}

func toRuntimeDelegateHandoff(handoff projections.DelegateHandoffView) DelegateHandoff {
	return DelegateHandoff{
		DelegateID:          handoff.DelegateID,
		Backend:             DelegateBackend(handoff.Backend),
		LastRunID:           handoff.LastRunID,
		Summary:             handoff.Summary,
		Artifacts:           toRuntimeDelegateArtifacts(handoff.Artifacts),
		PromotedFacts:       append([]string(nil), handoff.PromotedFacts...),
		OpenQuestions:       append([]string(nil), handoff.OpenQuestions...),
		RecommendedNextStep: handoff.RecommendedNextStep,
		CreatedAt:           handoff.CreatedAt,
		UpdatedAt:           handoff.UpdatedAt,
	}
}

func toRuntimeDelegateArtifacts(artifacts []projections.DelegateArtifactRefView) []DelegateArtifactRef {
	if len(artifacts) == 0 {
		return nil
	}
	out := make([]DelegateArtifactRef, 0, len(artifacts))
	for _, artifact := range artifacts {
		out = append(out, DelegateArtifactRef{
			Ref:         artifact.Ref,
			Kind:        artifact.Kind,
			Label:       artifact.Label,
			ContentType: artifact.ContentType,
		})
	}
	return out
}

func toRuntimeDelegateEvents(events []projections.DelegateEventRefView) []DelegateEventRef {
	if len(events) == 0 {
		return nil
	}
	out := make([]DelegateEventRef, 0, len(events))
	for _, event := range events {
		out = append(out, DelegateEventRef{
			EventID: event.EventID,
			Kind:    event.Kind,
		})
	}
	return out
}

func cloneAnyMap(input map[string]any) map[string]any {
	if input == nil {
		return nil
	}
	out := make(map[string]any, len(input))
	for key, value := range input {
		out[key] = value
	}
	return out
}
