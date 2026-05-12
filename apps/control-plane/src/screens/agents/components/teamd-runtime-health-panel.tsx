import { useQuery } from '@tanstack/react-query'

type TeamdStatusResponse = {
  ok: boolean
  agentdUrl: string
  status?: {
    ok?: boolean
    version?: string
    commit?: string
    tree_state?: string
    build_id?: string
    data_dir?: string
    database?: string
    telegram_mode?: string
    event_bus_backend?: string
    event_bus_required?: boolean
    event_bus_nats_configured?: boolean
    session_count?: number
    mission_count?: number
    run_count?: number
    job_count?: number
  }
  snapshot?: {
    generated_at?: number
    agents?: Array<Record<string, unknown>>
    sessions?: Array<Record<string, unknown>>
    recent_runs?: Array<Record<string, unknown>>
    recent_tool_calls?: Array<{
      status?: string
      error?: string | null
      tool_name?: string
      summary?: string
    }>
    delivery_targets?: Array<Record<string, unknown>>
    telegram_chats?: Array<Record<string, unknown>>
    recent_traces?: Array<Record<string, unknown>>
  }
  errors: Array<string>
}

function formatCount(value: number | undefined): string {
  if (typeof value !== 'number') return '0'
  return value.toLocaleString()
}

function shortCommit(value: string | undefined): string {
  if (!value) return 'unknown'
  return value.length > 12 ? value.slice(0, 12) : value
}

function boolLabel(value: boolean | undefined): string {
  if (value === true) return 'yes'
  if (value === false) return 'no'
  return 'unknown'
}

function timestampLabel(value: number | undefined): string {
  if (!value) return 'not loaded'
  return new Date(value * 1000).toLocaleString()
}

function RuntimeMetric({
  label,
  value,
  hint,
}: {
  label: string
  value: string
  hint?: string
}) {
  return (
    <div className="rounded-2xl border border-[var(--theme-border)] bg-[var(--theme-bg)] px-4 py-3">
      <div className="text-[10px] font-semibold uppercase tracking-[0.16em] text-[var(--theme-muted)]">
        {label}
      </div>
      <div className="mt-1 text-2xl font-semibold tabular-nums text-[var(--theme-text)]">
        {value}
      </div>
      {hint && <div className="mt-1 truncate text-xs text-[var(--theme-muted-2)]">{hint}</div>}
    </div>
  )
}

