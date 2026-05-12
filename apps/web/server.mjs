import { createServer } from "node:http";
import { readFile, stat } from "node:fs/promises";
import { extname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL(".", import.meta.url));
const distDir = join(root, "dist");
const port = Number.parseInt(process.env.PORT ?? process.env.TEAMD_WEB_PORT ?? "5173", 10);
const host = process.env.HOST ?? process.env.TEAMD_WEB_HOST ?? "0.0.0.0";
const agentdBase = new URL(process.env.TEAMD_AGENTD_BASE_URL ?? "http://127.0.0.1:5140");
const agentdToken = process.env.TEAMD_AGENTD_TOKEN;
const agentdTimeoutMs = Number.parseInt(process.env.TEAMD_AGENTD_TIMEOUT_MS ?? "120000", 10);

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

createServer(async (req, res) => {
  const url = new URL(req.url ?? "/", `http://${req.headers.host ?? "localhost"}`);
  try {
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
}).listen(port, host, () => {
  console.log(`teamD web console listening on http://${host}:${port}`);
  console.log(`proxying agentd at ${agentdBase.toString()}`);
});
