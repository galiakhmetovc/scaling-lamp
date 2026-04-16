package daemon

import (
	"context"
	"embed"
	"encoding/json"
	"fmt"
	"io/fs"
	"net"
	"net/http"
	"net/http/httputil"
	"net/url"
	"path"
	"slices"
	"strings"
	"sync"
	"time"

	"golang.org/x/net/websocket"
	"teamd/internal/runtime"
)

//go:embed assets/*
var embeddedAssets embed.FS

type Server struct {
	agentMu        sync.RWMutex
	agent          *runtime.Agent
	httpServer     *http.Server
	listenAddr     string
	runtimeMu      sync.RWMutex
	sessionRuntime map[string]*sessionRuntimeState
	daemonBus      *daemonBus
}

type BootstrapPayload struct {
	AgentID     string                   `json:"agent_id"`
	ConfigPath  string                   `json:"config_path"`
	ListenAddr  string                   `json:"listen_addr"`
	Transport   ClientTransportSnapshot  `json:"transport"`
	Assets      WebAssetsSnapshot        `json:"assets"`
	Settings    SettingsSnapshot         `json:"settings"`
	Sessions    []runtimeSessionSnapshot `json:"sessions"`
	GeneratedAt time.Time                `json:"generated_at"`
}

type ClientTransportSnapshot struct {
	EndpointPath  string `json:"endpoint_path"`
	WebsocketPath string `json:"websocket_path"`
}

type WebAssetsSnapshot struct {
	Mode string `json:"mode"`
}

type runtimeSessionSnapshot struct {
	SessionID    string    `json:"session_id"`
	CreatedAt    time.Time `json:"created_at"`
	LastActivity time.Time `json:"last_activity"`
	MessageCount int       `json:"message_count"`
}

func New(agent *runtime.Agent) (*Server, error) {
	if agent == nil {
		return nil, fmt.Errorf("agent is nil")
	}
	if strings.TrimSpace(agent.Contracts.OperatorSurface.ID) == "" {
		return nil, fmt.Errorf("daemon mode requires operator_surface contract configuration")
	}
	operatorSurface := agent.Contracts.OperatorSurface
	if strings.TrimSpace(operatorSurface.DaemonServer.ID) == "" {
		return nil, fmt.Errorf("daemon mode requires daemon server policy")
	}
	if strings.TrimSpace(operatorSurface.ClientTransport.ID) == "" {
		return nil, fmt.Errorf("daemon mode requires client transport policy")
	}
	if strings.TrimSpace(operatorSurface.Settings.ID) == "" {
		return nil, fmt.Errorf("daemon mode requires settings policy")
	}
	if strings.TrimSpace(operatorSurface.WebAssets.ID) == "" {
		return nil, fmt.Errorf("daemon mode requires web assets policy")
	}
	params := operatorSurface.DaemonServer.Params
	if strings.TrimSpace(params.ListenHost) == "" {
		return nil, fmt.Errorf("daemon mode requires non-empty operator_surface.daemon_server.listen_host")
	}
	if params.ListenPort <= 0 {
		return nil, fmt.Errorf("daemon mode requires operator_surface.daemon_server.listen_port > 0")
	}
	transport := operatorSurface.ClientTransport.Params
	if !strings.HasPrefix(strings.TrimSpace(transport.EndpointPath), "/") {
		return nil, fmt.Errorf("daemon mode requires operator_surface.client_transport.endpoint_path to start with '/'")
	}
	if !strings.HasPrefix(strings.TrimSpace(transport.WebSocketPath), "/") {
		return nil, fmt.Errorf("daemon mode requires operator_surface.client_transport.websocket_path to start with '/'")
	}
	if strings.TrimSpace(operatorSurface.WebAssets.Params.Mode) == "" {
		return nil, fmt.Errorf("daemon mode requires operator_surface.web_assets.mode")
	}

	server := &Server{
		agent:          agent,
		listenAddr:     net.JoinHostPort(params.ListenHost, fmt.Sprintf("%d", params.ListenPort)),
		sessionRuntime: map[string]*sessionRuntimeState{},
		daemonBus:      newDaemonBus(),
	}
	server.httpServer = &http.Server{
		Addr:    server.listenAddr,
		Handler: server.routes(),
	}
	return server, nil
}

func (s *Server) ListenAndServe(ctx context.Context) error {
	if s == nil || s.httpServer == nil {
		return fmt.Errorf("daemon server is not initialized")
	}
	errCh := make(chan error, 1)
	go func() {
		err := s.httpServer.ListenAndServe()
		if err == http.ErrServerClosed {
			err = nil
		}
		errCh <- err
	}()

	select {
	case err := <-errCh:
		return err
	case <-ctx.Done():
		shutdownCtx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		if err := s.httpServer.Shutdown(shutdownCtx); err != nil {
			return fmt.Errorf("shutdown daemon server: %w", err)
		}
		return <-errCh
	}
}