export function TeamdRuntimeHealthPanel() {
  const query = useQuery({
    queryKey: ['teamd-runtime-status'],
    queryFn: async () => {
      const response = await fetch('/api/teamd-status')
      const payload = (await response.json()) as TeamdStatusResponse
      if (!response.ok && !payload.errors?.length) {
        throw new Error(`teamD status request failed (${response.status})`)
      }
      return payload
    },
    refetchInterval: 15_000,
  })

  const data = query.data
  const status = data?.status
  const snapshot = data?.snapshot
  const failedTools =
    snapshot?.recent_tool_calls?.filter((call) => call.status === 'failed' || call.error).length ??
    0
  const runtimeOk = Boolean(data?.ok)

  return (
    <section className="rounded-3xl border border-[var(--theme-border)] bg-[var(--theme-card)] p-5 shadow-[0_24px_80px_var(--theme-shadow)]">
      <div className="flex flex-col gap-4 lg:flex-row lg:items-start lg:justify-between">
        <div>
          <div className="text-[10px] font-semibold uppercase tracking-[0.18em] text-[var(--theme-accent)]">
            Adapted teamD runtime
          </div>
          <h2 className="mt-1 text-xl font-semibold text-[var(--theme-text)]">
            agentd control-plane boundary
          </h2>
          <p className="mt-2 max-w-3xl text-sm text-[var(--theme-muted-2)]">
            This panel is backed by <code>/api/teamd-status</code>, which proxies
            <code> agentd</code>. Other imported Hermes surfaces are still being adapted module by
            module.
          </p>
        </div>
        <div
          className={[
            'rounded-full border px-3 py-1 text-xs font-semibold uppercase tracking-[0.14em]',
            runtimeOk
              ? 'border-emerald-500/30 bg-emerald-500/10 text-emerald-700'
              : 'border-[var(--theme-danger-border)] bg-[var(--theme-danger-soft)] text-[var(--theme-danger)]',
          ].join(' ')}
        >
          {query.isPending ? 'loading' : runtimeOk ? 'connected' : 'disconnected'}
        </div>
      </div>

      <div className="mt-5 grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
        <RuntimeMetric
          label="Agents"
          value={formatCount(snapshot?.agents?.length)}
          hint={`profiles from snapshot`}
        />
        <RuntimeMetric
          label="Sessions"
          value={formatCount(status?.session_count ?? snapshot?.sessions?.length)}
          hint={`${formatCount(snapshot?.sessions?.length)} loaded in snapshot`}
        />
        <RuntimeMetric
          label="Runs"
          value={formatCount(status?.run_count)}
          hint={`${formatCount(snapshot?.recent_runs?.length)} recent`}
        />
        <RuntimeMetric
          label="Tool errors"
          value={formatCount(failedTools)}
          hint={`${formatCount(snapshot?.recent_tool_calls?.length)} recent tool calls`}
        />
      </div>

      <div className="mt-4 grid gap-3 lg:grid-cols-3">
        <div className="rounded-2xl border border-[var(--theme-border)] bg-[var(--theme-bg)] p-4">
          <div className="text-xs font-semibold uppercase tracking-[0.14em] text-[var(--theme-muted)]">
            Daemon
          </div>
          <dl className="mt-3 space-y-2 text-sm">
            <div className="flex justify-between gap-4">
              <dt className="text-[var(--theme-muted-2)]">URL</dt>
              <dd className="truncate text-[var(--theme-text)]">{data?.agentdUrl ?? 'unknown'}</dd>
            </div>
            <div className="flex justify-between gap-4">
              <dt className="text-[var(--theme-muted-2)]">Version</dt>
              <dd className="text-[var(--theme-text)]">{status?.version ?? 'unknown'}</dd>
            </div>
            <div className="flex justify-between gap-4">
              <dt className="text-[var(--theme-muted-2)]">Commit</dt>
              <dd className="text-[var(--theme-text)]">{shortCommit(status?.commit)}</dd>
            </div>
          </dl>
        </div>

        <div className="rounded-2xl border border-[var(--theme-border)] bg-[var(--theme-bg)] p-4">
          <div className="text-xs font-semibold uppercase tracking-[0.14em] text-[var(--theme-muted)]">
            Backing services
          </div>
          <dl className="mt-3 space-y-2 text-sm">
            <div className="flex justify-between gap-4">
              <dt className="text-[var(--theme-muted-2)]">Database</dt>
              <dd className="truncate text-[var(--theme-text)]">{status?.database ?? 'unknown'}</dd>
            </div>
            <div className="flex justify-between gap-4">
              <dt className="text-[var(--theme-muted-2)]">Event bus</dt>
              <dd className="text-[var(--theme-text)]">{status?.event_bus_backend ?? 'unknown'}</dd>
            </div>
            <div className="flex justify-between gap-4">
              <dt className="text-[var(--theme-muted-2)]">NATS configured</dt>
              <dd className="text-[var(--theme-text)]">
                {boolLabel(status?.event_bus_nats_configured)}
              </dd>
            </div>
          </dl>
        </div>

        <div className="rounded-2xl border border-[var(--theme-border)] bg-[var(--theme-bg)] p-4">
          <div className="text-xs font-semibold uppercase tracking-[0.14em] text-[var(--theme-muted)]">
            Delivery
          </div>
          <dl className="mt-3 space-y-2 text-sm">
            <div className="flex justify-between gap-4">
              <dt className="text-[var(--theme-muted-2)]">Telegram</dt>
              <dd className="text-[var(--theme-text)]">{status?.telegram_mode ?? 'unknown'}</dd>
            </div>
            <div className="flex justify-between gap-4">
              <dt className="text-[var(--theme-muted-2)]">Targets</dt>
              <dd className="text-[var(--theme-text)]">
                {formatCount(snapshot?.delivery_targets?.length)}
              </dd>
            </div>
            <div className="flex justify-between gap-4">
              <dt className="text-[var(--theme-muted-2)]">Snapshot</dt>
              <dd className="text-[var(--theme-text)]">
                {timestampLabel(snapshot?.generated_at)}
              </dd>
            </div>
          </dl>
        </div>
      </div>

      {data?.errors?.length ? (
        <div className="mt-4 rounded-2xl border border-[var(--theme-danger-border)] bg-[var(--theme-danger-soft)] p-4 text-sm text-[var(--theme-text)]">
          <div className="font-semibold text-[var(--theme-danger)]">agentd errors</div>
          <ul className="mt-2 space-y-1">
            {data.errors.map((error) => (
              <li key={error} className="break-all">
                {error}
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </section>
  )
}
