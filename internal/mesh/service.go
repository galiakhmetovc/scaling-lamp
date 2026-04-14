package mesh

import (
	"context"
	"encoding/json"
	"fmt"
	"log/slog"
	"strings"
	"time"
)

type Executor interface {
	Execute(context.Context, Envelope) (CandidateReply, error)
}

type ClarifierRuntime interface {
	Clarify(context.Context, ClarificationInput) (ClarifiedTask, error)
}

type PeerTransport interface {
	Send(context.Context, string, Envelope) (Envelope, error)
}

type ServiceDeps struct {
	AgentID           string
	Registry          Registry
	Router            *Router
	Classifier        TaskClassifier
	Clarifier         ClarifierRuntime
	Planner           PlannerRuntime
	Briefing          BriefSynthesizer
	Evaluator         *Evaluator
	Transport         PeerTransport
	ProposalExecutor  Executor
	ExecutionExecutor Executor
	Timeouts          TimeoutPolicy
}

type Service struct {
	agentID           string
	registry          Registry
	router            *Router
	classifier        TaskClassifier
	clarifier         ClarifierRuntime
	planner           PlannerRuntime
	briefing          BriefSynthesizer
	evaluator         *Evaluator
	transport         PeerTransport
	proposalExecutor  Executor
	executionExecutor Executor
	timeouts          TimeoutPolicy
	logger            *slog.Logger
}

func NewService(deps ServiceDeps) *Service {
	return &Service{
		agentID:    deps.AgentID,
		registry:   deps.Registry,
		router:     deps.Router,
		classifier: deps.Classifier,
		clarifier:  deps.Clarifier,
		planner:    deps.Planner,
		briefing:   deps.Briefing,
		evaluator:  deps.Evaluator,
		transport:  deps.Transport,
		proposalExecutor:  deps.ProposalExecutor,
		executionExecutor: deps.ExecutionExecutor,
		timeouts:   deps.Timeouts,
		logger:     slog.Default(),
	}
}

