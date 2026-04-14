package cli

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strings"

	"teamd/internal/api"
	"teamd/internal/runtime"
)

var ErrStopStream = errors.New("stop stream")

type Client struct {
	baseURL   string
	http      *http.Client
	authToken string
}

func NewClient(baseURL string, client *http.Client) *Client {
	baseURL = strings.TrimRight(strings.TrimSpace(baseURL), "/")
	if client == nil {
		client = http.DefaultClient
	}
	return &Client{baseURL: baseURL, http: client}
}

func (c *Client) WithAuthToken(token string) *Client {
	clone := *c
	clone.authToken = strings.TrimSpace(token)
	return &clone
}

func (c *Client) applyAuth(req *http.Request) {
	if strings.TrimSpace(c.authToken) == "" {
		return
	}
	req.Header.Set("Authorization", "Bearer "+c.authToken)
}

func (c *Client) do(req *http.Request) (*http.Response, error) {
	c.applyAuth(req)
	return c.http.Do(req)
}

func (c *Client) Runtime() (api.RuntimeSummaryResponse, error) {
	var out api.RuntimeSummaryResponse
	err := c.getJSON("/api/runtime", &out)
	return out, err
}

func (c *Client) RuntimeForSession(sessionID string) (api.RuntimeSummaryResponse, error) {
	var out api.RuntimeSummaryResponse
	err := c.getJSON("/api/runtime/sessions/"+sessionID, &out)
	return out, err
}

func (c *Client) UpdateRuntimeSession(sessionID string, req api.SessionOverrideRequest) (api.RuntimeSummaryResponse, error) {
	body, _ := json.Marshal(req)
	request, err := http.NewRequest(http.MethodPatch, c.baseURL+"/api/runtime/sessions/"+sessionID, bytes.NewReader(body))
	if err != nil {
		return api.RuntimeSummaryResponse{}, err
	}
	request.Header.Set("Content-Type", "application/json")
	c.applyAuth(request)
	resp, err := c.do(request)
	if err != nil {
		return api.RuntimeSummaryResponse{}, err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return api.RuntimeSummaryResponse{}, decodeAPIError(resp)
	}
	var out api.RuntimeSummaryResponse
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return api.RuntimeSummaryResponse{}, err
	}
	return out, nil
}

func (c *Client) ClearRuntimeSession(sessionID string) (api.RuntimeSummaryResponse, error) {
	request, err := http.NewRequest(http.MethodDelete, c.baseURL+"/api/runtime/sessions/"+sessionID, nil)
	if err != nil {
		return api.RuntimeSummaryResponse{}, err
	}
	resp, err := c.do(request)
	if err != nil {
		return api.RuntimeSummaryResponse{}, err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return api.RuntimeSummaryResponse{}, decodeAPIError(resp)
	}
	var out api.RuntimeSummaryResponse
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return api.RuntimeSummaryResponse{}, err
	}
	return out, nil
}

func (c *Client) MemorySearch(chatID int64, sessionID, query string, limit int) (api.MemorySearchResponse, error) {
	req, err := http.NewRequest(http.MethodGet, c.baseURL+"/api/memory/search", nil)
	if err != nil {
		return api.MemorySearchResponse{}, err
	}
	q := req.URL.Query()
	q.Set("chat_id", fmt.Sprintf("%d", chatID))
	q.Set("session_id", sessionID)
	q.Set("query", query)
	if limit > 0 {
		q.Set("limit", fmt.Sprintf("%d", limit))
	}
	req.URL.RawQuery = q.Encode()
	resp, err := c.do(req)
	if err != nil {
		return api.MemorySearchResponse{}, err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return api.MemorySearchResponse{}, decodeAPIError(resp)
	}
	var out api.MemorySearchResponse
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return api.MemorySearchResponse{}, err
	}
	return out, nil
}

func (c *Client) MemoryRead(docKey string) (api.MemoryDocumentResponse, error) {
	var out api.MemoryDocumentResponse
	err := c.getJSON("/api/memory/"+docKey, &out)
	return out, err
}

func (c *Client) Artifact(ref string) (api.ArtifactResponse, error) {
	var out api.ArtifactResponse
	err := c.getJSON("/api/artifacts/"+url.PathEscape(ref), &out)
	return out, err
}