func (s *Server) ListenAddr() string {
	if s == nil {
		return ""
	}
	return s.listenAddr
}

func (s *Server) Handler() http.Handler {
	return s.routes()
}

func (s *Server) routes() http.Handler {
	mux := http.NewServeMux()
	operatorSurface := s.currentAgent().Contracts.OperatorSurface
	transport := operatorSurface.ClientTransport.Params

	mux.HandleFunc("/healthz", s.handleHealthz)
	mux.HandleFunc("/config.js", s.handleClientConfig)
	mux.HandleFunc(path.Join(transport.EndpointPath, "bootstrap"), s.handleBootstrap)
	mux.Handle(transport.WebSocketPath, s.websocketHandler())

	assetsHandler, err := s.assetsHandler()
	if err != nil {
		return http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
			http.Error(w, err.Error(), http.StatusInternalServerError)
		})
	}
	mux.Handle("/", assetsHandler)
	return mux
}

func (s *Server) handleHealthz(w http.ResponseWriter, _ *http.Request) {
	agent := s.currentAgent()
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(map[string]any{
		"ok":           true,
		"agent_id":     agent.Config.ID,
		"generated_at": agent.Now().UTC(),
	})
}

func (s *Server) handleBootstrap(w http.ResponseWriter, _ *http.Request) {
	agent := s.currentAgent()
	w.Header().Set("Content-Type", "application/json")
	payload := BootstrapPayload{
		AgentID:    agent.Config.ID,
		ConfigPath: agent.ConfigPath,
		ListenAddr: s.listenAddr,
		Transport: ClientTransportSnapshot{
			EndpointPath:  agent.Contracts.OperatorSurface.ClientTransport.Params.EndpointPath,
			WebsocketPath: agent.Contracts.OperatorSurface.ClientTransport.Params.WebSocketPath,
		},
		Assets: WebAssetsSnapshot{
			Mode: agent.Contracts.OperatorSurface.WebAssets.Params.Mode,
		},
		Sessions:    []runtimeSessionSnapshot{},
		GeneratedAt: agent.Now().UTC(),
	}
	for _, session := range agent.ListSessions() {
		payload.Sessions = append(payload.Sessions, runtimeSessionSnapshot{
			SessionID:    session.SessionID,
			CreatedAt:    session.CreatedAt,
			LastActivity: session.LastActivity,
			MessageCount: session.MessageCount,
		})
	}
	settings, err := s.settingsSnapshot()
	if err != nil {
		http.Error(w, fmt.Sprintf("build settings snapshot: %v", err), http.StatusInternalServerError)
		return
	}
	payload.Settings = settings
	_ = json.NewEncoder(w).Encode(payload)
}

func (s *Server) handleClientConfig(w http.ResponseWriter, _ *http.Request) {
	agent := s.currentAgent()
	w.Header().Set("Content-Type", "application/javascript")
	payload := map[string]any{
		"endpointPath":  agent.Contracts.OperatorSurface.ClientTransport.Params.EndpointPath,
		"websocketPath": agent.Contracts.OperatorSurface.ClientTransport.Params.WebSocketPath,
	}
	body, err := json.Marshal(payload)
	if err != nil {
		http.Error(w, fmt.Sprintf("marshal client config: %v", err), http.StatusInternalServerError)
		return
	}
	_, _ = fmt.Fprintf(w, "window.__TEAMD_CLIENT_CONFIG__ = %s;\n", body)
}