func (s *Service) HandleOwnerTask(ctx context.Context, sessionID, prompt string, policy OrchestrationPolicy) (CandidateReply, error) {
	if policy.Profile == "" {
		policy = DefaultPolicy()
	}
	trace := []TraceEvent{
		{Section: "Ingress", Summary: "owner received request", Payload: prompt},
		{Section: "Policy", Summary: "orchestration policy", Payload: FormatPolicy(policy)},
	}
	taskPrompt := prompt
	taskClass := "analysis"
	taskShape := string(TaskShapeSingle)
	if s.clarifier != nil && policy.ClarificationMode != "off" {
		clarified, err := s.clarifier.Clarify(ctx, ClarificationInput{
			Mode:                   policy.ClarificationMode,
			Prompt:                 prompt,
			CriticalMissingInfo:    true,
			MaxClarificationRounds: policy.MaxClarificationRounds,
			CurrentClarificationRound: 1,
		})
		if err != nil {
			return CandidateReply{}, err
		}
		trace = append(trace, TraceEvent{
			Section: "Clarification",
			Summary: "clarified task",
			Payload: fmt.Sprintf("goal=%s\ntask_class=%s\ntask_shape=%s\nmissing_info=%v\nassumptions=%v", clarified.Goal, clarified.TaskClass, clarified.TaskShape, clarified.MissingInfo, clarified.Assumptions),
		})
		if clarified.RequiresFollowUp {
			return CandidateReply{
				AgentID: s.agentID,
				Stage:   "final",
				Text:    clarified.FollowUpQuestion,
				Trace: append(trace, TraceEvent{
					Section: "Clarification",
					Summary: "follow-up requested",
					Payload: clarified.FollowUpQuestion,
				}),
			}, nil
		}
		if clarified.Goal != "" {
			taskPrompt = clarified.Goal
		}
		if clarified.TaskClass != "" {
			taskClass = clarified.TaskClass
		}
		if clarified.TaskShape != "" {
			taskShape = clarified.TaskShape
		}
	}
	if s.classifier != nil && taskClass == "analysis" {
		classifiedClass, _, err := s.classifier.Classify(ctx, taskPrompt)
		if err != nil {
			return CandidateReply{}, err
		}
		taskClass = classifiedClass
		trace = append(trace, TraceEvent{
			Section: "Classification",
			Summary: "fallback classifier result",
			Payload: taskClass,
		})
	}
	if taskShape == string(TaskShapeComposite) && s.planner != nil && policy.CompositePlanning != "off" {
		return s.handleCompositeTask(ctx, sessionID, taskPrompt, taskClass, trace)
	}

	traceID := fmt.Sprintf("%s:%d", sessionID, time.Now().UnixNano())
	baseEnv := Envelope{
		Version:    "v1",
		MessageID:  traceID + ":self",
		TraceID:    traceID,
		SessionID:  sessionID,
		OwnerAgent: s.agentID,
		FromAgent:  s.agentID,
		TaskClass:  taskClass,
		TaskShape:  taskShape,
		Kind:       "proposal",
		TTL:        2,
		Prompt:     taskPrompt,
	}

	candidates := make([]CandidateReply, 0, 2)
	self, err := s.proposalExecutor.Execute(ctx, baseEnv)
	if err != nil {
		return CandidateReply{}, err
	}
	if self.AgentID == "" {
		self.AgentID = s.agentID
	}
	self = ensureProposalCandidate(self)
	candidates = append(candidates, self)
	trace = append(trace, TraceEvent{
		Section: "Proposal",
		Summary: "owner proposal",
		Payload: self.Text,
	})

	peers, err := s.router.SelectPeers(ctx, s.agentID, taskClass)
	if err != nil {
		return CandidateReply{}, err
	}
	for _, peer := range peers {
		if s.transport == nil {
			break
		}
		peerCtx := ctx
		if s.timeouts.PeerTimeout > 0 {
			var cancel context.CancelFunc
			peerCtx, cancel = context.WithTimeout(ctx, s.timeouts.PeerTimeout)
			defer cancel()
		}
		replyEnv, err := s.transport.Send(peerCtx, peer.Addr, Envelope{
			Version:    "v1",
			MessageID:  traceID + ":" + peer.AgentID,
			TraceID:    traceID,
			SessionID:  sessionID,
			OwnerAgent: s.agentID,
			FromAgent:  s.agentID,
			ToAgent:    peer.AgentID,
			TaskClass:  taskClass,
			TaskShape:  taskShape,
			Kind:       "proposal",
			TTL:        2,
			Prompt:     taskPrompt,
		})
		if err != nil {
			s.logger.Debug("mesh peer request failed", "peer", peer.AgentID, "err", err)
			continue
		}
		reply, err := DecodeCandidateReply(replyEnv)
		if err != nil {
			s.logger.Debug("mesh peer reply decode failed", "peer", peer.AgentID, "err", err)
			continue
		}
		reply = ensureProposalCandidate(reply)
		candidates = append(candidates, reply)
		trace = append(trace, TraceEvent{
			Section: "Proposal",
			Summary: "peer proposal from " + peer.AgentID,
			Payload: reply.Text,
		})
	}

	winner, updates, err := s.evaluator.Evaluate(ctx, taskClass, candidates)
	if err != nil {
		return CandidateReply{}, err
	}
	for _, update := range updates {
		if err := s.registry.RecordScore(ctx, update); err != nil {
			return CandidateReply{}, err
		}
	}
	brief, err := s.briefing.Synthesize(candidates)
	if err != nil {
		return CandidateReply{}, err
	}
	trace = append(trace,
		TraceEvent{Section: "Winner", Summary: "selected winner", Payload: fmt.Sprintf("agent=%s\nscore=%d", winner.AgentID, winner.DeterministicScore)},
		TraceEvent{Section: "ExecutionBrief", Summary: "brief for executor", Payload: fmt.Sprintf("goal=%s\nsteps=%v\nconstraints=%v", brief.Goal, brief.RequiredSteps, brief.Constraints)},
	)

	executionEnv := Envelope{
		Version:    "v1",
		MessageID:  traceID + ":execute",
		TraceID:    traceID,
		SessionID:  sessionID,
		OwnerAgent: s.agentID,
		FromAgent:  s.agentID,
		ToAgent:    winner.AgentID,
		TaskClass:  taskClass,
		TaskShape:  taskShape,
		Kind:       "execute",
		TTL:        2,
		Prompt:     taskPrompt,
		ExecutionBrief: brief,
	}
	if winner.AgentID == "" || winner.AgentID == s.agentID {
		final, err := s.executionExecutor.Execute(ctx, executionEnv)
		if err != nil {
			return CandidateReply{}, err
		}
		final.Trace = append(trace, TraceEvent{Section: "Execution", Summary: "owner executed winner flow", Payload: final.Text})
		return final, nil
	}
	replyEnv, err := s.transport.Send(ctx, peerAddr(peers, winner.AgentID), executionEnv)
	if err != nil {
		return CandidateReply{}, err
	}
	final, err := DecodeCandidateReply(replyEnv)
	if err != nil {
		return CandidateReply{}, err
	}
	final.Trace = append(trace, TraceEvent{Section: "Execution", Summary: "peer executed winner flow", Payload: fmt.Sprintf("executor=%s\nresponse=%s", winner.AgentID, final.Text)})
	return final, nil
}