func (c *Client) ArtifactContent(ref string) (api.ArtifactContentResponse, error) {
	req, err := http.NewRequest(http.MethodGet, c.baseURL+"/api/artifacts/"+url.PathEscape(ref)+"/content", nil)
	if err != nil {
		return api.ArtifactContentResponse{}, err
	}
	resp, err := c.do(req)
	if err != nil {
		return api.ArtifactContentResponse{}, err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return api.ArtifactContentResponse{}, decodeAPIError(resp)
	}
	body, err := io.ReadAll(resp.Body)
	if err != nil {
		return api.ArtifactContentResponse{}, err
	}
	return api.ArtifactContentResponse{Content: string(body)}, nil
}

func (c *Client) ArtifactSearch(req api.ArtifactSearchRequest) (api.ArtifactSearchResponse, error) {
	httpReq, err := http.NewRequest(http.MethodGet, c.baseURL+"/api/artifacts/search", nil)
	if err != nil {
		return api.ArtifactSearchResponse{}, err
	}
	q := httpReq.URL.Query()
	if req.OwnerType != "" {
		q.Set("owner_type", req.OwnerType)
	}
	if req.OwnerID != "" {
		q.Set("owner_id", req.OwnerID)
	}
	if req.RunID != "" {
		q.Set("run_id", req.RunID)
	}
	if req.WorkerID != "" {
		q.Set("worker_id", req.WorkerID)
	}
	if req.Query != "" {
		q.Set("query", req.Query)
	}
	if req.Limit > 0 {
		q.Set("limit", fmt.Sprintf("%d", req.Limit))
	}
	if req.Global {
		q.Set("global", "true")
	}
	httpReq.URL.RawQuery = q.Encode()
	resp, err := c.do(httpReq)
	if err != nil {
		return api.ArtifactSearchResponse{}, err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return api.ArtifactSearchResponse{}, decodeAPIError(resp)
	}
	var out api.ArtifactSearchResponse
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return api.ArtifactSearchResponse{}, err
	}
	return out, nil
}

func (c *Client) Plans(ownerType, ownerID string, limit int) (api.PlanListResponse, error) {
	req, err := http.NewRequest(http.MethodGet, c.baseURL+"/api/plans", nil)
	if err != nil {
		return api.PlanListResponse{}, err
	}
	q := req.URL.Query()
	if ownerType != "" {
		q.Set("owner_type", ownerType)
	}
	if ownerID != "" {
		q.Set("owner_id", ownerID)
	}
	if limit > 0 {
		q.Set("limit", fmt.Sprintf("%d", limit))
	}
	req.URL.RawQuery = q.Encode()
	resp, err := c.do(req)
	if err != nil {
		return api.PlanListResponse{}, err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return api.PlanListResponse{}, decodeAPIError(resp)
	}
	var out api.PlanListResponse
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return api.PlanListResponse{}, err
	}
	return out, nil
}

func (c *Client) Plan(planID string) (api.PlanResponse, error) {
	var out api.PlanResponse
	err := c.getJSON("/api/plans/"+planID, &out)
	return out, err
}

func (c *Client) CreatePlan(req api.CreatePlanRequest) (api.PlanResponse, error) {
	body, _ := json.Marshal(req)
	httpReq, err := http.NewRequest(http.MethodPost, c.baseURL+"/api/plans", bytes.NewReader(body))
	if err != nil {
		return api.PlanResponse{}, err
	}
	httpReq.Header.Set("Content-Type", "application/json")
	resp, err := c.do(httpReq)
	if err != nil {
		return api.PlanResponse{}, err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return api.PlanResponse{}, decodeAPIError(resp)
	}
	var out api.PlanResponse
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return api.PlanResponse{}, err
	}
	return out, nil
}

