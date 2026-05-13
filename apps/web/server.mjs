import { createServer } from "node:http";
import { timingSafeEqual } from "node:crypto";
import { readFile, stat } from "node:fs/promises";
import { extname, join, normalize } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = fileURLToPath(new URL(".", import.meta.url));
const distDir = join(root, "dist");
const port = Number.parseInt(process.env.PORT ?? process.env.TEAMD_WEB_PORT ?? "5173", 10);
const host = process.env.HOST ?? process.env.TEAMD_WEB_HOST ?? "0.0.0.0";
const agentdBase = new URL(process.env.TEAMD_AGENTD_BASE_URL ?? "http://127.0.0.1:5140");
const agentdToken = process.env.TEAMD_AGENTD_TOKEN;
const agentdTimeoutMs = Number.parseInt(process.env.TEAMD_AGENTD_TIMEOUT_MS ?? "120000", 10);
const ssePollMs = Number.parseInt(process.env.TEAMD_WEB_SSE_POLL_MS ?? "2000", 10);
const webAuth = buildAuthConfig(process.env);

const mimeByExt = new Map([
  [".html", "text/html; charset=utf-8"],
  [".js", "text/javascript; charset=utf-8"],
  [".css", "text/css; charset=utf-8"],
  [".json", "application/json; charset=utf-8"],
  [".svg", "image/svg+xml"],
  [".png", "image/png"],
  [".jpg", "image/jpeg"],
  [".jpeg", "image/jpeg"],
  [".webp", "image/webp"],
  [".ico", "image/x-icon"]
]);

function readRequestBody(req) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    req.on("data", (chunk) => chunks.push(chunk));
    req.on("end", () => resolve(chunks.length === 0 ? undefined : Buffer.concat(chunks)));
    req.on("error", reject);
  });
}

function sendJson(res, status, payload) {
  const body = JSON.stringify(payload);
  res.writeHead(status, {
    "content-type": "application/json; charset=utf-8",
    "cache-control": "no-store",
    "content-length": Buffer.byteLength(body)
  });
  res.end(body);
}

function buildAuthConfig(env) {
  const username = env.TEAMD_WEB_AUTH_USER ?? env.TEAMD_WEB_USERNAME ?? "";
  const password = env.TEAMD_WEB_AUTH_PASSWORD ?? env.TEAMD_WEB_PASSWORD ?? "";
  return {
    enabled: username.length > 0 && password.length > 0,
    username,
    password,
    realm: env.TEAMD_WEB_AUTH_REALM ?? "teamD Web Console"
  };
}

function safeEqual(left, right) {
  const leftBuffer = Buffer.from(left, "utf8");
  const rightBuffer = Buffer.from(right, "utf8");
  return leftBuffer.length === rightBuffer.length && timingSafeEqual(leftBuffer, rightBuffer);
}

export function isAuthorizedRequest(req, auth = webAuth) {
  if (!auth.enabled) {
    return true;
  }
  const header = req.headers.authorization;
  if (!header || !header.startsWith("Basic ")) {
    return false;
  }

  let decoded;
  try {
    decoded = Buffer.from(header.slice("Basic ".length), "base64").toString("utf8");
  } catch {
    return false;
  }
  const separator = decoded.indexOf(":");
  if (separator < 0) {
    return false;
  }

  const username = decoded.slice(0, separator);
  const password = decoded.slice(separator + 1);
  return safeEqual(username, auth.username) && safeEqual(password, auth.password);
}

function authRealmHeader(auth) {
  const realm = String(auth.realm ?? "teamD Web Console").replace(/["\\\r\n]/g, "");
  return `Basic realm="${realm}", charset="UTF-8"`;
}

function sendUnauthorized(res, auth) {
  const body = "Authentication required\n";
  res.writeHead(401, {
    "www-authenticate": authRealmHeader(auth),
    "content-type": "text/plain; charset=utf-8",
    "cache-control": "no-store",
    "content-length": Buffer.byteLength(body)
  });
  res.end(body);
}

function safeStaticPath(pathname) {
  const decoded = decodeURIComponent(pathname);
  const clean = normalize(decoded).replace(/^(\.\.[/\\])+/, "");
  return join(distDir, clean === "/" ? "index.html" : clean);
}

async function serveStatic(req, res, pathname) {
  const filePath = safeStaticPath(pathname);
  try {
    const info = await stat(filePath);
    if (!info.isFile()) {
      throw new Error("not a file");
    }
    const body = await readFile(filePath);
    res.writeHead(200, {
      "content-type": mimeByExt.get(extname(filePath)) ?? "application/octet-stream",
      "cache-control": pathname.includes("/assets/") ? "public, max-age=31536000, immutable" : "no-store",
      "content-length": body.length
    });
    res.end(body);
  } catch {
    const index = await readFile(join(distDir, "index.html"));
    res.writeHead(200, {
      "content-type": "text/html; charset=utf-8",
      "cache-control": "no-store",
      "content-length": index.length
    });
    res.end(index);
  }
}

function copyResponseHeaders(source, res) {
  const excluded = new Set([
    "connection",
    "content-encoding",
    "content-length",
    "keep-alive",
    "transfer-encoding",
    "upgrade"
  ]);
  source.headers.forEach((value, key) => {
    if (!excluded.has(key.toLowerCase())) {
      res.setHeader(key, value);
    }
  });
}

async function proxyAgentd(req, res, url) {
  const targetPath = url.pathname.replace(/^\/api\/agentd/, "") || "/";
  const target = new URL(`${targetPath}${url.search}`, agentdBase);
  const body = req.method === "GET" || req.method === "HEAD" ? undefined : await readRequestBody(req);
  const headers = {};
  if (req.headers["content-type"]) {
    headers["content-type"] = req.headers["content-type"];
  }
  if (req.headers.accept) {
    headers.accept = req.headers.accept;
  }
  if (agentdToken) {
    headers.authorization = `Bearer ${agentdToken}`;
  }

  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), agentdTimeoutMs);
  try {
    const response = await fetch(target, {
      method: req.method,
      headers,
      body,
      signal: controller.signal
    });
    const responseBody = Buffer.from(await response.arrayBuffer());
    res.statusCode = response.status;
    copyResponseHeaders(response, res);
    res.setHeader("cache-control", "no-store");
    res.setHeader("content-length", responseBody.length);
    res.end(responseBody);
  } catch (error) {
    sendJson(res, 502, {
      error: "agentd proxy failed",
      detail: error instanceof Error ? error.message : String(error)
    });
  } finally {
    clearTimeout(timeout);
  }
}

