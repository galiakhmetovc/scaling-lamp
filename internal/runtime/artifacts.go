package runtime

import (
	"context"
	"encoding/json"
	"fmt"
	"path/filepath"

	"teamd/internal/artifacts"
	"teamd/internal/contracts"
	"teamd/internal/tools"
)

func (a *Agent) artifactToolDefinitions(contractSet contracts.ResolvedContracts) ([]tools.Definition, error) {
	return artifacts.NewDefinitionExecutor().Build(contractSet.Memory)
}

func (a *Agent) ensureArtifactStore(contractSet contracts.ResolvedContracts) (*artifacts.Store, error) {
	if a == nil {
		return nil, fmt.Errorf("agent is nil")
	}
	if a.ArtifactStore != nil {
		return a.ArtifactStore, nil
	}
	if !contractSet.Memory.Offload.Enabled || contractSet.Memory.Offload.Strategy != "artifact_store" {
		return nil, nil
	}
	root := contractSet.Memory.Offload.Params.StoragePath
	if root == "" {
		if a.ConfigPath == "" {
			return nil, fmt.Errorf("artifact storage path is not configured")
		}
		root = filepath.Join(filepath.Dir(a.ConfigPath), "var", "artifacts")
	}
	store, err := artifacts.NewStore(root)
	if err != nil {
		return nil, err
	}
	a.ArtifactStore = store
	return store, nil
}

func (a *Agent) maybeOffloadToolResult(ctx context.Context, contractSet contracts.ResolvedContracts, toolName, resultText string) (string, []string, error) {
	policy := contractSet.Memory.Offload
	if !policy.Enabled || policy.Strategy != "artifact_store" {
		return resultText, nil, nil
	}
	if toolName == "artifact_read" || toolName == "artifact_search" {
		return resultText, nil, nil
	}
	maxChars := policy.Params.MaxChars
	if maxChars <= 0 || len(resultText) <= maxChars {
		return resultText, nil, nil
	}
	store, err := a.ensureArtifactStore(contractSet)
	if err != nil {
		return "", nil, err
	}
	if store == nil {
		return resultText, nil, nil
	}
	record, err := store.Write(ctx, toolName, resultText, policy.Params.PreviewChars)
	if err != nil {
		return "", nil, err
	}
	return jsonString(map[string]any{
		"status":         "ok",
		"tool":           toolName,
		"offloaded":      true,
		"artifact_ref":   record.Ref,
		"size_chars":     record.SizeChars,
		"size_bytes":     record.SizeBytes,
		"preview":        record.Preview,
			"retrieval_hint": fmt.Sprintf("Use artifact_read with artifact_ref %q to inspect the full content.", record.Ref),
		}), []string{record.Ref}, nil
}

func (a *Agent) executeArtifactCommand(ctx context.Context, contractSet contracts.ResolvedContracts, callName string, args map[string]any) (string, error) {
	store, err := a.ensureArtifactStore(contractSet)
	if err != nil {
		return "", err
	}
	if store == nil {
		return "", fmt.Errorf("artifact store is not configured")
	}
	switch callName {
	case "artifact_read":
		artifactRef, err := stringArg(args, "artifact_ref")
		if err != nil {
			return "", err
		}
		record, content, err := store.Read(ctx, artifactRef)
		if err != nil {
			return "", err
		}
		body, err := json.Marshal(map[string]any{
			"status":       "ok",
			"tool":         callName,
			"artifact_ref": record.Ref,
			"tool_name":    record.ToolName,
			"size_chars":   record.SizeChars,
			"size_bytes":   record.SizeBytes,
			"content":      content,
		})
		if err != nil {
			return "", fmt.Errorf("encode artifact read result: %w", err)
		}
		return string(body), nil
	case "artifact_search":
		query, err := stringArg(args, "query")
		if err != nil {
			return "", err
		}
		limit, err := optionalIntArg(args, "limit")
		if err != nil {
			return "", err
		}
		if limit <= 0 {
			limit = contractSet.Memory.Offload.Params.SearchLimit
		}
		results, err := store.Search(ctx, query, limit)
		if err != nil {
			return "", err
		}
		body, err := json.Marshal(map[string]any{
			"status":  "ok",
			"tool":    callName,
			"query":   query,
			"results": results,
		})
		if err != nil {
			return "", fmt.Errorf("encode artifact search result: %w", err)
		}
		return string(body), nil
	default:
		return "", fmt.Errorf("artifact tool %q is not implemented", callName)
	}
}