func (c *Client) ReplacePlanItems(planID string, req api.ReplacePlanItemsRequest) (api.PlanResponse, error) {
	body, _ := json.Marshal(req)
	httpReq, err := http.NewRequest(http.MethodPut, c.baseURL+"/api/plans/"+planID+"/items", bytes.NewReader(body))
	if err != nil {
		return api.PlanResponse{}, err
	}
	httpReq.Header.Set("Content-Type", "application/json")
	resp, err := c.do(httpReq)
	if err != nil {
		return api.PlanResponse{}, err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return api.PlanResponse{}, decodeAPIError(resp)
	}
	var out api.PlanResponse
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return api.PlanResponse{}, err
	}
	return out, nil
}

func (c *Client) AppendPlanNote(planID, note string) (api.PlanResponse, error) {
	body, _ := json.Marshal(api.AppendPlanNoteRequest{Note: note})
	httpReq, err := http.NewRequest(http.MethodPost, c.baseURL+"/api/plans/"+planID+"/notes", bytes.NewReader(body))
	if err != nil {
		return api.PlanResponse{}, err
	}
	httpReq.Header.Set("Content-Type", "application/json")
	resp, err := c.do(httpReq)
	if err != nil {
		return api.PlanResponse{}, err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return api.PlanResponse{}, decodeAPIError(resp)
	}
	var out api.PlanResponse
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return api.PlanResponse{}, err
	}
	return out, nil
}

func (c *Client) StartPlanItem(planID, itemID string) (api.PlanResponse, error) {
	return c.postPlanAction(planID, itemID, "start")
}

func (c *Client) CompletePlanItem(planID, itemID string) (api.PlanResponse, error) {
	return c.postPlanAction(planID, itemID, "complete")
}

func (c *Client) postPlanAction(planID, itemID, action string) (api.PlanResponse, error) {
	httpReq, err := http.NewRequest(http.MethodPost, c.baseURL+"/api/plans/"+planID+"/items/"+itemID+"/"+action, nil)
	if err != nil {
		return api.PlanResponse{}, err
	}
	resp, err := c.do(httpReq)
	if err != nil {
		return api.PlanResponse{}, err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return api.PlanResponse{}, decodeAPIError(resp)
	}
	var out api.PlanResponse
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return api.PlanResponse{}, err
	}
	return out, nil
}

func (c *Client) StartJob(req api.CreateJobRequest) (api.CreateJobResponse, error) {
	body, _ := json.Marshal(req)
	httpReq, err := http.NewRequest(http.MethodPost, c.baseURL+"/api/jobs", bytes.NewReader(body))
	if err != nil {
		return api.CreateJobResponse{}, err
	}
	httpReq.Header.Set("Content-Type", "application/json")
	resp, err := c.do(httpReq)
	if err != nil {
		return api.CreateJobResponse{}, err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return api.CreateJobResponse{}, decodeAPIError(resp)
	}
	var out api.CreateJobResponse
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return api.CreateJobResponse{}, err
	}
	return out, nil
}

func (c *Client) Jobs(limit int) (api.JobListResponse, error) {
	req, err := http.NewRequest(http.MethodGet, c.baseURL+"/api/jobs", nil)
	if err != nil {
		return api.JobListResponse{}, err
	}
	q := req.URL.Query()
	if limit > 0 {
		q.Set("limit", fmt.Sprintf("%d", limit))
	}
	req.URL.RawQuery = q.Encode()
	resp, err := c.do(req)
	if err != nil {
		return api.JobListResponse{}, err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return api.JobListResponse{}, decodeAPIError(resp)
	}
	var out api.JobListResponse
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return api.JobListResponse{}, err
	}
	return out, nil
}

func (c *Client) Job(jobID string) (api.JobStatusResponse, error) {
	var out api.JobStatusResponse
	err := c.getJSON("/api/jobs/"+jobID, &out)
	return out, err
}

func (c *Client) JobLogs(jobID, stream string, afterID int64, limit int) (api.JobLogsResponse, error) {
	req, err := http.NewRequest(http.MethodGet, c.baseURL+"/api/jobs/"+jobID+"/logs", nil)
	if err != nil {
		return api.JobLogsResponse{}, err
	}
	q := req.URL.Query()
	if stream != "" {
		q.Set("stream", stream)
	}
	if afterID > 0 {
		q.Set("after_id", fmt.Sprintf("%d", afterID))
	}
	if limit > 0 {
		q.Set("limit", fmt.Sprintf("%d", limit))
	}
	req.URL.RawQuery = q.Encode()
	resp, err := c.do(req)
	if err != nil {
		return api.JobLogsResponse{}, err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return api.JobLogsResponse{}, decodeAPIError(resp)
	}
	var out api.JobLogsResponse
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return api.JobLogsResponse{}, err
	}
	return out, nil
}

