export type AgentdFetchResult<T> =
  | {
      ok: true
      status: number
      url: string
      data: T
    }
  | {
      ok: false
      status: number | null
      url: string
      error: string
    }

export type TeamdRuntimeStatus = {
  ok: boolean
  version?: string
  commit?: string
  tree_state?: string
  build_id?: string
  bind_host?: string
  bind_port?: number
  permission_mode?: string
  session_count?: number
  mission_count?: number
  run_count?: number
  job_count?: number
  components?: number
  data_dir?: string
  database?: string
  telegram_mode?: string
  event_bus_required?: boolean
  event_bus_backend?: string
  event_bus_nats_configured?: boolean
}

export type TeamdWebSnapshot = {
  generated_at: number
  status: Record<string, unknown>
  event_bus: Record<string, unknown>
  agents: Array<Record<string, unknown>>
  sessions: Array<Record<string, unknown>>
  recent_runs: Array<Record<string, unknown>>
  recent_tool_calls: Array<Record<string, unknown>>
  delivery_targets: Array<Record<string, unknown>>
  telegram_chats: Array<Record<string, unknown>>
  recent_traces: Array<Record<string, unknown>>
}

function normalizeBaseUrl(input: string | undefined): string {
  const raw = input?.trim() || 'http://127.0.0.1:5140'
  return raw.replace(/\/+$/, '')
}

export function getAgentdBaseUrl(): string {
  return normalizeBaseUrl(process.env.TEAMD_AGENTD_BASE_URL)
}

function buildAgentdUrl(path: string): string {
  const normalizedPath = path.startsWith('/') ? path : `/${path}`
  return `${getAgentdBaseUrl()}${normalizedPath}`
}

function agentdHeaders(): HeadersInit {
  const token = process.env.TEAMD_AGENTD_TOKEN?.trim()
  return token ? { Authorization: `Bearer ${token}` } : {}
}

export async function fetchAgentdJson<T>(
  path: string,
  options?: { timeoutMs?: number },
): Promise<AgentdFetchResult<T>> {
  const url = buildAgentdUrl(path)
  try {
    const response = await fetch(url, {
      headers: agentdHeaders(),
      signal: AbortSignal.timeout(options?.timeoutMs ?? 3_000),
    })
    const text = await response.text()
    if (!response.ok) {
      return {
        ok: false,
        status: response.status,
        url,
        error: text || `agentd request failed with HTTP ${response.status}`,
      }
    }
    const data = text ? (JSON.parse(text) as T) : ({} as T)
    return { ok: true, status: response.status, url, data }
  } catch (error) {
    return {
      ok: false,
      status: null,
      url,
      error: error instanceof Error ? error.message : 'agentd request failed',
    }
  }
}

export function summarizeAgentdFailure(result: AgentdFetchResult<unknown>): string | undefined {
  return result.ok ? undefined : `${result.url}: ${result.error}`
}

