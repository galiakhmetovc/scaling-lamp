package daemon

type executionVersion string

const (
	executionVersionV1 executionVersion = "v1"
	executionVersionV2 executionVersion = "v2"
)

func (s *Server) defaultChatRunExecutionVersion() executionVersion {
	return executionVersionV1
}

func (s *Server) sessionExecutionVersion(sessionID string) executionVersion {
	s.runtimeMu.RLock()
	defer s.runtimeMu.RUnlock()
	if s == nil || s.sessionExecutionVersions == nil {
		return executionVersionV1
	}
	if version, ok := s.sessionExecutionVersions[sessionID]; ok && version != "" {
		return version
	}
	return executionVersionV1
}

func (s *Server) migrateSessionExecutionVersion(sessionID string, version executionVersion) {
	s.runtimeMu.Lock()
	defer s.runtimeMu.Unlock()
	if s.sessionExecutionVersions == nil {
		s.sessionExecutionVersions = map[string]executionVersion{}
	}
	if version == "" {
		version = executionVersionV1
	}
	s.sessionExecutionVersions[sessionID] = version
}

func (s *Server) startMainRunV2(sessionID string) bool {
	return s.startMainRunWithVersion(sessionID, executionVersionV2)
}