func (c *Client) CancelJob(jobID string) error {
	req, err := http.NewRequest(http.MethodPost, c.baseURL+"/api/jobs/"+jobID+"/cancel", nil)
	if err != nil {
		return err
	}
	resp, err := c.do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return decodeAPIError(resp)
	}
	return nil
}

func (c *Client) StartWorker(req api.CreateWorkerRequest) (api.WorkerStatusResponse, error) {
	body, _ := json.Marshal(req)
	httpReq, err := http.NewRequest(http.MethodPost, c.baseURL+"/api/workers", bytes.NewReader(body))
	if err != nil {
		return api.WorkerStatusResponse{}, err
	}
	httpReq.Header.Set("Content-Type", "application/json")
	resp, err := c.do(httpReq)
	if err != nil {
		return api.WorkerStatusResponse{}, err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return api.WorkerStatusResponse{}, decodeAPIError(resp)
	}
	var out api.WorkerStatusResponse
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return api.WorkerStatusResponse{}, err
	}
	return out, nil
}

func (c *Client) Workers(chatID int64, limit int) (api.WorkerListResponse, error) {
	req, err := http.NewRequest(http.MethodGet, c.baseURL+"/api/workers", nil)
	if err != nil {
		return api.WorkerListResponse{}, err
	}
	q := req.URL.Query()
	if chatID != 0 {
		q.Set("chat_id", fmt.Sprintf("%d", chatID))
	}
	if limit > 0 {
		q.Set("limit", fmt.Sprintf("%d", limit))
	}
	req.URL.RawQuery = q.Encode()
	resp, err := c.do(req)
	if err != nil {
		return api.WorkerListResponse{}, err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return api.WorkerListResponse{}, decodeAPIError(resp)
	}
	var out api.WorkerListResponse
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return api.WorkerListResponse{}, err
	}
	return out, nil
}

func (c *Client) Worker(workerID string) (api.WorkerStatusResponse, error) {
	var out api.WorkerStatusResponse
	err := c.getJSON("/api/workers/"+workerID, &out)
	return out, err
}

func (c *Client) MessageWorker(workerID, content string) (api.WorkerStatusResponse, error) {
	body, _ := json.Marshal(api.WorkerMessageRequest{Content: content})
	req, err := http.NewRequest(http.MethodPost, c.baseURL+"/api/workers/"+workerID+"/messages", bytes.NewReader(body))
	if err != nil {
		return api.WorkerStatusResponse{}, err
	}
	req.Header.Set("Content-Type", "application/json")
	resp, err := c.do(req)
	if err != nil {
		return api.WorkerStatusResponse{}, err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return api.WorkerStatusResponse{}, decodeAPIError(resp)
	}
	var out api.WorkerStatusResponse
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return api.WorkerStatusResponse{}, err
	}
	return out, nil
}

func (c *Client) WaitWorker(workerID string, afterCursor int, afterEventID int64) (api.WorkerWaitResponse, error) {
	req, err := http.NewRequest(http.MethodGet, c.baseURL+"/api/workers/"+workerID+"/wait", nil)
	if err != nil {
		return api.WorkerWaitResponse{}, err
	}
	q := req.URL.Query()
	q.Set("after_cursor", fmt.Sprintf("%d", afterCursor))
	q.Set("after_event_id", fmt.Sprintf("%d", afterEventID))
	req.URL.RawQuery = q.Encode()
	resp, err := c.do(req)
	if err != nil {
		return api.WorkerWaitResponse{}, err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return api.WorkerWaitResponse{}, decodeAPIError(resp)
	}
	var out api.WorkerWaitResponse
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return api.WorkerWaitResponse{}, err
	}
	return out, nil
}

func (c *Client) WorkerHandoff(workerID string) (api.WorkerHandoffResponse, error) {
	var out api.WorkerHandoffResponse
	err := c.getJSON("/api/workers/"+workerID+"/handoff", &out)
	return out, err
}

