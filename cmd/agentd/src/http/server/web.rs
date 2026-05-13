use super::*;
use crate::about::{APP_BUILD_ID, APP_COMMIT, APP_TREE_STATE, APP_VERSION};
use crate::http::types::{
    SessionSummaryResponse, WebAgentResponse, WebDeliveryTargetResponse, WebEventBusResponse,
    WebRunResponse, WebRuntimeStatusResponse, WebSnapshotResponse, WebTelegramChatResponse,
    WebToolCallResponse, WebTraceResponse,
};
use crate::redaction::{redact_sensitive_option, redact_sensitive_text};
use agent_persistence::{
    AgentRepository, DeliveryRepository, RunRepository, TelegramRepository, ToolCallRepository,
    TraceRepository,
};
use std::time::{SystemTime, UNIX_EPOCH};

const WEB_SESSION_LIMIT: usize = 25;
const WEB_RUN_SESSION_LIMIT: usize = 3;
const WEB_RUN_LIMIT: usize = 30;
const WEB_TOOL_CALL_SESSION_LIMIT: usize = 60;
const WEB_TOOL_CALL_LIMIT: usize = 120;
const WEB_TRACE_LIMIT: usize = 30;

pub(super) fn is_web_console_request(request: &Request) -> bool {
    matches!(request.url().split('?').next(), Some("/web" | "/web/"))
}

pub(super) fn handle_web_console(request: Request) -> std::io::Result<()> {
    respond_html(request, StatusCode(200), WEB_CONSOLE_HTML)
}

pub(super) fn handle_web_snapshot(app: &App, request: Request) -> std::io::Result<()> {
    match build_web_snapshot(app) {
        Ok(snapshot) => respond_json(request, StatusCode(200), &snapshot),
        Err(error) => {
            let (status, payload) = map_bootstrap_error(error);
            respond_json(request, status, &payload)
        }
    }
}

fn build_web_snapshot(app: &App) -> Result<WebSnapshotResponse, BootstrapError> {
    let store = app.store()?;
    let status = app.runtime_status_snapshot()?;

    let mut sessions = app
        .list_session_summaries()?
        .into_iter()
        .map(SessionSummaryResponse::from)
        .collect::<Vec<_>>();
    sessions.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| right.created_at.cmp(&left.created_at))
            .then_with(|| right.id.cmp(&left.id))
    });
    sessions.truncate(WEB_SESSION_LIMIT);

    let mut recent_runs = Vec::new();
    let mut recent_tool_calls = Vec::new();
    for session in &sessions {
        recent_runs.extend(
            store
                .list_recent_runs_for_session(&session.id, WEB_RUN_SESSION_LIMIT)?
                .into_iter()
                .map(WebRunResponse::from),
        );
        recent_tool_calls.extend(
            store
                .list_recent_tool_calls_for_session(&session.id, WEB_TOOL_CALL_SESSION_LIMIT)?
                .into_iter()
                .map(WebToolCallResponse::from),
        );
    }
    recent_runs.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| right.started_at.cmp(&left.started_at))
            .then_with(|| right.id.cmp(&left.id))
    });
    recent_runs.truncate(WEB_RUN_LIMIT);
    recent_tool_calls.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| right.requested_at.cmp(&left.requested_at))
            .then_with(|| right.id.cmp(&left.id))
    });
    recent_tool_calls.truncate(WEB_TOOL_CALL_LIMIT);

    let mut agents = store
        .list_agent_profiles()?
        .into_iter()
        .map(WebAgentResponse::from)
        .collect::<Vec<_>>();
    agents.sort_by(|left, right| left.id.cmp(&right.id));

    let mut delivery_targets = store
        .list_delivery_targets()?
        .into_iter()
        .map(WebDeliveryTargetResponse::from)
        .collect::<Vec<_>>();
    delivery_targets.sort_by(|left, right| left.target_id.cmp(&right.target_id));

    let mut telegram_chats = store
        .list_telegram_chat_bindings()?
        .into_iter()
        .map(WebTelegramChatResponse::from)
        .collect::<Vec<_>>();
    telegram_chats.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| left.telegram_chat_id.cmp(&right.telegram_chat_id))
    });

    let recent_traces = store
        .list_recent_trace_links(WEB_TRACE_LIMIT)?
        .into_iter()
        .map(WebTraceResponse::from)
        .collect::<Vec<_>>();

    Ok(WebSnapshotResponse {
        generated_at: unix_timestamp()?,
        status: WebRuntimeStatusResponse {
            ok: true,
            version: Some(APP_VERSION.to_string()),
            commit: Some(APP_COMMIT.to_string()),
            tree_state: Some(APP_TREE_STATE.to_string()),
            build_id: Some(APP_BUILD_ID.to_string()),
            data_dir: status.data_dir,
            database: status.database,
            permission_mode: status.permission_mode,
            session_count: status.session_count,
            mission_count: status.mission_count,
            run_count: status.run_count,
            job_count: status.job_count,
        },
        event_bus: WebEventBusResponse {
            backend: app.config.event_bus.backend.clone(),
            required: app.config.event_bus.required,
            nats_configured: app.config.event_bus.nats_url.is_some(),
            input_stream: app.config.event_bus.input_stream.clone(),
            session_stream: app.config.event_bus.session_stream.clone(),
            delivery_stream: app.config.event_bus.delivery_stream.clone(),
            task_stream: app.config.event_bus.task_stream.clone(),
            dlq_stream: app.config.event_bus.dlq_stream.clone(),
        },
        agents,
        sessions,
        recent_runs,
        recent_tool_calls,
        delivery_targets,
        telegram_chats,
        recent_traces,
    })
}

