package daemon

import (
	"strings"

	"teamd/internal/runtime"
	"teamd/internal/runtime/workspace"
)

func (s *Server) workspaceFilesManager() (*workspace.WorkspaceFilesManager, error) {
	agent := s.currentAgent()
	root := workspaceRootForAgent(agent)

	s.workspaceMu.Lock()
	defer s.workspaceMu.Unlock()

	if s.workspaceFiles != nil && s.workspaceFilesRoot == root {
		return s.workspaceFiles, nil
	}
	mgr, err := workspace.NewWorkspaceFilesManager(root)
	if err != nil {
		return nil, err
	}
	s.workspaceFiles = mgr
	s.workspaceFilesRoot = root
	return mgr, nil
}

func (s *Server) workspaceEditorManager() (*workspace.WorkspaceEditorManager, error) {
	agent := s.currentAgent()
	root := workspaceRootForAgent(agent)

	s.workspaceMu.Lock()
	defer s.workspaceMu.Unlock()

	if s.workspaceEditor != nil && s.workspaceEditorRoot == root {
		return s.workspaceEditor, nil
	}
	mgr, err := workspace.NewWorkspaceEditorManager(root)
	if err != nil {
		return nil, err
	}
	s.workspaceEditor = mgr
	s.workspaceEditorRoot = root
	return mgr, nil
}

func (s *Server) workspaceArtifactsManager() (*workspace.WorkspaceArtifactsManager, error) {
	agent := s.currentAgent()
	if agent == nil {
		return nil, nil
	}
	root, err := agent.ArtifactStorePath()
	if err != nil || strings.TrimSpace(root) == "" {
		return nil, nil
	}

	s.workspaceMu.Lock()
	defer s.workspaceMu.Unlock()

	if s.workspaceArtifacts != nil && s.workspaceArtifactsRoot == root {
		return s.workspaceArtifacts, nil
	}
	mgr, err := workspace.NewWorkspaceArtifactsManager(root)
	if err != nil {
		return nil, err
	}
	s.workspaceArtifacts = mgr
	s.workspaceArtifactsRoot = root
	return mgr, nil
}

func workspaceRootForAgent(agent *runtime.Agent) string {
	if agent == nil {
		return "."
	}
	root := strings.TrimSpace(agent.Contracts.FilesystemExecution.Scope.Params.RootPath)
	if root == "" {
		return "."
	}
	return root
}
