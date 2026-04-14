package mesh

import (
	"context"
	"testing"
	"strings"
)

type stubTaskClassifier struct {
	taskClass  string
	confidence float64
	err        error
}

func (s stubTaskClassifier) Classify(_ context.Context, _ string) (string, float64, error) {
	return s.taskClass, s.confidence, s.err
}

type stubExecutor struct {
	reply CandidateReply
	err   error
	calls []Envelope
}

func (s *stubExecutor) Execute(_ context.Context, env Envelope) (CandidateReply, error) {
	s.calls = append(s.calls, env)
	return s.reply, s.err
}

type stubPeerTransport struct {
	replies []Envelope
	err     error
	calls   []Envelope
}

func (s *stubPeerTransport) Send(_ context.Context, _ string, env Envelope) (Envelope, error) {
	s.calls = append(s.calls, env)
	if s.err != nil {
		return Envelope{}, s.err
	}
	if len(s.replies) == 0 {
		return Envelope{}, nil
	}
	reply := s.replies[0]
	s.replies = s.replies[1:]
	return reply, nil
}

type stubClarifier struct {
	task ClarifiedTask
	err  error
	inputs []ClarificationInput
}

func (s *stubClarifier) Clarify(_ context.Context, input ClarificationInput) (ClarifiedTask, error) {
	s.inputs = append(s.inputs, input)
	return s.task, s.err
}

type stubPlanner struct {
	plan  TaskPlan
	err   error
	calls []string
}

func (s *stubPlanner) Plan(_ context.Context, prompt string) (TaskPlan, error) {
	s.calls = append(s.calls, prompt)
	return s.plan, s.err
}

func TestServiceHandleOwnerTaskSelectsWinnerAndRecordsScores(t *testing.T) {
	ctx := context.Background()
	reg := newMemoryRegistry()
	for _, peer := range []PeerDescriptor{
		{AgentID: "owner", Addr: "http://owner.local", Status: "idle"},
		{AgentID: "peer-a", Addr: "http://peer-a.local", Status: "idle"},
	} {
		if err := reg.Register(ctx, peer); err != nil {
			t.Fatalf("register %s: %v", peer.AgentID, err)
		}
	}

	proposalExec := &stubExecutor{
		reply: CandidateReply{AgentID: "owner", Stage: "final", Text: "owner proposal", PassedChecks: true, DeterministicScore: 1},
	}
	executionExec := &stubExecutor{
		reply: CandidateReply{AgentID: "owner", Stage: "final", Text: "owner final", PassedChecks: true, DeterministicScore: 1},
	}
	peerTransport := &stubPeerTransport{
		replies: []Envelope{
			EncodeCandidateReply(Envelope{Version: "v1", MessageID: "reply-1", TraceID: "trace-1"}, CandidateReply{AgentID: "peer-a", Stage: "final", Text: "peer proposal", PassedChecks: true, DeterministicScore: 5}),
			EncodeCandidateReply(Envelope{Version: "v1", MessageID: "reply-2", TraceID: "trace-1"}, CandidateReply{AgentID: "peer-a", Stage: "final", Text: "peer execution", PassedChecks: true, DeterministicScore: 1}),
		},
	}
	svc := NewService(ServiceDeps{
		AgentID:     "owner",
		Registry:    reg,
		Router:      NewRouter(reg, RouterConfig{AgentID: "owner", ColdStartFanout: 1}),
		Classifier:  stubTaskClassifier{taskClass: "coding", confidence: 0.9},
		Clarifier:   &stubClarifier{task: ClarifiedTask{Goal: "write a script", TaskClass: "coding", TaskShape: "single"}},
		Briefing:    NewBriefSynthesizer(),
		Evaluator:   NewEvaluator(nil),
		Transport:   peerTransport,
		ProposalExecutor:  proposalExec,
		ExecutionExecutor: executionExec,
		Timeouts:    TimeoutPolicy{},
	})

	policy, _ := PolicyForProfile("balanced")
	winner, err := svc.HandleOwnerTask(ctx, "telegram:1001/default", "write a script", policy)
	if err != nil {
		t.Fatalf("handle owner task: %v", err)
	}
	if winner.Text != "peer execution" {
		t.Fatalf("expected peer winner, got %#v", winner)
	}
	if len(peerTransport.calls) != 2 {
		t.Fatalf("expected proposal+execution transport calls, got %#v", peerTransport.calls)
	}
	if peerTransport.calls[0].Kind != "proposal" || peerTransport.calls[1].Kind != "execute" {
		t.Fatalf("unexpected peer call kinds: %#v", peerTransport.calls)
	}
	if peerTransport.calls[1].ExecutionBrief.Goal == "" || len(peerTransport.calls[1].ExecutionBrief.RequiredSteps) == 0 {
		t.Fatalf("expected execution brief on winner execution, got %#v", peerTransport.calls[1])
	}
	if len(executionExec.calls) != 0 {
		t.Fatalf("owner execution should not run when peer wins: %#v", executionExec.calls)
	}

	scores, err := reg.ListScores(ctx, "coding")
	if err != nil {
		t.Fatalf("list scores: %v", err)
	}
	if len(scores) != 2 {
		t.Fatalf("expected recorded scores for both candidates, got %#v", scores)
	}
	if len(winner.Trace) == 0 {
		t.Fatalf("expected mesh trace events, got %#v", winner)
	}
}