function agentdHeaders(accept = "application/json") {
  const headers = { accept };
  if (agentdToken) {
    headers.authorization = `Bearer ${agentdToken}`;
  }
  return headers;
}

async function fetchAgentdJson(path) {
  const target = new URL(path, agentdBase);
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), agentdTimeoutMs);
  try {
    const response = await fetch(target, {
      method: "GET",
      headers: agentdHeaders(),
      signal: controller.signal
    });
    const text = await response.text();
    const payload = text ? JSON.parse(text) : null;
    if (!response.ok) {
      throw new Error(payload?.error ?? payload?.detail ?? response.statusText);
    }
    return payload;
  } finally {
    clearTimeout(timeout);
  }
}

function snapshotMarker(snapshot) {
  return JSON.stringify({
    status: {
      sessions: snapshot?.status?.session_count,
      runs: snapshot?.status?.run_count,
      jobs: snapshot?.status?.job_count,
      components: snapshot?.status?.components
    },
    sessions: (snapshot?.sessions ?? []).map((session) => [
      session.id,
      session.updated_at,
      session.message_count,
      session.context_tokens,
      session.has_pending_approval
    ]),
    runs: (snapshot?.recent_runs ?? []).map((run) => [run.id, run.status, run.updated_at, run.finished_at]),
    tools: (snapshot?.recent_tool_calls ?? []).map((tool) => [
      tool.id,
      tool.status,
      tool.updated_at,
      tool.result_summary,
      tool.error
    ]),
    traces: (snapshot?.recent_traces ?? []).map((trace) => [trace.trace_id, trace.span_id, trace.created_at])
  });
}

function writeSse(res, event, data) {
  res.write(`event: ${event}\n`);
  res.write(`data: ${JSON.stringify(data)}\n\n`);
}

async function streamEvents(req, res) {
  res.writeHead(200, {
    "content-type": "text/event-stream; charset=utf-8",
    "cache-control": "no-store, no-transform",
    connection: "keep-alive",
    "x-accel-buffering": "no"
  });
  res.write(": connected\n\n");

  let closed = false;
  let lastMarker = "";
  req.on("close", () => {
    closed = true;
  });

  async function pushSnapshot() {
    if (closed) {
      return;
    }
    try {
      const snapshot = await fetchAgentdJson("/v1/web/snapshot");
      const marker = snapshotMarker(snapshot);
      if (marker !== lastMarker) {
        lastMarker = marker;
        writeSse(res, "snapshot", snapshot);
      } else {
        res.write(": heartbeat\n\n");
      }
    } catch (error) {
      writeSse(res, "error", {
        error: "agentd snapshot stream failed",
        detail: error instanceof Error ? error.message : String(error)
      });
    }
  }

  await pushSnapshot();
  const timer = setInterval(() => {
    void pushSnapshot();
    if (closed) {
      clearInterval(timer);
      res.end();
    }
  }, Math.max(500, ssePollMs));
}

export function createWebServer(auth = webAuth) {
  return createServer(async (req, res) => {
    const url = new URL(req.url ?? "/", `http://${req.headers.host ?? "localhost"}`);
    try {
      if (!isAuthorizedRequest(req, auth)) {
        sendUnauthorized(res, auth);
        return;
      }
      if (url.pathname === "/api/events") {
        await streamEvents(req, res);
        return;
      }
      if (url.pathname.startsWith("/api/agentd/")) {
        await proxyAgentd(req, res, url);
        return;
      }
      await serveStatic(req, res, url.pathname);
    } catch (error) {
      sendJson(res, 500, {
        error: "web console request failed",
        detail: error instanceof Error ? error.message : String(error)
      });
    }
  });
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  createWebServer().listen(port, host, () => {
    console.log(`teamD web console listening on http://${host}:${port}`);
    console.log(`proxying agentd at ${agentdBase.toString()}`);
    console.log(`basic auth ${webAuth.enabled ? "enabled" : "disabled"}`);
  });
}