func (c *Client) CloseWorker(workerID string) (api.WorkerStatusResponse, error) {
	req, err := http.NewRequest(http.MethodPost, c.baseURL+"/api/workers/"+workerID+"/close", nil)
	if err != nil {
		return api.WorkerStatusResponse{}, err
	}
	resp, err := c.do(req)
	if err != nil {
		return api.WorkerStatusResponse{}, err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return api.WorkerStatusResponse{}, decodeAPIError(resp)
	}
	var out api.WorkerStatusResponse
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return api.WorkerStatusResponse{}, err
	}
	return out, nil
}

func (c *Client) Sessions(chatID int64, limit int) (api.SessionListResponse, error) {
	req, err := http.NewRequest(http.MethodGet, c.baseURL+"/api/sessions", nil)
	if err != nil {
		return api.SessionListResponse{}, err
	}
	q := req.URL.Query()
	if chatID != 0 {
		q.Set("chat_id", fmt.Sprintf("%d", chatID))
	}
	if limit > 0 {
		q.Set("limit", fmt.Sprintf("%d", limit))
	}
	req.URL.RawQuery = q.Encode()
	resp, err := c.do(req)
	if err != nil {
		return api.SessionListResponse{}, err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return api.SessionListResponse{}, decodeAPIError(resp)
	}
	var out api.SessionListResponse
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return api.SessionListResponse{}, err
	}
	return out, nil
}

func (c *Client) Session(sessionID string) (api.SessionStateResponse, error) {
	var out api.SessionStateResponse
	err := c.getJSON("/api/sessions/"+sessionID, &out)
	return out, err
}

func (c *Client) ControlState(sessionID string, chatID int64) (api.ControlStateResponse, error) {
	var out api.ControlStateResponse
	path := "/api/control/" + sessionID
	if chatID != 0 {
		path += "?chat_id=" + fmt.Sprintf("%d", chatID)
	}
	err := c.getJSON(path, &out)
	return out, err
}

func (c *Client) ControlAction(sessionID string, req api.ControlActionRequest) (api.ControlActionResponse, error) {
	body, _ := json.Marshal(req)
	httpReq, err := http.NewRequest(http.MethodPost, c.baseURL+"/api/control/"+sessionID+"/actions", bytes.NewReader(body))
	if err != nil {
		return api.ControlActionResponse{}, err
	}
	httpReq.Header.Set("Content-Type", "application/json")
	resp, err := c.do(httpReq)
	if err != nil {
		return api.ControlActionResponse{}, err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return api.ControlActionResponse{}, decodeAPIError(resp)
	}
	var out api.ControlActionResponse
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return api.ControlActionResponse{}, err
	}
	return out, nil
}

func (c *Client) SessionAction(req api.SessionActionRequest) (api.SessionActionResponse, error) {
	body, _ := json.Marshal(req)
	httpReq, err := http.NewRequest(http.MethodPost, c.baseURL+"/api/session-actions", bytes.NewReader(body))
	if err != nil {
		return api.SessionActionResponse{}, err
	}
	httpReq.Header.Set("Content-Type", "application/json")
	resp, err := c.do(httpReq)
	if err != nil {
		return api.SessionActionResponse{}, err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return api.SessionActionResponse{}, decodeAPIError(resp)
	}
	var out api.SessionActionResponse
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return api.SessionActionResponse{}, err
	}
	return out, nil
}

func (c *Client) UpdateSession(sessionID string, req api.SessionOverrideRequest) (api.RuntimeSummaryResponse, error) {
	body, _ := json.Marshal(req)
	request, err := http.NewRequest(http.MethodPatch, c.baseURL+"/api/sessions/"+sessionID, bytes.NewReader(body))
	if err != nil {
		return api.RuntimeSummaryResponse{}, err
	}
	request.Header.Set("Content-Type", "application/json")
	resp, err := c.do(request)
	if err != nil {
		return api.RuntimeSummaryResponse{}, err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return api.RuntimeSummaryResponse{}, decodeAPIError(resp)
	}
	var out api.RuntimeSummaryResponse
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return api.RuntimeSummaryResponse{}, err
	}
	return out, nil
}