func TestServiceHandleOwnerTaskReturnsFollowUpWhenClarifierNeedsMoreInfo(t *testing.T) {
	ctx := context.Background()
	reg := newMemoryRegistry()
	if err := reg.Register(ctx, PeerDescriptor{AgentID: "owner", Status: "idle"}); err != nil {
		t.Fatalf("register owner: %v", err)
	}

	proposalExec := &stubExecutor{}
	executionExec := &stubExecutor{}
	clarifier := &stubClarifier{
		task: ClarifiedTask{
			Goal:             "Написать скрипт",
			MissingInfo:      []string{"Какой язык нужен?"},
			RequiresFollowUp: true,
			FollowUpQuestion: "Какой язык нужен?",
			TaskClass:        "coding",
			TaskShape:        "single",
		},
	}
	svc := NewService(ServiceDeps{
		AgentID:           "owner",
		Registry:          reg,
		Router:            NewRouter(reg, RouterConfig{AgentID: "owner", ColdStartFanout: 1}),
		Classifier:        stubTaskClassifier{taskClass: "coding", confidence: 0.9},
		Clarifier:         clarifier,
		Briefing:          NewBriefSynthesizer(),
		Evaluator:         NewEvaluator(nil),
		ProposalExecutor:  proposalExec,
		ExecutionExecutor: executionExec,
	})

	policy, _ := PolicyForProfile("balanced")
	reply, err := svc.HandleOwnerTask(ctx, "telegram:1001/default", "напиши скрипт", policy)
	if err != nil {
		t.Fatalf("handle owner task: %v", err)
	}
	if reply.Text != "Какой язык нужен?" || reply.Stage != "final" {
		t.Fatalf("unexpected follow-up reply: %#v", reply)
	}
	if len(proposalExec.calls) != 0 || len(executionExec.calls) != 0 {
		t.Fatalf("expected no proposal or execution calls on follow-up, got proposals=%#v execution=%#v", proposalExec.calls, executionExec.calls)
	}
}

func TestServiceHandleOwnerTaskRoutesCompositeStepsAndIntegratesOutput(t *testing.T) {
	ctx := context.Background()
	reg := newMemoryRegistry()
	for _, peer := range []PeerDescriptor{
		{AgentID: "owner", Addr: "http://owner.local", Status: "idle"},
		{AgentID: "peer-a", Addr: "http://peer-a.local", Status: "idle"},
	} {
		if err := reg.Register(ctx, peer); err != nil {
			t.Fatalf("register %s: %v", peer.AgentID, err)
		}
	}

	clarifier := &stubClarifier{
		task: ClarifiedTask{
			Goal:      "Напиши скрипт и задокументируй его",
			TaskClass: "coding",
			TaskShape: "composite",
		},
	}
	planner := &stubPlanner{
		plan: TaskPlan{
			TaskShape: "composite",
			Steps: []PlannedStep{
				{StepID: "step-1", Title: "Написать скрипт", TaskClass: "coding", Description: "Подготовить скрипт", RequiresTools: true},
				{StepID: "step-2", Title: "Задокументировать", TaskClass: "writing", Description: "Подготовить документацию", RequiresTools: false},
			},
		},
	}
	executionExec := &stubExecutor{
		reply: CandidateReply{AgentID: "owner", Stage: "final", Text: "owner local result"},
	}
	peerTransport := &stubPeerTransport{
		replies: []Envelope{
			EncodeCandidateReply(Envelope{Version: "v1", MessageID: "reply-step-1", TraceID: "trace-1"}, CandidateReply{AgentID: "peer-a", Stage: "final", Text: "peer script result"}),
		},
	}

	svc := NewService(ServiceDeps{
		AgentID:           "owner",
		Registry:          reg,
		Router:            NewRouter(reg, RouterConfig{AgentID: "owner", ColdStartFanout: 1}),
		Classifier:        stubTaskClassifier{taskClass: "coding", confidence: 0.9},
		Clarifier:         clarifier,
		Planner:           planner,
		Briefing:          NewBriefSynthesizer(),
		Evaluator:         NewEvaluator(nil),
		Transport:         peerTransport,
		ProposalExecutor:  &stubExecutor{},
		ExecutionExecutor: executionExec,
	})

	policy, _ := PolicyForProfile("composite")
	reply, err := svc.HandleOwnerTask(ctx, "telegram:1001/default", "Напиши скрипт и задокументируй его", policy)
	if err != nil {
		t.Fatalf("handle owner task: %v", err)
	}
	if !strings.Contains(reply.Text, "Написать скрипт") || !strings.Contains(reply.Text, "peer script result") || !strings.Contains(reply.Text, "owner local result") {
		t.Fatalf("unexpected integrated reply: %#v", reply)
	}
	if len(peerTransport.calls) != 1 || peerTransport.calls[0].ParentStepID != "step-1" {
		t.Fatalf("expected routed remote step, got %#v", peerTransport.calls)
	}
	if len(executionExec.calls) != 1 || executionExec.calls[0].ParentStepID != "step-2" {
		t.Fatalf("expected one local execution step, got %#v", executionExec.calls)
	}
}