func (s *Service) handleCompositeTask(ctx context.Context, sessionID, prompt, fallbackClass string, trace []TraceEvent) (CandidateReply, error) {
	plan, err := s.planner.Plan(ctx, prompt)
	if err != nil {
		return CandidateReply{}, err
	}
	trace = append(trace, TraceEvent{
		Section: "Planner",
		Summary: "composite task plan",
		Payload: fmt.Sprintf("task_shape=%s\nsteps=%v", plan.TaskShape, plan.Steps),
	})
	outputs := make([]string, 0, len(plan.Steps))
	for _, step := range plan.Steps {
		stepClass := step.TaskClass
		if stepClass == "" {
			stepClass = fallbackClass
		}
		env := Envelope{
			Version:    "v1",
			MessageID:  fmt.Sprintf("%s:%s", sessionID, step.StepID),
			TraceID:    fmt.Sprintf("%s:%s", sessionID, step.StepID),
			SessionID:  sessionID,
			OwnerAgent: s.agentID,
			FromAgent:  s.agentID,
			TaskClass:  stepClass,
			TaskShape:  string(TaskShapeComposite),
			ParentStepID: step.StepID,
			Kind:       "execute",
			TTL:        1,
			Prompt:     step.Description,
			ExecutionBrief: ExecutionBrief{
				Goal:          step.Title,
				RequiredSteps: []string{step.Description},
			},
		}
		reply, err := s.executeCompositeStep(ctx, env)
		if err != nil {
			return CandidateReply{}, err
		}
		trace = append(trace, TraceEvent{
			Section: "CompositeStep",
			Summary: step.StepID + " " + step.Title,
			Payload: reply.Text,
		})
		outputs = append(outputs, fmt.Sprintf("%s\n%s", step.Title, reply.Text))
	}
	return CandidateReply{
		AgentID: s.agentID,
		Stage:   "final",
		Text:    strings.Join(outputs, "\n\n"),
		Trace:   append(trace, TraceEvent{Section: "Integration", Summary: "owner integrated composite outputs", Payload: strings.Join(outputs, "\n\n")}),
	}, nil
}

func (s *Service) executeCompositeStep(ctx context.Context, env Envelope) (CandidateReply, error) {
	peers, err := s.router.SelectPeers(ctx, s.agentID, env.TaskClass)
	if err != nil {
		return CandidateReply{}, err
	}
	if len(peers) == 0 || env.TaskClass == "writing" {
		return s.executionExecutor.Execute(ctx, env)
	}
	replyEnv, err := s.transport.Send(ctx, peers[0].Addr, Envelope{
		Version:       env.Version,
		MessageID:     env.MessageID,
		TraceID:       env.TraceID,
		SessionID:     env.SessionID,
		OwnerAgent:    env.OwnerAgent,
		FromAgent:     env.FromAgent,
		ToAgent:       peers[0].AgentID,
		TaskClass:     env.TaskClass,
		TaskShape:     env.TaskShape,
		ParentStepID:  env.ParentStepID,
		Kind:          env.Kind,
		TTL:           env.TTL,
		Prompt:        env.Prompt,
		ExecutionBrief: env.ExecutionBrief,
	})
	if err != nil {
		return CandidateReply{}, err
	}
	return DecodeCandidateReply(replyEnv)
}