func (c *Client) ClearSession(sessionID string) (api.RuntimeSummaryResponse, error) {
	request, err := http.NewRequest(http.MethodDelete, c.baseURL+"/api/sessions/"+sessionID, nil)
	if err != nil {
		return api.RuntimeSummaryResponse{}, err
	}
	resp, err := c.do(request)
	if err != nil {
		return api.RuntimeSummaryResponse{}, err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return api.RuntimeSummaryResponse{}, decodeAPIError(resp)
	}
	var out api.RuntimeSummaryResponse
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return api.RuntimeSummaryResponse{}, err
	}
	return out, nil
}

func (c *Client) StartRun(chatID int64, sessionID, query string) (api.CreateRunResponse, error) {
	body, _ := json.Marshal(api.CreateRunRequest{
		ChatID:    chatID,
		SessionID: sessionID,
		Query:     query,
	})
	req, err := http.NewRequest(http.MethodPost, c.baseURL+"/api/runs", bytes.NewReader(body))
	if err != nil {
		return api.CreateRunResponse{}, err
	}
	req.Header.Set("Content-Type", "application/json")
	resp, err := c.do(req)
	if err != nil {
		return api.CreateRunResponse{}, err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return api.CreateRunResponse{}, decodeAPIError(resp)
	}
	var out api.CreateRunResponse
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return api.CreateRunResponse{}, err
	}
	return out, nil
}

func (c *Client) RunStatus(runID string) (api.RunStatusResponse, error) {
	var out api.RunStatusResponse
	err := c.getJSON("/api/runs/"+runID, &out)
	return out, err
}

func (c *Client) RunReplay(runID string) (api.RunReplayResponse, error) {
	var out api.RunReplayResponse
	err := c.getJSON("/api/runs/"+runID+"/replay", &out)
	return out, err
}

func (c *Client) Events(req api.EventListRequest) (api.EventListResponse, error) {
	httpReq, err := http.NewRequest(http.MethodGet, c.baseURL+"/api/events", nil)
	if err != nil {
		return api.EventListResponse{}, err
	}
	httpReq.URL.RawQuery = encodeEventListQuery(httpReq.URL.Query(), req).Encode()
	resp, err := c.do(httpReq)
	if err != nil {
		return api.EventListResponse{}, err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return api.EventListResponse{}, decodeAPIError(resp)
	}
	var out api.EventListResponse
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return api.EventListResponse{}, err
	}
	return out, nil
}

func (c *Client) StreamEvents(ctx context.Context, req api.EventListRequest, onEvent func(runtime.RuntimeEvent) error) error {
	httpReq, err := http.NewRequestWithContext(ctx, http.MethodGet, c.baseURL+"/api/events/stream", nil)
	if err != nil {
		return err
	}
	httpReq.Header.Set("Accept", "text/event-stream")
	httpReq.URL.RawQuery = encodeEventListQuery(httpReq.URL.Query(), req).Encode()
	resp, err := c.do(httpReq)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return decodeAPIError(resp)
	}
	scanner := bufio.NewScanner(resp.Body)
	scanner.Buffer(make([]byte, 0, 64*1024), 1024*1024)
	var dataLines []string
	for scanner.Scan() {
		line := scanner.Text()
		switch {
		case line == "":
			if len(dataLines) == 0 {
				continue
			}
			var item runtime.RuntimeEvent
			if err := json.Unmarshal([]byte(strings.Join(dataLines, "\n")), &item); err != nil {
				return err
			}
			dataLines = dataLines[:0]
			if err := onEvent(item); err != nil {
				return err
			}
		case strings.HasPrefix(line, ":"):
			continue
		case strings.HasPrefix(line, "data:"):
			dataLines = append(dataLines, strings.TrimSpace(strings.TrimPrefix(line, "data:")))
		}
	}
	if err := scanner.Err(); err != nil {
		return err
	}
	return ctx.Err()
}