func (s *Server) websocketHandler() http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if err := s.validateWebsocketOrigin(r); err != nil {
			http.Error(w, err.Error(), http.StatusForbidden)
			return
		}
		websocket.Handler(func(conn *websocket.Conn) {
			defer conn.Close()
			ctx, cancel := context.WithCancel(r.Context())
			defer cancel()

			agent := s.currentAgent()
			subID, ch := agent.UIBus.Subscribe(128)
			defer agent.UIBus.Unsubscribe(subID)
			daemonSubID, daemonCh := s.daemonBus.Subscribe(128)
			defer s.daemonBus.Unsubscribe(daemonSubID)

			outbound := make(chan WebsocketEnvelope, 256)
			var writeWG sync.WaitGroup
			writeWG.Add(1)
			go func() {
				defer writeWG.Done()
				encoder := json.NewEncoder(conn)
				for envelope := range outbound {
					if err := encoder.Encode(envelope); err != nil {
						cancel()
						return
					}
				}
			}()

			send := func(envelope WebsocketEnvelope) {
				envelope.GeneratedAt = s.currentAgent().Now().UTC()
				select {
				case <-ctx.Done():
				case outbound <- envelope:
				}
			}
			send(WebsocketEnvelope{Type: "hello"})

			var producerWG sync.WaitGroup
			producerWG.Add(1)
			go func() {
				defer producerWG.Done()
				for {
					select {
					case <-ctx.Done():
						return
					case event, ok := <-ch:
						if !ok {
							return
						}
						send(WebsocketEnvelope{Type: "ui_event", Event: &event})
					case envelope, ok := <-daemonCh:
						if !ok {
							return
						}
						send(envelope)
					}
				}
			}()

			decoder := json.NewDecoder(conn)
			var commandWG sync.WaitGroup
			for {
				var req CommandRequest
				if err := decoder.Decode(&req); err != nil {
					cancel()
					break
				}
				if req.Type != "command" {
					send(WebsocketEnvelope{Type: "command_failed", ID: req.ID, Command: req.Command, Error: fmt.Sprintf("unsupported websocket message type %q", req.Type)})
					continue
				}
				send(WebsocketEnvelope{Type: "command_accepted", ID: req.ID, Command: req.Command})
				commandWG.Add(1)
				go func(req CommandRequest) {
					defer commandWG.Done()
					payload, err := s.executeCommand(ctx, req)
					if err != nil {
						send(WebsocketEnvelope{Type: "command_failed", ID: req.ID, Command: req.Command, Error: err.Error()})
						return
					}
					send(WebsocketEnvelope{Type: "command_completed", ID: req.ID, Command: req.Command, Payload: payload})
					if req.Command == "settings.form.apply" || req.Command == "settings.raw.apply" {
						s.publishSettingsApplied()
					}
				}(req)
			}

			commandWG.Wait()
			producerWG.Wait()
			close(outbound)
			writeWG.Wait()
		}).ServeHTTP(w, r)
	})
}

func (s *Server) validateWebsocketOrigin(r *http.Request) error {
	allowed := s.currentAgent().Contracts.OperatorSurface.DaemonServer.Params.AllowedOrigins
	if len(allowed) == 0 {
		return nil
	}
	origin := strings.TrimSpace(r.Header.Get("Origin"))
	if origin == "" {
		return fmt.Errorf("websocket origin is required")
	}
	if slices.Contains(allowed, "*") {
		return nil
	}
	for _, candidate := range allowed {
		if strings.EqualFold(strings.TrimSpace(candidate), origin) {
			return nil
		}
	}
	return fmt.Errorf("origin %q is not allowed", origin)
}

func (s *Server) assetsHandler() (http.Handler, error) {
	assets := s.currentAgent().Contracts.OperatorSurface.WebAssets.Params
	switch assets.Mode {
	case "embedded_assets":
		subFS, err := fs.Sub(embeddedAssets, "assets")
		if err != nil {
			return nil, fmt.Errorf("load embedded assets: %w", err)
		}
		files := http.FileServer(http.FS(subFS))
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			if r.URL.Path == "/" {
				http.ServeFileFS(w, r, subFS, "index.html")
				return
			}
			files.ServeHTTP(w, r)
		}), nil
	case "dev_proxy":
		if strings.TrimSpace(assets.DevProxyURL) == "" {
			return nil, fmt.Errorf("operator_surface.web_assets.dev_proxy_url is required for dev_proxy mode")
		}
		target, err := url.Parse(assets.DevProxyURL)
		if err != nil {
			return nil, fmt.Errorf("parse dev proxy url: %w", err)
		}
		return httputil.NewSingleHostReverseProxy(target), nil
	default:
		return nil, fmt.Errorf("unsupported web asset mode %q", assets.Mode)
	}
}

func (s *Server) providerLabel() string {
	agent := s.currentAgent()
	if s == nil || agent == nil {
		return "provider"
	}
	baseURL := agent.Contracts.ProviderRequest.Transport.Endpoint.Params.BaseURL
	if parsed, err := url.Parse(baseURL); err == nil && parsed.Host != "" {
		return parsed.Host
	}
	if agent.Contracts.ProviderRequest.Transport.ID != "" {
		return agent.Contracts.ProviderRequest.Transport.ID
	}
	return "provider"
}

func (s *Server) publishDaemon(envelope WebsocketEnvelope) {
	if s == nil || s.daemonBus == nil {
		return
	}
	envelope.GeneratedAt = s.currentAgent().Now().UTC()
	s.daemonBus.Publish(envelope)
}

func (s *Server) currentAgent() *runtime.Agent {
	s.agentMu.RLock()
	defer s.agentMu.RUnlock()
	return s.agent
}

func (s *Server) swapAgent(agent *runtime.Agent) {
	s.agentMu.Lock()
	defer s.agentMu.Unlock()
	s.agent = agent
}
