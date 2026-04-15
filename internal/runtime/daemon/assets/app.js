const clientConfig = window.__TEAMD_CLIENT_CONFIG__ || {
  endpointPath: "/api",
  websocketPath: "/ws",
};
const summary = document.getElementById("summary");
const listenAddr = document.getElementById("listenAddr");
const transportMode = document.getElementById("transportMode");
const assetsMode = document.getElementById("assetsMode");
const sessions = document.getElementById("sessions");
const events = document.getElementById("events");

function appendEvent(line) {
  events.textContent = `${line}\n${events.textContent}`.trim();
}

fetch(`${clientConfig.endpointPath}/bootstrap`)
  .then((resp) => resp.json())
  .then((data) => {
    summary.textContent = `agent ${data.agent_id} loaded from ${data.config_path}`;
    listenAddr.textContent = `listen: ${data.listen_addr}`;
    transportMode.textContent = `ws: ${data.transport.websocket_path}`;
    assetsMode.textContent = `assets: ${data.assets.mode}`;
    sessions.innerHTML = "";
    for (const session of data.sessions) {
      const li = document.createElement("li");
      li.textContent = `${session.session_id} | messages=${session.message_count}`;
      sessions.appendChild(li);
    }
    if (data.sessions.length === 0) {
      const li = document.createElement("li");
      li.textContent = "No sessions recorded yet.";
      li.className = "muted";
      sessions.appendChild(li);
    }
  })
  .catch((err) => {
    summary.textContent = `bootstrap failed: ${err}`;
  });

const scheme = window.location.protocol === "https:" ? "wss" : "ws";
const socket = new WebSocket(`${scheme}://${window.location.host}${clientConfig.websocketPath}`);
socket.onopen = () => appendEvent("[ok] websocket connected");
socket.onclose = () => appendEvent("[closed] websocket disconnected");
socket.onerror = () => appendEvent("[error] websocket error");
socket.onmessage = (event) => appendEvent(event.data);