func encodeEventListQuery(q url.Values, req api.EventListRequest) url.Values {
	if strings.TrimSpace(req.EntityType) != "" {
		q.Set("entity_type", req.EntityType)
	}
	if strings.TrimSpace(req.EntityID) != "" {
		q.Set("entity_id", req.EntityID)
	}
	if strings.TrimSpace(req.RunID) != "" {
		q.Set("run_id", req.RunID)
	}
	if strings.TrimSpace(req.SessionID) != "" {
		q.Set("session_id", req.SessionID)
	}
	if req.AfterID > 0 {
		q.Set("after_id", fmt.Sprintf("%d", req.AfterID))
	}
	if req.Limit > 0 {
		q.Set("limit", fmt.Sprintf("%d", req.Limit))
	}
	return q
}

func (c *Client) Runs(chatID int64, sessionID, status string, limit int) (api.RunListResponse, error) {
	req, err := http.NewRequest(http.MethodGet, c.baseURL+"/api/runs", nil)
	if err != nil {
		return api.RunListResponse{}, err
	}
	q := req.URL.Query()
	if chatID != 0 {
		q.Set("chat_id", fmt.Sprintf("%d", chatID))
	}
	if strings.TrimSpace(sessionID) != "" {
		q.Set("session_id", sessionID)
	}
	if strings.TrimSpace(status) != "" {
		q.Set("status", status)
	}
	if limit > 0 {
		q.Set("limit", fmt.Sprintf("%d", limit))
	}
	req.URL.RawQuery = q.Encode()
	resp, err := c.do(req)
	if err != nil {
		return api.RunListResponse{}, err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return api.RunListResponse{}, decodeAPIError(resp)
	}
	var out api.RunListResponse
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return api.RunListResponse{}, err
	}
	return out, nil
}

func (c *Client) CancelRun(runID string) error {
	req, err := http.NewRequest(http.MethodPost, c.baseURL+"/api/runs/"+runID+"/cancel", nil)
	if err != nil {
		return err
	}
	resp, err := c.do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return decodeAPIError(resp)
	}
	return nil
}

func (c *Client) Approvals(sessionID string) ([]api.ApprovalRecordResponse, error) {
	var out []api.ApprovalRecordResponse
	err := c.getJSON("/api/approvals?session_id="+sessionID, &out)
	return out, err
}

func (c *Client) Approve(id string) (api.ApprovalRecordResponse, error) {
	return c.decideApproval(id, "approve")
}

func (c *Client) Reject(id string) (api.ApprovalRecordResponse, error) {
	return c.decideApproval(id, "reject")
}

func (c *Client) decideApproval(id, action string) (api.ApprovalRecordResponse, error) {
	req, err := http.NewRequest(http.MethodPost, c.baseURL+"/api/approvals/"+id+"/"+action, nil)
	if err != nil {
		return api.ApprovalRecordResponse{}, err
	}
	req.Header.Set("X-Update-ID", "cli-"+id+"-"+action)
	resp, err := c.do(req)
	if err != nil {
		return api.ApprovalRecordResponse{}, err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return api.ApprovalRecordResponse{}, decodeAPIError(resp)
	}
	var out api.ApprovalRecordResponse
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return api.ApprovalRecordResponse{}, err
	}
	return out, nil
}

func (c *Client) getJSON(path string, out any) error {
	req, err := http.NewRequest(http.MethodGet, c.baseURL+path, nil)
	if err != nil {
		return err
	}
	resp, err := c.do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return decodeAPIError(resp)
	}
	return json.NewDecoder(resp.Body).Decode(out)
}

func decodeAPIError(resp *http.Response) error {
	body, _ := io.ReadAll(resp.Body)
	var out api.ErrorResponse
	if err := json.Unmarshal(body, &out); err == nil && out.Error.Code != "" {
		msg := out.Error.Message
		if out.Error.EntityType != "" || out.Error.EntityID != "" {
			msg = fmt.Sprintf("%s [%s/%s]", msg, out.Error.EntityType, out.Error.EntityID)
		}
		if out.Error.Retryable {
			msg += " (retryable)"
		}
		return fmt.Errorf("%s: %s", out.Error.Code, msg)
	}
	if len(body) > 0 {
		return fmt.Errorf("api request failed: %s: %s", resp.Status, strings.TrimSpace(string(body)))
	}
	return fmt.Errorf("api request failed: %s", resp.Status)
}