fn unix_timestamp() -> Result<i64, BootstrapError> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(BootstrapError::Clock)?
        .as_secs() as i64)
}

fn respond_html(request: Request, status: StatusCode, body: &str) -> std::io::Result<()> {
    let mut response = Response::from_string(body.to_string()).with_status_code(status);
    response.add_header(
        Header::from_bytes("Content-Type", "text/html; charset=utf-8")
            .map_err(|_| std::io::Error::other("invalid content type header"))?,
    );
    response.add_header(
        Header::from_bytes("Cache-Control", "no-store")
            .map_err(|_| std::io::Error::other("invalid cache control header"))?,
    );
    request.respond(response)
}

impl From<agent_persistence::AgentProfileRecord> for WebAgentResponse {
    fn from(value: agent_persistence::AgentProfileRecord) -> Self {
        Self {
            id: value.id,
            name: value.name,
            template_kind: value.template_kind,
            default_workspace_root: value.default_workspace_root,
            updated_at: value.updated_at,
        }
    }
}

impl From<agent_persistence::RunRecord> for WebRunResponse {
    fn from(value: agent_persistence::RunRecord) -> Self {
        Self {
            id: value.id,
            session_id: value.session_id,
            status: value.status,
            error: value.error,
            started_at: value.started_at,
            updated_at: value.updated_at,
            finished_at: value.finished_at,
        }
    }
}

impl From<agent_persistence::ToolCallRecord> for WebToolCallResponse {
    fn from(value: agent_persistence::ToolCallRecord) -> Self {
        Self {
            id: value.id,
            session_id: value.session_id,
            run_id: value.run_id,
            tool_name: value.tool_name,
            status: value.status,
            summary: redact_sensitive_text(value.summary.as_str()),
            error: redact_sensitive_option(value.error),
            result_summary: redact_sensitive_option(value.result_summary),
            result_artifact_id: value.result_artifact_id,
            requested_at: value.requested_at,
            updated_at: value.updated_at,
        }
    }
}

impl From<agent_persistence::DeliveryTargetRecord> for WebDeliveryTargetResponse {
    fn from(value: agent_persistence::DeliveryTargetRecord) -> Self {
        Self {
            target_id: value.target_id,
            kind: value.kind,
            scope: value.scope,
            format_policy: value.format_policy,
            updated_at: value.updated_at,
        }
    }
}

impl From<agent_persistence::TelegramChatBindingRecord> for WebTelegramChatResponse {
    fn from(value: agent_persistence::TelegramChatBindingRecord) -> Self {
        Self {
            telegram_chat_id: value.telegram_chat_id,
            scope: value.scope,
            selected_session_id: value.selected_session_id,
            default_agent_profile_id: value.default_agent_profile_id,
            inbound_queue_mode: value.inbound_queue_mode,
            inbound_coalesce_window_ms: value.inbound_coalesce_window_ms,
            updated_at: value.updated_at,
        }
    }
}

impl From<agent_persistence::TraceLinkRecord> for WebTraceResponse {
    fn from(value: agent_persistence::TraceLinkRecord) -> Self {
        Self {
            trace_id: value.trace_id,
            span_id: value.span_id,
            entity_kind: value.entity_kind,
            entity_id: value.entity_id,
            surface: value.surface,
            entrypoint: value.entrypoint,
            created_at: value.created_at,
        }
    }
}