func (s *Service) HandleEnvelope(ctx context.Context, env Envelope) (Envelope, error) {
	executor := s.executionExecutor
	if env.Kind == "proposal" {
		executor = s.proposalExecutor
	}
	reply, err := executor.Execute(ctx, env)
	if err != nil {
		return Envelope{}, err
	}
	return EncodeCandidateReply(Envelope{
		Version:    env.Version,
		MessageID:  env.MessageID + ":reply",
		TraceID:    env.TraceID,
		SessionID:  env.SessionID,
		OwnerAgent: env.OwnerAgent,
		FromAgent:  s.agentID,
		ToAgent:    env.FromAgent,
		TaskClass:  env.TaskClass,
		Kind:       "reply",
		TTL:        env.TTL,
	}, reply), nil
}

func EncodeCandidateReply(env Envelope, reply CandidateReply) Envelope {
	if env.Metadata == nil {
		env.Metadata = map[string]any{}
	}
	env.Metadata["candidate_reply"] = map[string]any{
		"agent_id":             reply.AgentID,
		"stage":                reply.Stage,
		"text":                 reply.Text,
		"latency_ms":           reply.Latency.Milliseconds(),
		"tokens_used":          reply.TokensUsed,
		"deterministic_score":  reply.DeterministicScore,
		"judge_score":          reply.JudgeScore,
		"passed_checks":        reply.PassedChecks,
		"err":                  reply.Err,
	}
	return env
}

func DecodeCandidateReply(env Envelope) (CandidateReply, error) {
	raw, ok := env.Metadata["candidate_reply"]
	if !ok {
		return CandidateReply{}, fmt.Errorf("mesh reply metadata missing candidate_reply")
	}
	body, err := json.Marshal(raw)
	if err != nil {
		return CandidateReply{}, err
	}
	var payload struct {
		AgentID            string `json:"agent_id"`
		Stage              string `json:"stage"`
		Text               string `json:"text"`
		LatencyMS          int64  `json:"latency_ms"`
		TokensUsed         int    `json:"tokens_used"`
		DeterministicScore int    `json:"deterministic_score"`
		JudgeScore         int    `json:"judge_score"`
		PassedChecks       bool   `json:"passed_checks"`
		Err                string `json:"err"`
	}
	if err := json.Unmarshal(body, &payload); err != nil {
		return CandidateReply{}, err
	}
	return CandidateReply{
		AgentID:            payload.AgentID,
		Stage:              payload.Stage,
		Text:               payload.Text,
		Latency:            time.Duration(payload.LatencyMS) * time.Millisecond,
		TokensUsed:         payload.TokensUsed,
		DeterministicScore: payload.DeterministicScore,
		JudgeScore:         payload.JudgeScore,
		PassedChecks:       payload.PassedChecks,
		Err:                payload.Err,
	}, nil
}

func peerAddr(peers []PeerDescriptor, agentID string) string {
	for _, peer := range peers {
		if peer.AgentID == agentID {
			return peer.Addr
		}
	}
	return ""
}

func ensureProposalCandidate(reply CandidateReply) CandidateReply {
	if strings.TrimSpace(reply.Proposal.Understanding) == "" {
		reply.Proposal.Understanding = strings.TrimSpace(reply.Text)
	}
	if len(reply.Proposal.PlannedChecks) == 0 && strings.TrimSpace(reply.Text) != "" {
		reply.Proposal.PlannedChecks = []string{"review proposal output"}
	}
	if reply.ProposalMetadata.Confidence == 0 {
		reply.ProposalMetadata.Confidence = 0.5
	}
	return reply
}
