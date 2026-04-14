package cli

import (
	"context"
	"net/http"
	"net/http/httptest"
	"testing"

	"teamd/internal/api"
	"teamd/internal/runtime"
)

func TestClientReadsRuntimeAndApprovals(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/api/runtime":
			_, _ = w.Write([]byte(`{"memory_policy":{"profile":"conservative"},"action_policy":{"ApprovalRequiredTools":["shell.exec"]}}`))
		case "/api/runtime/sessions/1001:default":
			switch r.Method {
			case http.MethodGet:
				_, _ = w.Write([]byte(`{"session_id":"1001:default","runtime":{"model":"glm-5.1"},"memory_policy":{"profile":"standard"},"action_policy":{"approval_required_tools":["shell.exec"]},"has_overrides":true}`))
			case http.MethodPatch:
				_, _ = w.Write([]byte(`{"session_id":"1001:default","runtime":{"model":"glm-5.1"},"memory_policy":{"profile":"standard"},"action_policy":{"approval_required_tools":["shell.exec"]},"has_overrides":true}`))
			case http.MethodDelete:
				_, _ = w.Write([]byte(`{"session_id":"1001:default","runtime":{"model":"glm-5-turbo"},"memory_policy":{"profile":"conservative"},"action_policy":{"approval_required_tools":["shell.exec"]},"has_overrides":false}`))
			default:
				t.Fatalf("unexpected method for session runtime: %s", r.Method)
			}
		case "/api/approvals":
			_, _ = w.Write([]byte(`[{"id":"approval-1","worker_id":"shell.exec","session_id":"1001:default","payload":"{}","status":"pending"}]`))
		case "/api/memory/search":
			_, _ = w.Write([]byte(`{"items":[{"DocKey":"continuity:1","Kind":"continuity","Title":"Test","Body":"remembered","Score":0.9}]}`))
		case "/api/memory/continuity:1":
			_, _ = w.Write([]byte(`{"document":{"DocKey":"continuity:1","Kind":"continuity","Title":"Test","Body":"remembered"}}`))
		case "/api/artifacts/artifact://tool-output-1":
			_, _ = w.Write([]byte(`{"artifact":{"ref":"artifact://tool-output-1","name":"tool-output-1","size_bytes":18}}`))
		case "/api/artifacts/artifact://tool-output-1/content":
			_, _ = w.Write([]byte(`full artifact body`))
		case "/api/artifacts/search":
			if r.URL.Query().Get("owner_type") == "run" && r.URL.Query().Get("owner_id") == "run-1" {
				_, _ = w.Write([]byte(`{"items":[{"ref":"artifact://tool-output-1","name":"tool-output-1","owner_type":"run","owner_id":"run-1","size_bytes":18,"preview":"alpha\nbeta"}]}`))
			} else if r.URL.Query().Get("global") == "true" {
				_, _ = w.Write([]byte(`{"items":[{"ref":"artifact://tool-output-1","name":"tool-output-1","owner_type":"run","owner_id":"run-1","size_bytes":18,"preview":"alpha\nbeta"},{"ref":"artifact://tool-output-2","name":"tool-output-2","owner_type":"run","owner_id":"run-2","size_bytes":19,"preview":"other"}]}`))
			} else {
				t.Fatalf("unexpected search query: %s", r.URL.RawQuery)
			}
		case "/api/events":
			_, _ = w.Write([]byte(`{"items":[{"ID":1,"EntityType":"run","EntityID":"run-1","SessionID":"1001:default","Kind":"run.started","Payload":{},"CreatedAt":"2026-04-11T00:00:00Z"}]}`))
		case "/api/control/1001:default":
			_, _ = w.Write([]byte(`{"control":{"session":{"SessionID":"1001:default","ChatID":1001,"RuntimeSummary":{"SessionID":"1001:default","Runtime":{"model":"glm-5-turbo"},"memory_policy":{"profile":"conservative"},"action_policy":{"approval_required_tools":["shell.exec"]}},"LatestRun":{"RunID":"run-1","ChatID":1001,"SessionID":"1001:default","Status":"running"}},"approvals":[{"id":"approval-1","worker_id":"shell.exec","session_id":"1001:default","status":"pending","target_type":"run","target_id":"worker-run-1"}],"workers":[{"WorkerID":"worker-1","Status":"waiting_approval","LastRunID":"worker-run-1"}],"jobs":[]}}`))
		case "/api/control/1001:default/actions":
			_, _ = w.Write([]byte(`{"result":{"action":"run.cancel","message":"Отмена запрошена","pages":["ok"],"control":{"session":{"SessionID":"1001:default"}}}}`))
		case "/api/session-actions":
			_, _ = w.Write([]byte(`{"result":{"action":"session.stats","active_session":"default","sessions":["default","deploy"],"message_count":2}}`))
		case "/api/events/stream":
			w.Header().Set("Content-Type", "text/event-stream")
			_, _ = w.Write([]byte("event: runtime\n"))
			_, _ = w.Write([]byte("data: {\"ID\":1,\"EntityType\":\"run\",\"EntityID\":\"run-1\",\"SessionID\":\"1001:default\",\"Kind\":\"run.started\",\"Payload\":{},\"CreatedAt\":\"2026-04-11T00:00:00Z\"}\n\n"))
		case "/api/plans":
			switch r.Method {
			case http.MethodGet:
				_, _ = w.Write([]byte(`{"items":[{"PlanID":"plan-1","OwnerType":"run","OwnerID":"run-1","Title":"Investigate rollout","Items":[{"ItemID":"item-1","Content":"Inspect runtime","Status":"pending","Position":1}],"Notes":["note"]}]}`))
			case http.MethodPost:
				_, _ = w.Write([]byte(`{"plan":{"PlanID":"plan-1","OwnerType":"run","OwnerID":"run-1","Title":"Investigate rollout"}}`))
			default:
				t.Fatalf("unexpected plans method: %s", r.Method)
			}
		case "/api/plans/plan-1":
			_, _ = w.Write([]byte(`{"plan":{"PlanID":"plan-1","OwnerType":"run","OwnerID":"run-1","Title":"Investigate rollout","Items":[{"ItemID":"item-1","Content":"Inspect runtime","Status":"pending","Position":1}],"Notes":["note"]}}`))
		case "/api/plans/plan-1/items":
			_, _ = w.Write([]byte(`{"plan":{"PlanID":"plan-1","OwnerType":"run","OwnerID":"run-1","Title":"Investigate rollout","Items":[{"ItemID":"item-1","Content":"Inspect runtime","Status":"pending","Position":1}]}}`))
		case "/api/plans/plan-1/notes":
			_, _ = w.Write([]byte(`{"plan":{"PlanID":"plan-1","OwnerType":"run","OwnerID":"run-1","Title":"Investigate rollout","Notes":["note"]}}`))
		case "/api/plans/plan-1/items/item-1/start":
			_, _ = w.Write([]byte(`{"plan":{"PlanID":"plan-1","OwnerType":"run","OwnerID":"run-1","Title":"Investigate rollout","Items":[{"ItemID":"item-1","Content":"Inspect runtime","Status":"in_progress","Position":1}]}}`))
		case "/api/plans/plan-1/items/item-1/complete":
			_, _ = w.Write([]byte(`{"plan":{"PlanID":"plan-1","OwnerType":"run","OwnerID":"run-1","Title":"Investigate rollout","Items":[{"ItemID":"item-1","Content":"Inspect runtime","Status":"completed","Position":1}]}}`))
		case "/api/approvals/approval-1/approve":
			_, _ = w.Write([]byte(`{"id":"approval-1","worker_id":"shell.exec","session_id":"1001:default","payload":"{}","status":"approved"}`))
		case "/api/runs":
			_, _ = w.Write([]byte(`{"run_id":"run-1","accepted":true,"run":{"RunID":"run-1","ChatID":1001,"SessionID":"1001:default","Query":"hello","Status":"running","Active":true}}`))
		case "/api/runs/run-1":
			_, _ = w.Write([]byte(`{"run":{"RunID":"run-1","ChatID":1001,"SessionID":"1001:default","Query":"hello","Status":"running","Active":true}}`))
		case "/api/runs/run-1/cancel":
			_, _ = w.Write([]byte(`{"ok":true,"run_id":"run-1"}`))
		case "/api/jobs":
			switch r.Method {
			case http.MethodGet:
				_, _ = w.Write([]byte(`{"items":[{"JobID":"job-1","Status":"completed"}]}`))
			case http.MethodPost:
				_, _ = w.Write([]byte(`{"job":{"JobID":"job-1","Status":"queued"}}`))
			default:
				t.Fatalf("unexpected jobs method: %s", r.Method)
			}
		case "/api/jobs/job-1":
			_, _ = w.Write([]byte(`{"job":{"JobID":"job-1","Status":"completed"}}`))
		case "/api/jobs/job-1/logs":
			_, _ = w.Write([]byte(`{"items":[{"ID":1,"JobID":"job-1","Stream":"stdout","Content":"hello"}]}`))
		case "/api/jobs/job-1/cancel":
			_, _ = w.Write([]byte(`{"ok":true,"job_id":"job-1"}`))
		case "/api/workers":
			switch r.Method {
			case http.MethodGet:
				_, _ = w.Write([]byte(`{"items":[{"WorkerID":"worker-1","Status":"idle"}]}`))
			case http.MethodPost:
				_, _ = w.Write([]byte(`{"worker":{"WorkerID":"worker-1","Status":"idle"}}`))
			default:
				t.Fatalf("unexpected workers method: %s", r.Method)
			}
		case "/api/workers/worker-1":
			_, _ = w.Write([]byte(`{"worker":{"WorkerID":"worker-1","Status":"idle"}}`))
		case "/api/workers/worker-1/messages":
			_, _ = w.Write([]byte(`{"worker":{"WorkerID":"worker-1","Status":"running"}}`))
		case "/api/workers/worker-1/wait":
			_, _ = w.Write([]byte(`{"worker":{"WorkerID":"worker-1","Status":"idle"},"handoff":{"WorkerID":"worker-1","Summary":"done","Artifacts":["artifact://worker-output-1"]},"messages":[{"cursor":1,"role":"assistant","content":"done"}],"events":[{"ID":1,"EntityType":"worker","EntityID":"worker-1","Kind":"worker.spawned","Payload":{},"CreatedAt":"2026-04-11T00:00:00Z"}],"next_cursor":1,"next_event_after":1}`))
		case "/api/workers/worker-1/handoff":
			_, _ = w.Write([]byte(`{"handoff":{"WorkerID":"worker-1","Summary":"done","Artifacts":["artifact://worker-output-1"]}}`))
		case "/api/workers/worker-1/close":
			_, _ = w.Write([]byte(`{"worker":{"WorkerID":"worker-1","Status":"closed"}}`))
		default:
			t.Fatalf("unexpected path: %s", r.URL.Path)
		}
	}))
	defer server.Close()

	client := NewClient(server.URL, server.Client())
	runtimeSummary, err := client.Runtime()
	if err != nil {
		t.Fatalf("runtime: %v", err)
	}
	if runtimeSummary.MemoryPolicy.Profile != "conservative" {
		t.Fatalf("unexpected runtime summary: %+v", runtimeSummary)
	}

	sessionSummary, err := client.RuntimeForSession("1001:default")
	if err != nil {
		t.Fatalf("runtime for session: %v", err)
	}
	if sessionSummary.SessionID != "1001:default" || sessionSummary.Runtime.Model != "glm-5.1" {
		t.Fatalf("unexpected session runtime summary: %+v", sessionSummary)
	}

	approvals, err := client.Approvals("1001:default")
	if err != nil {
		t.Fatalf("approvals: %v", err)
	}
	if len(approvals) != 1 || approvals[0].ID != "approval-1" {
		t.Fatalf("unexpected approvals: %+v", approvals)
	}

	approved, err := client.Approve("approval-1")
	if err != nil {
		t.Fatalf("approve: %v", err)
	}
	if approved.Status != "approved" {
		t.Fatalf("unexpected approval response: %+v", approved)
	}

	run, err := client.StartRun(1001, "1001:default", "hello")
	if err != nil {
		t.Fatalf("start run: %v", err)
	}
	if run.RunID != "run-1" || !run.Accepted {
		t.Fatalf("unexpected start run response: %+v", run)
	}

	status, err := client.RunStatus("run-1")
	if err != nil {
		t.Fatalf("run status: %v", err)
	}
	if status.Run.RunID != "run-1" {
		t.Fatalf("unexpected run status: %+v", status)
	}
	if err := client.CancelRun("run-1"); err != nil {
		t.Fatalf("cancel run: %v", err)
	}
	if _, err := client.StartJob(api.CreateJobRequest{ChatID: 1001, SessionID: "1001:default", Command: "echo"}); err != nil {
		t.Fatalf("start job: %v", err)
	}
	if _, err := client.Jobs(10); err != nil {
		t.Fatalf("list jobs: %v", err)
	}
	if _, err := client.Job("job-1"); err != nil {
		t.Fatalf("show job: %v", err)
	}
	if _, err := client.JobLogs("job-1", "", 0, 10); err != nil {
		t.Fatalf("job logs: %v", err)
	}
	if err := client.CancelJob("job-1"); err != nil {
		t.Fatalf("cancel job: %v", err)
	}
	if _, err := client.StartWorker(api.CreateWorkerRequest{ChatID: 1001, SessionID: "1001:default", Prompt: "hello"}); err != nil {
		t.Fatalf("start worker: %v", err)
	}
	if _, err := client.Workers(1001, 10); err != nil {
		t.Fatalf("workers: %v", err)
	}
	if _, err := client.Worker("worker-1"); err != nil {
		t.Fatalf("worker: %v", err)
	}
	if _, err := client.MessageWorker("worker-1", "do it"); err != nil {
		t.Fatalf("message worker: %v", err)
	}
	if _, err := client.WaitWorker("worker-1", 0, 0); err != nil {
		t.Fatalf("wait worker: %v", err)
	}
	handoff, err := client.WorkerHandoff("worker-1")
	if err != nil {
		t.Fatalf("worker handoff: %v", err)
	}
	if handoff.Handoff.WorkerID != "worker-1" || len(handoff.Handoff.Artifacts) != 1 {
		t.Fatalf("unexpected worker handoff: %+v", handoff.Handoff)
	}
	if _, err := client.CloseWorker("worker-1"); err != nil {
		t.Fatalf("close worker: %v", err)
	}
	if _, err := client.UpdateRuntimeSession("1001:default", api.SessionOverrideRequest{}); err != nil {
		t.Fatalf("update runtime session: %v", err)
	}
	if _, err := client.ClearRuntimeSession("1001:default"); err != nil {
		t.Fatalf("clear runtime session: %v", err)
	}
	search, err := client.MemorySearch(1001, "1001:default", "test", 5)
	if err != nil {
		t.Fatalf("memory search: %v", err)
	}
	if len(search.Items) != 1 || search.Items[0].DocKey != "continuity:1" {
		t.Fatalf("unexpected memory search: %+v", search)
	}
	doc, err := client.MemoryRead("continuity:1")
	if err != nil {
		t.Fatalf("memory read: %v", err)
	}
	if doc.Document.DocKey != "continuity:1" {
		t.Fatalf("unexpected memory document: %+v", doc)
	}
	events, err := client.Events(api.EventListRequest{EntityType: "run", EntityID: "run-1", Limit: 10})
	if err != nil {
		t.Fatalf("events: %v", err)
	}
	if len(events.Items) != 1 || events.Items[0].Kind != "run.started" {
		t.Fatalf("unexpected events: %+v", events)
	}
	streamCount := 0
	err = client.StreamEvents(context.Background(), api.EventListRequest{EntityType: "run", EntityID: "run-1", Limit: 10}, func(item runtime.RuntimeEvent) error {
		streamCount++
		if item.Kind != "run.started" || item.EntityID != "run-1" {
			t.Fatalf("unexpected streamed event: %+v", item)
		}
		return ErrStopStream
	})
	if err != ErrStopStream {
		t.Fatalf("stream events: %v", err)
	}
	if streamCount != 1 {
		t.Fatalf("unexpected stream count: %d", streamCount)
	}
	artifact, err := client.Artifact("artifact://tool-output-1")
	if err != nil {
		t.Fatalf("artifact: %v", err)
	}
	if artifact.Artifact.Ref != "artifact://tool-output-1" {
		t.Fatalf("unexpected artifact metadata: %+v", artifact)
	}
	content, err := client.ArtifactContent("artifact://tool-output-1")
	if err != nil {
		t.Fatalf("artifact content: %v", err)
	}
	if content.Content != "full artifact body" {
		t.Fatalf("unexpected artifact content: %+v", content)
	}
	artifactSearch, err := client.ArtifactSearch(api.ArtifactSearchRequest{
		OwnerType: "run",
		OwnerID:   "run-1",
		Query:     "beta",
		Limit:     5,
	})
	if err != nil {
		t.Fatalf("artifact search: %v", err)
	}
	if len(artifactSearch.Items) != 1 || artifactSearch.Items[0].Ref != "artifact://tool-output-1" || artifactSearch.Items[0].OwnerID != "run-1" {
		t.Fatalf("unexpected artifact search: %+v", artifactSearch.Items)
	}
	globalSearch, err := client.ArtifactSearch(api.ArtifactSearchRequest{
		Global: true,
		Query:  "alpha",
		Limit:  10,
	})
	if err != nil {
		t.Fatalf("global artifact search: %v", err)
	}
	if len(globalSearch.Items) != 2 {
		t.Fatalf("unexpected global artifact search: %+v", globalSearch.Items)
	}
	plans, err := client.Plans("run", "run-1", 10)
	if err != nil {
		t.Fatalf("plans: %v", err)
	}
	if len(plans.Items) != 1 || plans.Items[0].PlanID != "plan-1" {
		t.Fatalf("unexpected plans: %+v", plans)
	}
	control, err := client.ControlState("1001:default", 1001)
	if err != nil {
		t.Fatalf("control state: %v", err)
	}
	if control.Control.Session.SessionID != "1001:default" {
		t.Fatalf("unexpected control state: %+v", control.Control)
	}
	controlAction, err := client.ControlAction("1001:default", api.ControlActionRequest{Action: "run.cancel", ChatID: 1001})
	if err != nil {
		t.Fatalf("control action: %v", err)
	}
	if controlAction.Result.Message != "Отмена запрошена" {
		t.Fatalf("unexpected control action: %+v", controlAction.Result)
	}
	sessionAction, err := client.SessionAction(api.SessionActionRequest{ChatID: 1001, Action: "session.stats"})
	if err != nil {
		t.Fatalf("session action: %v", err)
	}
	if sessionAction.Result.ActiveSession != "default" || sessionAction.Result.MessageCount != 2 {
		t.Fatalf("unexpected session action: %+v", sessionAction.Result)
	}
	if _, err := client.CreatePlan(api.CreatePlanRequest{OwnerType: "run", OwnerID: "run-1", Title: "Investigate rollout"}); err != nil {
		t.Fatalf("create plan: %v", err)
	}
	plan, err := client.Plan("plan-1")
	if err != nil {
		t.Fatalf("plan: %v", err)
	}
	if plan.Plan.PlanID != "plan-1" {
		t.Fatalf("unexpected plan: %+v", plan)
	}
	if _, err := client.ReplacePlanItems("plan-1", api.ReplacePlanItemsRequest{Items: []runtime.PlanItem{{Content: "Inspect runtime"}}}); err != nil {
		t.Fatalf("replace plan items: %v", err)
	}
	if _, err := client.AppendPlanNote("plan-1", "note"); err != nil {
		t.Fatalf("append plan note: %v", err)
	}
	startedPlan, err := client.StartPlanItem("plan-1", "item-1")
	if err != nil {
		t.Fatalf("start plan item: %v", err)
	}
	if len(startedPlan.Plan.Items) != 1 || startedPlan.Plan.Items[0].Status != runtime.PlanItemInProgress {
		t.Fatalf("unexpected started plan: %+v", startedPlan.Plan)
	}
	completedPlan, err := client.CompletePlanItem("plan-1", "item-1")
	if err != nil {
		t.Fatalf("complete plan item: %v", err)
	}
	if len(completedPlan.Plan.Items) != 1 || completedPlan.Plan.Items[0].Status != runtime.PlanItemCompleted {
		t.Fatalf("unexpected completed plan: %+v", completedPlan.Plan)
	}
}

func TestClientSendsBearerTokenWhenConfigured(t *testing.T) {
	var authHeader string
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		authHeader = r.Header.Get("Authorization")
		_, _ = w.Write([]byte(`{"memory_policy":{"profile":"conservative"}}`))
	}))
	defer server.Close()

	client := NewClient(server.URL, server.Client()).WithAuthToken("operator-secret")
	if _, err := client.Runtime(); err != nil {
		t.Fatalf("runtime: %v", err)
	}
	if authHeader != "Bearer operator-secret" {
		t.Fatalf("unexpected auth header: %q", authHeader)
	}
}

func TestClientDecodesStructuredAPIError(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusConflict)
		_, _ = w.Write([]byte(`{"error":{"code":"conflict","message":"run already active","entity_type":"run","entity_id":"run-1","retryable":false},"time":"2026-04-11T00:00:00Z"}`))
	}))
	defer server.Close()

	client := NewClient(server.URL, server.Client())
	_, err := client.StartRun(1001, "1001:default", "hello")
	if err == nil {
		t.Fatal("expected structured api error")
	}
	if got := err.Error(); got != "conflict: run already active [run/run-1]" {
		t.Fatalf("unexpected error: %q", got)
	}
}
