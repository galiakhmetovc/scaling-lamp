package daemon

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestPersistShellApprovalRuleAndReloadUpdatesPrefixes(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	sourceRoot := filepath.Join("..", "..", "..", "config", "zai-smoke")
	targetRoot := filepath.Join(dir, "zai-smoke")
	copyDir(t, sourceRoot, targetRoot)

	agentPath := filepath.Join(targetRoot, "agent.yaml")
	agentYAML, err := os.ReadFile(agentPath)
	if err != nil {
		t.Fatalf("ReadFile(%q): %v", agentPath, err)
	}
	patched := strings.Replace(
		string(agentYAML),
		"    projection_store_path: ../../var/zai-smoke/projections.json\n",
		"    projection_store_path: ./var/projections.json\n",
		1,
	)
	if patched == string(agentYAML) {
		t.Fatalf("failed to patch projection_store_path in %q", agentPath)
	}
	if err := os.WriteFile(agentPath, []byte(patched), 0o644); err != nil {
		t.Fatalf("WriteFile(%q): %v", agentPath, err)
	}

	reloaded, err := PersistShellApprovalRuleAndReload(agentPath, "allow", "go test")
	if err != nil {
		t.Fatalf("PersistShellApprovalRuleAndReload allow: %v", err)
	}
	if len(reloaded.Contracts.ShellExecution.Approval.Params.AllowPrefixes) != 1 || reloaded.Contracts.ShellExecution.Approval.Params.AllowPrefixes[0] != "go test" {
		t.Fatalf("allow prefixes = %#v", reloaded.Contracts.ShellExecution.Approval.Params.AllowPrefixes)
	}

	reloaded, err = PersistShellApprovalRuleAndReload(agentPath, "deny", "go env")
	if err != nil {
		t.Fatalf("PersistShellApprovalRuleAndReload deny: %v", err)
	}
	if len(reloaded.Contracts.ShellExecution.Approval.Params.DenyPrefixes) != 1 || reloaded.Contracts.ShellExecution.Approval.Params.DenyPrefixes[0] != "go env" {
		t.Fatalf("deny prefixes = %#v", reloaded.Contracts.ShellExecution.Approval.Params.DenyPrefixes)
	}
}

func copyDir(t *testing.T, sourceRoot, targetRoot string) {
	t.Helper()
	if err := filepath.Walk(sourceRoot, func(sourcePath string, info os.FileInfo, err error) error {
		if err != nil {
			return err
		}
		rel, err := filepath.Rel(sourceRoot, sourcePath)
		if err != nil {
			return err
		}
		targetPath := filepath.Join(targetRoot, rel)
		if info.IsDir() {
			return os.MkdirAll(targetPath, 0o755)
		}
		body, err := os.ReadFile(sourcePath)
		if err != nil {
			return err
		}
		return os.WriteFile(targetPath, body, 0o644)
	}); err != nil {
		t.Fatalf("copyDir(%q, %q): %v", sourceRoot, targetRoot, err)
	}
}
