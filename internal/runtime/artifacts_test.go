package runtime

import (
	"context"
	"strings"
	"testing"

	"teamd/internal/artifacts"
	"teamd/internal/contracts"
)

func TestMaybeOffloadToolResultAddsSummaryAndMetadata(t *testing.T) {
	store, err := artifacts.NewStore(t.TempDir())
	if err != nil {
		t.Fatalf("NewStore returned error: %v", err)
	}
	agent := &Agent{ArtifactStore: store}
	content := "alpha\nbeta\nwarning line\ngamma\ndelta"

	displayText, refs, err := agent.maybeOffloadToolResult(context.Background(), offloadContracts(12, 18), "fs_read_text", content)
	if err != nil {
		t.Fatalf("maybeOffloadToolResult returned error: %v", err)
	}
	if len(refs) != 1 || !strings.HasPrefix(refs[0], "artifact://") {
		t.Fatalf("artifact refs = %#v, want single artifact ref", refs)
	}
	if !strings.Contains(displayText, `"summary":"text output offloaded; 5 lines; markers=warn; sample=\"alpha\""`) {
		t.Fatalf("display text missing summary: %s", displayText)
	}
	if !strings.Contains(displayText, `"line_count":5`) {
		t.Fatalf("display text missing line count: %s", displayText)
	}
	if !strings.Contains(displayText, `"token_estimate":9`) {
		t.Fatalf("display text missing token estimate: %s", displayText)
	}
	if !strings.Contains(displayText, `"truncated":true`) {
		t.Fatalf("display text missing truncated metadata: %s", displayText)
	}
}

func TestMaybeOffloadToolResultSummarizesShellErrors(t *testing.T) {
	store, err := artifacts.NewStore(t.TempDir())
	if err != nil {
		t.Fatalf("NewStore returned error: %v", err)
	}
	agent := &Agent{ArtifactStore: store}
	content := "INFO boot\nWARN slow dependency\nERROR build failed\nexit status 1\nstack line"

	displayText, _, err := agent.maybeOffloadToolResult(context.Background(), offloadContracts(16, 24), "shell_exec", content)
	if err != nil {
		t.Fatalf("maybeOffloadToolResult returned error: %v", err)
	}
	if !strings.Contains(displayText, `shell output offloaded; 5 lines; markers=error,warn,fail`) {
		t.Fatalf("display text missing shell summary: %s", displayText)
	}
}

func TestMaybeOffloadToolResultSummarizesJSONShape(t *testing.T) {
	store, err := artifacts.NewStore(t.TempDir())
	if err != nil {
		t.Fatalf("NewStore returned error: %v", err)
	}
	agent := &Agent{ArtifactStore: store}
	content := "{\n  \"status\": \"error\",\n  \"count\": 3,\n  \"items\": [1,2,3],\n  \"message\": \"boom\"\n}"

	displayText, _, err := agent.maybeOffloadToolResult(context.Background(), offloadContracts(16, 24), "fs_read_text", content)
	if err != nil {
		t.Fatalf("maybeOffloadToolResult returned error: %v", err)
	}
	if !strings.Contains(displayText, `json object offloaded; keys=count,items,message,status; status=error; count=3`) {
		t.Fatalf("display text missing json summary: %s", displayText)
	}
}

func offloadContracts(maxChars, previewChars int) contracts.ResolvedContracts {
	return contracts.ResolvedContracts{
		Memory: contracts.MemoryContract{
			Offload: contracts.OffloadPolicy{
				Enabled:  true,
				Strategy: "artifact_store",
				Params: contracts.OffloadParams{
					MaxChars:     maxChars,
					PreviewChars: previewChars,
				},
			},
		},
	}
}