const WEB_CONSOLE_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>teamD Read-only Console</title>
  <style>
    :root {
      color-scheme: dark;
      --bg: #090d12;
      --panel: #101720;
      --panel-2: #151f2c;
      --line: #263445;
      --text: #e9f0f7;
      --muted: #8ea0b4;
      --accent: #4cc38a;
      --warn: #e6b45c;
      --bad: #ef7777;
      --good: #7ddc9a;
    }
    * { box-sizing: border-box; }
    body {
      margin: 0;
      background:
        radial-gradient(circle at 10% 0%, rgba(76,195,138,.14), transparent 28rem),
        linear-gradient(135deg, #090d12 0%, #0d141d 52%, #080b10 100%);
      color: var(--text);
      font-family: ui-sans-serif, "Segoe UI", "Helvetica Neue", Arial, sans-serif;
      min-height: 100vh;
    }
    header {
      padding: 28px 32px 20px;
      border-bottom: 1px solid var(--line);
      display: flex;
      gap: 24px;
      justify-content: space-between;
      align-items: flex-end;
    }
    h1 { margin: 0; font-size: clamp(28px, 4vw, 54px); letter-spacing: -.05em; }
    .subtitle { color: var(--muted); margin: 8px 0 0; max-width: 760px; line-height: 1.5; }
    .auth { display: flex; gap: 8px; align-items: center; }
    input {
      background: #0b1118;
      border: 1px solid var(--line);
      color: var(--text);
      border-radius: 10px;
      padding: 10px 12px;
      min-width: 260px;
    }
    button {
      background: var(--accent);
      border: 0;
      color: #04100a;
      border-radius: 10px;
      padding: 10px 14px;
      font-weight: 700;
      cursor: pointer;
    }
    main { padding: 24px 32px 40px; display: grid; gap: 18px; }
    .stats { display: grid; grid-template-columns: repeat(6, minmax(120px, 1fr)); gap: 12px; }
    .stat, section {
      border: 1px solid var(--line);
      background: rgba(16,23,32,.88);
      border-radius: 18px;
      box-shadow: 0 18px 60px rgba(0,0,0,.25);
    }
    .stat { padding: 14px; }
    .stat .label { color: var(--muted); font-size: 12px; text-transform: uppercase; letter-spacing: .08em; }
    .stat .value { font-size: 28px; font-weight: 800; margin-top: 6px; }
    .grid { display: grid; grid-template-columns: minmax(0, 1.35fr) minmax(360px, .65fr); gap: 18px; }
    section { overflow: hidden; }
    section h2 {
      margin: 0;
      padding: 16px 18px;
      font-size: 16px;
      border-bottom: 1px solid var(--line);
      display: flex;
      justify-content: space-between;
      gap: 12px;
    }
    .table { width: 100%; border-collapse: collapse; }
    .table th, .table td {
      padding: 11px 14px;
      border-bottom: 1px solid rgba(38,52,69,.75);
      vertical-align: top;
      text-align: left;
      font-size: 13px;
    }
    .table th { color: var(--muted); font-weight: 600; background: rgba(255,255,255,.02); }
    code { color: #b9d7ff; font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; }
    .muted { color: var(--muted); }
    .pill { display: inline-block; padding: 3px 8px; border-radius: 999px; background: #1f2b3b; color: var(--muted); font-size: 12px; }
    .pill.good { color: var(--good); background: rgba(125,220,154,.12); }
    .pill.bad { color: var(--bad); background: rgba(239,119,119,.12); }
    .pill.warn { color: var(--warn); background: rgba(230,180,92,.12); }
    .stack { display: grid; gap: 18px; }
    .empty, .error { padding: 18px; color: var(--muted); }
    .error { color: var(--bad); }
    @media (max-width: 1100px) {
      header { align-items: stretch; flex-direction: column; }
      .grid, .stats { grid-template-columns: 1fr; }
      .auth { align-items: stretch; flex-direction: column; }
      input { min-width: 0; width: 100%; }
    }
  </style>
</head>
<body>
  <header>
    <div>
      <h1>teamD Read-only Console</h1>
      <p class="subtitle">Live snapshot over existing runtime data. No mutations, no control actions: sessions, agents, event bus, recent runs, tool calls, Telegram bindings, delivery targets, traces.</p>
    </div>
    <div class="auth">
      <input id="token" type="password" placeholder="Bearer token, if configured">
      <button id="refresh">Refresh</button>
    </div>
  </header>
  <main>
    <div id="notice" class="muted">Loading snapshot...</div>
    <div id="stats" class="stats"></div>
    <div class="grid">
      <section>
        <h2>Recent Sessions <span id="sessions-count" class="muted"></span></h2>
        <div id="sessions"></div>
      </section>
      <div class="stack">
        <section>
          <h2>Runtime</h2>
          <div id="runtime"></div>
        </section>
        <section>
          <h2>Agents</h2>
          <div id="agents"></div>
        </section>
      </div>
    </div>
    <div class="grid">
      <section>
        <h2>Recent Tool Calls</h2>
        <div id="tools"></div>
      </section>
      <section>
        <h2>Recent Runs</h2>
        <div id="runs"></div>
      </section>
    </div>
    <div class="grid">
      <section>
        <h2>Telegram Chats</h2>
        <div id="telegram"></div>
      </section>
      <section>
        <h2>Delivery Targets & Traces</h2>
        <div id="delivery"></div>
      </section>
    </div>
  </main>
  <script>
    const tokenInput = document.getElementById('token');
    tokenInput.value = localStorage.getItem('teamd.web.token') || '';
    document.getElementById('refresh').addEventListener('click', load);
    tokenInput.addEventListener('change', () => localStorage.setItem('teamd.web.token', tokenInput.value));

    function escapeHtml(value) {
      return String(value ?? '').replace(/[&<>"']/g, char => ({
        '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'
      }[char]));
    }
    function fmt(ts) {
      if (!ts) return 'n/a';
      return new Date(ts * 1000).toLocaleString();
    }
    function pill(status) {
      const value = String(status || 'unknown');
      const klass = /failed|error|cancel|blocked/.test(value) ? 'bad' : /running|queued|waiting|pending/.test(value) ? 'warn' : 'good';
      return `<span class="pill ${klass}">${escapeHtml(value)}</span>`;
    }
    function table(headers, rows) {
      if (!rows.length) return '<div class="empty">No data.</div>';
      return `<table class="table"><thead><tr>${headers.map(h => `<th>${escapeHtml(h)}</th>`).join('')}</tr></thead><tbody>${rows.join('')}</tbody></table>`;
    }
    function cell(value) {
      return `<td>${value}</td>`;
    }
    async function load() {
      const notice = document.getElementById('notice');
      notice.className = 'muted';
      notice.textContent = 'Loading snapshot...';
      const headers = {};
      const token = tokenInput.value.trim();
      if (token) headers.Authorization = `Bearer ${token}`;
      try {
        const response = await fetch('/v1/web/snapshot', { headers });
        if (!response.ok) throw new Error(`${response.status} ${await response.text()}`);
        const data = await response.json();
        localStorage.setItem('teamd.web.token', token);
        render(data);
        notice.textContent = `Snapshot: ${fmt(data.generated_at)}`;
      } catch (error) {
        notice.className = 'error';
        notice.textContent = `Snapshot failed: ${error.message}`;
      }
    }
    function render(data) {
      document.getElementById('stats').innerHTML = [
        ['Sessions', data.status.session_count],
        ['Runs', data.status.run_count],
        ['Jobs', data.status.job_count],
        ['Agents', data.agents.length],
        ['Telegram chats', data.telegram_chats.length],
        ['Tool calls', data.recent_tool_calls.length]
      ].map(([label, value]) => `<div class="stat"><div class="label">${label}</div><div class="value">${value}</div></div>`).join('');
      document.getElementById('runtime').innerHTML = table(['Field', 'Value'], [
        ['Version', `${escapeHtml(data.status.version)} <span class="muted">${escapeHtml(data.status.commit)}</span>`],
        ['Database', `<code>${escapeHtml(data.status.database)}</code>`],
        ['Data dir', `<code>${escapeHtml(data.status.data_dir)}</code>`],
        ['Event bus', `${escapeHtml(data.event_bus.backend)} &middot; required=${data.event_bus.required} &middot; nats=${data.event_bus.nats_configured}`],
        ['Streams', `<code>${escapeHtml([data.event_bus.input_stream, data.event_bus.session_stream, data.event_bus.delivery_stream, data.event_bus.task_stream, data.event_bus.dlq_stream].join(' / '))}</code>`]
      ].map(([a, b]) => `<tr>${cell(escapeHtml(a))}${cell(b)}</tr>`));
      document.getElementById('sessions-count').textContent = `${data.sessions.length} shown`;
      document.getElementById('sessions').innerHTML = table(['Updated', 'Session', 'Agent', 'State', 'Usage'], data.sessions.map(s =>
        `<tr>${cell(fmt(s.updated_at))}${cell(`<code>${escapeHtml(s.id)}</code><br>${escapeHtml(s.title)}<br><span class="muted">${escapeHtml(s.message_count)} messages</span>`)}${cell(`${escapeHtml(s.agent_name)}<br><span class="muted">${escapeHtml(s.agent_profile_id)}</span>`)}${cell(`${s.has_pending_approval ? pill('pending approval') : ''} ${s.background_job_count ? pill(`${s.running_background_job_count} running / ${s.queued_background_job_count} queued`) : pill('idle')}`)}${cell(`${escapeHtml(s.usage_total_tokens ?? s.context_tokens)} tokens<br><span class="muted">${escapeHtml(s.model || '')}</span>`)}</tr>`
      ));
      document.getElementById('agents').innerHTML = table(['Agent', 'Template', 'Workspace'], data.agents.map(a =>
        `<tr>${cell(`<code>${escapeHtml(a.id)}</code><br>${escapeHtml(a.name)}`)}${cell(escapeHtml(a.template_kind))}${cell(`<code>${escapeHtml(a.default_workspace_root || 'default')}</code>`)}</tr>`
      ));
      document.getElementById('tools').innerHTML = table(['Updated', 'Tool', 'Session', 'Result'], data.recent_tool_calls.map(t =>
        `<tr>${cell(fmt(t.updated_at))}${cell(`${pill(t.status)}<br><strong>${escapeHtml(t.tool_name)}</strong><br><span class="muted">${escapeHtml(t.summary)}</span>`)}${cell(`<code>${escapeHtml(t.session_id)}</code><br><span class="muted">${escapeHtml(t.run_id)}</span>`)}${cell(`${escapeHtml(t.result_summary || '')}${t.error ? `<br><span class="error">${escapeHtml(t.error)}</span>` : ''}`)}</tr>`
      ));
      document.getElementById('runs').innerHTML = table(['Updated', 'Run', 'Status'], data.recent_runs.map(r =>
        `<tr>${cell(fmt(r.updated_at))}${cell(`<code>${escapeHtml(r.id)}</code><br><span class="muted">${escapeHtml(r.session_id)}</span>`)}${cell(`${pill(r.status)}${r.error ? `<br><span class="error">${escapeHtml(r.error)}</span>` : ''}`)}</tr>`
      ));
      document.getElementById('telegram').innerHTML = table(['Updated', 'Chat', 'Binding'], data.telegram_chats.map(c =>
        `<tr>${cell(fmt(c.updated_at))}${cell(`<code>${escapeHtml(c.telegram_chat_id)}</code><br>${escapeHtml(c.scope)}`)}${cell(`session=<code>${escapeHtml(c.selected_session_id || 'none')}</code><br>agent=<code>${escapeHtml(c.default_agent_profile_id || 'default')}</code><br><span class="muted">${escapeHtml(c.inbound_queue_mode)} / ${escapeHtml(c.inbound_coalesce_window_ms ?? 'default')}ms</span>`)}</tr>`
      ));
      const deliveryRows = data.delivery_targets.map(d =>
        `<tr>${cell(`<code>${escapeHtml(d.target_id)}</code><br>${escapeHtml(d.kind)}`)}${cell(escapeHtml(d.scope))}${cell(escapeHtml(d.format_policy))}</tr>`
      );
      const traceRows = data.recent_traces.slice(0, 8).map(t =>
        `<tr>${cell(`<code>${escapeHtml(t.trace_id)}</code><br><span class="muted">${escapeHtml(t.span_id)}</span>`)}${cell(`${escapeHtml(t.entity_kind)}<br><span class="muted">${escapeHtml(t.entity_id)}</span>`)}${cell(escapeHtml(t.surface || t.entrypoint || 'n/a'))}</tr>`
      );
      document.getElementById('delivery').innerHTML = `<h3 style="padding:0 14px;margin:14px 0 8px">Delivery targets</h3>${table(['Target', 'Scope', 'Format'], deliveryRows)}<h3 style="padding:0 14px;margin:18px 0 8px">Recent traces</h3>${table(['Trace', 'Entity', 'Surface'], traceRows)}`;
    }
    load();
    setInterval(load, 10000);
  </script>
</body>
</html>"#;
