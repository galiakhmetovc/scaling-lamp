package daemon

import (
	"bufio"
	"fmt"
	"log/slog"
	"net"
	"net/http"
	"os"
	"time"

	"teamd/internal/contracts"
)

func newDaemonLogger(params contracts.DaemonServerParams) *slog.Logger {
	level := parseLogLevel(params.LogLevel)
	handler := slog.NewJSONHandler(os.Stderr, &slog.HandlerOptions{
		AddSource: params.LogAddSource,
		Level:     level,
	})
	return slog.New(handler)
}

func parseLogLevel(value string) slog.Level {
	switch value {
	case "debug":
		return slog.LevelDebug
	case "warn":
		return slog.LevelWarn
	case "error":
		return slog.LevelError
	default:
		return slog.LevelInfo
	}
}

func (s *Server) logInfo(msg string, attrs ...slog.Attr) {
	if s == nil || s.logger == nil {
		return
	}
	s.logger.LogAttrs(nil, slog.LevelInfo, msg, attrs...)
}

func (s *Server) logError(msg string, err error, attrs ...slog.Attr) {
	if s == nil || s.logger == nil {
		return
	}
	if err != nil {
		attrs = append(attrs, slog.String("error", err.Error()))
	}
	s.logger.LogAttrs(nil, slog.LevelError, msg, attrs...)
}

func (s *Server) loggingMiddleware(next http.Handler) http.Handler {
	if next == nil {
		return http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
			http.Error(w, "daemon handler is nil", http.StatusInternalServerError)
		})
	}
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		started := time.Now()
		rec := &statusRecorder{ResponseWriter: w, status: http.StatusOK}
		next.ServeHTTP(rec, r)
		s.logInfo("daemon.http.request.completed",
			slog.String("method", r.Method),
			slog.String("path", r.URL.Path),
			slog.String("remote_addr", r.RemoteAddr),
			slog.Int("status", rec.status),
			slog.Int64("duration_ms", time.Since(started).Milliseconds()),
		)
	})
}

type statusRecorder struct {
	http.ResponseWriter
	status int
}

func (r *statusRecorder) WriteHeader(status int) {
	r.status = status
	r.ResponseWriter.WriteHeader(status)
}

func (r *statusRecorder) Flush() {
	if flusher, ok := r.ResponseWriter.(http.Flusher); ok {
		flusher.Flush()
	}
}

func (r *statusRecorder) Hijack() (net.Conn, *bufio.ReadWriter, error) {
	hijacker, ok := r.ResponseWriter.(http.Hijacker)
	if !ok {
		return nil, nil, fmt.Errorf("response writer does not implement http.Hijacker")
	}
	return hijacker.Hijack()
}

func (r *statusRecorder) Push(target string, opts *http.PushOptions) error {
	if pusher, ok := r.ResponseWriter.(http.Pusher); ok {
		return pusher.Push(target, opts)
	}
	return http.ErrNotSupported
}

func commandSessionID(payload map[string]any) string {
	if payload == nil {
		return ""
	}
	if value, _ := payload["session_id"].(string); value != "" {
		return value
	}
	return ""
}

func payloadSessionID(payload any) string {
	value, ok := payload.(map[string]any)
	if !ok || value == nil {
		return ""
	}
	if sessionValue, ok := value["session"].(SessionSnapshot); ok {
		return sessionValue.SessionID
	}
	if sessionMap, ok := value["session"].(map[string]any); ok {
		if sessionID, _ := sessionMap["session_id"].(string); sessionID != "" {
			return sessionID
		}
	}
	if sessionID, _ := value["session_id"].(string); sessionID != "" {
		return sessionID
	}
	return ""
}

func (s *Server) logSettingsApply(scope string, values int, baseRevision string) {
	s.logInfo("daemon.settings.apply.requested",
		slog.String("scope", scope),
		slog.Int("value_count", values),
		slog.String("base_revision", baseRevision),
	)
}

func (s *Server) logSettingsReload(result string) {
	s.logInfo("daemon.settings.reload."+result)
}

func validateDaemonLogging(params contracts.DaemonServerParams) error {
	if params.LogFormat != "json" {
		return fmt.Errorf("daemon mode requires operator_surface.daemon_server.log_format=json")
	}
	switch params.LogLevel {
	case "debug", "info", "warn", "error":
		return nil
	default:
		return fmt.Errorf("daemon mode requires operator_surface.daemon_server.log_level to be one of debug, info, warn, error")
	}
}
