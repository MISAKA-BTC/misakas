// The Network tab: joining the MISAKA network, as a ladder the user can see themselves on.
//
// Three ideas organise it:
//
// * **The class list is the headline.** "What can this machine mine?" is the question that brings
//   people here, and each chain-registered class answers with its share, its artifact requirement,
//   and this machine's readiness — the same fit-first UX the model list has.
// * **Roles are rungs, and each states its real prerequisites.** Observing needs a reachable
//   node. Verifying needs a running node — on this chain, syncing IS verifying. Producing needs a
//   bonded key, and the panel explains the bond instead of pretending a button can conjure one.
// * **Everything a button does is a visible command line.** A person putting a bonded key on the
//   line gets to read the exact flags before anything runs, and can reproduce them without the
//   Studio afterwards.

import { useCallback, useEffect, useRef, useState } from 'react'
import { api } from '../lib/api'
import { bytes, count, shortHash } from '../lib/format'
import type { NetworkOverview, NodeClassRow, PalwClassStatus, Settings } from '../lib/types'
import { useStudio } from '../store/studio'
import { CopyButton, EmptyState, Field, Icon, Section, Spinner } from './common'

export function NetworkView() {
  const [overview, setOverview] = useState<NetworkOverview | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const toast = useStudio((s) => s.toast)
  const settings = useStudio((s) => s.settings)
  const saveSettings = useStudio((s) => s.saveSettings)
  const setDownload = useStudio((s) => s.setDownload)
  const timer = useRef<ReturnType<typeof setInterval> | null>(null)

  const refresh = useCallback(async () => {
    try {
      setOverview(await api.network())
      setError(null)
    } catch (e) {
      setError((e as Error).message)
    }
  }, [])

  // Poll while the tab is open. The node's numbers (DAA score, peers, activity) move on their
  // own; a static snapshot of a chain is stale by definition.
  useEffect(() => {
    void refresh()
    timer.current = setInterval(() => void refresh(), 3000)
    return () => {
      if (timer.current) clearInterval(timer.current)
    }
  }, [refresh])

  const start = async (role: 'observer' | 'verifier' | 'producer') => {
    setBusy(true)
    try {
      await api.startNode(role)
      toast('success', role === 'producer' ? 'Node started — producing' : 'Node started')
      await refresh()
    } catch (e) {
      toast('error', (e as Error).message)
    } finally {
      setBusy(false)
    }
  }

  const stop = async () => {
    setBusy(true)
    try {
      await api.stopNode()
      toast('info', 'Node stopped')
      await refresh()
    } catch (e) {
      toast('error', (e as Error).message)
    } finally {
      setBusy(false)
    }
  }

  const downloadArtifact = async (name: string) => {
    try {
      const progress = await api.downloadClassArtifact(name)
      setDownload(progress)
      toast('info', `Downloading ${progress.file} — verified against the chain-pinned digest when it lands`)
    } catch (e) {
      toast('error', (e as Error).message)
    }
  }

  if (!overview) {
    return (
      <EmptyState icon="globe" title="Reading the network state">
        {error ?? 'One moment…'}
      </EmptyState>
    )
  }

  const node = overview.node
  const supervised = node.status.source === 'supervised'

  return (
    <div className="h-full overflow-y-auto p-4">
      <div className="grid gap-4 xl:grid-cols-3">
        <div className="space-y-4 xl:col-span-2">
          <NodePanel
            overview={overview}
            busy={busy}
            onStart={start}
            onStop={stop}
          />

          <section className="card p-5">
            <div className="flex items-baseline justify-between">
              <h3 className="text-sm font-semibold">Mining classes</h3>
              <span className="text-[0.7rem] text-ink-500 dark:text-ink-400">testnet-11 genesis registry</span>
            </div>
            <p className="mt-1 text-xs text-ink-500 dark:text-ink-400">
              A block on this network is won by verified inference in one of these chain-registered classes. The floor needs
              nothing; the model classes need their converted artifact — and the node refuses any file that does not verify to
              the registered root.
            </p>
            <div className="mt-4 space-y-3">
              {overview.classes.map((cls) => (
                <ClassCard key={cls.spec.name} cls={cls} nodeRows={node.classes_from_node} onDownload={downloadArtifact} />
              ))}
            </div>
          </section>

          {node.activity.length > 0 && (
            <section className="card p-5">
              <h3 className="text-sm font-semibold">Node activity</h3>
              <p className="mt-1 text-xs text-ink-500 dark:text-ink-400">
                The PALW lines from the supervised node's log — production, panel receipts, holds with their reasons.
              </p>
              <pre className="mono mt-3 max-h-72 overflow-auto rounded-lg bg-ink-900 p-3 text-[0.68rem] leading-relaxed text-ink-200 dark:bg-black/50">
                {node.activity.join('\n')}
              </pre>
            </section>
          )}
        </div>

        <div className="space-y-4">
          <RolesPanel role={overview.role} supervised={supervised} />
          {settings && <NodeSettingsPanel settings={settings} save={saveSettings} />}
          {node.command_line && (
            <section className="card p-4">
              <div className="flex items-center justify-between">
                <h3 className="text-sm font-semibold">Exact command line</h3>
                <CopyButton text={node.command_line.join(' ')} label="Copy command" />
              </div>
              <p className="mt-1 text-[0.7rem] text-ink-500 dark:text-ink-400">
                What the Studio ran. Reproducible without it — that is the point.
              </p>
              <pre className="mono mt-2 overflow-x-auto rounded-lg bg-ink-100 p-2 text-[0.65rem] leading-relaxed dark:bg-ink-800">
                {node.command_line.join(' \\\n  ')}
              </pre>
            </section>
          )}
        </div>
      </div>
    </div>
  )
}

function NodePanel({
  overview,
  busy,
  onStart,
  onStop,
}: {
  overview: NetworkOverview
  busy: boolean
  onStart: (role: 'observer' | 'verifier' | 'producer') => void
  onStop: () => void
}) {
  const status = overview.node.status
  const supervised = status.source === 'supervised'

  return (
    <section className="card p-5">
      <div className="flex flex-wrap items-center gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className={`size-2 rounded-full ${status.reachable ? 'bg-emerald-500' : 'bg-ink-400'}`} />
            <h3 className="text-sm font-semibold">
              {status.reachable
                ? `${supervised ? 'Supervised node' : 'Attached node'} · ${status.network ?? '…'}`
                : 'No node'}
            </h3>
            {status.is_synced === true && <span className="badge bg-emerald-100 text-emerald-800 dark:bg-emerald-950 dark:text-emerald-300">synced</span>}
            {status.is_synced === false && <span className="badge bg-amber-100 text-amber-800 dark:bg-amber-950/60 dark:text-amber-300">syncing</span>}
          </div>
          <p className="mt-1 truncate text-[0.7rem] text-ink-500 dark:text-ink-400">
            {status.reachable ? `${status.rpc_url} · ${status.server_version ?? ''}` : (status.error ?? 'not running')}
          </p>
        </div>
        <div className="flex gap-2">
          {supervised ? (
            <button type="button" className="btn-outline" disabled={busy} onClick={onStop} title="Stop the supervised node">
              {busy ? <Spinner className="size-3.5" /> : <Icon name="stop" className="size-3.5" />}
              Stop node
            </button>
          ) : (
            <>
              <button type="button" className="btn-primary" disabled={busy || !overview.kaspad_found} onClick={() => onStart(overview.role)}>
                {busy ? <Spinner className="size-3.5" /> : <Icon name="power" className="size-3.5" />}
                Start node
              </button>
            </>
          )}
        </div>
      </div>

      {!overview.kaspad_found && !status.reachable && (
        <p className="mt-3 flex gap-2 rounded-lg bg-amber-50 p-2 text-xs text-amber-800 dark:bg-amber-950/40 dark:text-amber-300">
          <Icon name="warning" className="mt-0.5 size-4 shrink-0" />
          <span>
            No <span className="mono">kaspad</span> binary found (looked at <span className="mono">{overview.kaspad_path}</span>).
            Build it with <span className="mono">cargo build --release -p kaspad</span> in the misakas repository, or set the path
            below — or attach to a node that is already running.
          </span>
        </p>
      )}

      {status.reachable && (
        <dl className="mt-4 grid grid-cols-2 gap-x-6 gap-y-2 text-xs sm:grid-cols-3">
          <StatRow label="DAA score" value={status.virtual_daa_score?.toLocaleString() ?? '—'} />
          <StatRow label="Blocks" value={status.block_count?.toLocaleString() ?? '—'} />
          <StatRow label="Headers" value={status.header_count?.toLocaleString() ?? '—'} />
          <StatRow label="Peers" value={String(status.peer_count ?? '—')} />
          <StatRow label="Mempool" value={String(status.mempool_size ?? '—')} />
          <StatRow label="Difficulty" value={status.difficulty ? count(status.difficulty) : '—'} />
        </dl>
      )}
      {status.sink && (
        <p className="mono mt-2 truncate text-[0.65rem] text-ink-500 dark:text-ink-400" title={status.sink}>
          sink {shortHash(status.sink, 16, 8)}
        </p>
      )}
    </section>
  )
}

function StatRow({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <dt className="text-[0.65rem] uppercase tracking-wide text-ink-500 dark:text-ink-400">{label}</dt>
      <dd className="mt-0.5 font-medium tabular-nums">{value}</dd>
    </div>
  )
}

function ClassCard({
  cls,
  nodeRows,
  onDownload,
}: {
  cls: PalwClassStatus
  nodeRows: NodeClassRow[]
  onDownload: (name: string) => void
}) {
  const spec = cls.spec
  // The node's own dump line for this class. The floor is matched by its base flag — every
  // ConsensusV2 chain has exactly one, and its id is chain-specific (a locally minted chain's
  // floor differs from live testnet-11's). Everything else matches by id prefix: the node
  // prints the full id, the snapshot may only know a prefix.
  const live = spec.is_base
    ? nodeRows.find((row) => row.base)
    : nodeRows.find((row) => (spec.class_id_hex ? row.class_id.startsWith(spec.class_id_hex.slice(0, 16)) : false))

  const readiness = cls.readiness
  const badge =
    readiness.state === 'ready_built_in' ? (
      <span className="badge bg-emerald-100 text-emerald-800 dark:bg-emerald-950 dark:text-emerald-300">ready — nothing to download</span>
    ) : readiness.state === 'artifact_present' ? (
      <span className="badge bg-emerald-100 text-emerald-800 dark:bg-emerald-950 dark:text-emerald-300">
        artifact present{readiness.verified ? ' · verified' : ''}
      </span>
    ) : readiness.state === 'artifact_mismatch' ? (
      <span className="badge bg-red-100 text-red-800 dark:bg-red-950/60 dark:text-red-300">wrong size on disk</span>
    ) : readiness.downloadable ? (
      <span className="badge bg-amber-100 text-amber-800 dark:bg-amber-950/60 dark:text-amber-300">artifact not downloaded</span>
    ) : (
      <span className="badge bg-amber-100 text-amber-800 dark:bg-amber-950/60 dark:text-amber-300">convert locally</span>
    )

  return (
    <div className="rounded-xl border border-ink-200 p-4 dark:border-ink-800">
      <div className="flex flex-wrap items-center gap-2">
        <h4 className="mono text-sm font-semibold">{spec.name}</h4>
        <span className="badge bg-arc-500/15 text-arc-700 dark:text-arc-300">{spec.share_permille}‰ share</span>
        {spec.is_base && <span className="badge bg-ink-100 text-ink-600 dark:bg-ink-800 dark:text-ink-300">floor · always producible</span>}
        {badge}
        {live && (
          <span className="badge bg-emerald-100 text-emerald-800 dark:bg-emerald-950 dark:text-emerald-300" title="From the connected node's own class table">
            on chain: {live.status}
            {live.share_permille !== null && ` · ${live.share_permille}‰`}
          </span>
        )}
      </div>

      <p className="mt-2 text-xs leading-relaxed text-ink-600 dark:text-ink-300">{spec.description}</p>

      <div className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-[0.7rem] text-ink-500 dark:text-ink-400">
        {spec.artifact.kind === 'download' && (
          <>
            <span className="mono">{spec.artifact.filename}</span>
            <span>{bytes(spec.artifact.size_bytes)}</span>
            <span className="mono" title="SHA-256 the download is verified against">
              sha256 {spec.artifact.sha256.slice(0, 12)}…
            </span>
          </>
        )}
        {spec.artifact.kind === 'convert_locally' && (
          <>
            <span>~{bytes(spec.artifact.approx_size_bytes)}</span>
            <span>
              from <span className="mono">{spec.artifact.source_repo}</span>
            </span>
          </>
        )}
        {spec.class_id_hex && (
          <span className="mono" title={spec.class_id_complete ? 'class id' : 'class id (documented prefix; the node prints it in full)'}>
            class {spec.class_id_hex.slice(0, 12)}…{spec.class_id_complete ? '' : ' (prefix)'}
          </span>
        )}
      </div>

      {cls.memory_note && (
        <p className="mt-2 flex gap-2 rounded-lg bg-amber-50 p-2 text-[0.7rem] text-amber-800 dark:bg-amber-950/40 dark:text-amber-300">
          <Icon name="warning" className="mt-0.5 size-3.5 shrink-0" />
          {cls.memory_note}
        </p>
      )}

      <div className="mt-3 flex flex-wrap gap-2">
        {readiness.state === 'artifact_missing' && readiness.downloadable && !cls.memory_note && (
          <button type="button" className="btn-outline" onClick={() => onDownload(spec.name)}>
            <Icon name="download" className="size-3.5" />
            Download artifact
          </button>
        )}
        {spec.artifact.kind === 'convert_locally' && readiness.state === 'artifact_missing' && (
          <div className="flex items-center gap-1">
            <code className="mono rounded bg-ink-100 px-2 py-1 text-[0.65rem] dark:bg-ink-800">{spec.artifact.convert_command}</code>
            <CopyButton text={spec.artifact.convert_command} label="Copy conversion command" />
          </div>
        )}
      </div>
    </div>
  )
}

function RolesPanel({ role, supervised }: { role: string; supervised: boolean }) {
  const rungs: { id: string; title: string; requires: string; active: boolean }[] = [
    {
      id: 'observer',
      title: 'Observer',
      requires: 'A reachable node RPC — yours or someone else’s. Read-only.',
      active: role === 'observer',
    },
    {
      id: 'verifier',
      title: 'Verifier',
      requires:
        'A running full node. On this chain syncing IS verifying: every accepted block’s PALW attempt is checked by the nodes that accept it. No bond, no key.',
      active: role === 'verifier',
    },
    {
      id: 'producer',
      title: 'Producer (miner)',
      requires:
        'A bonded ML-DSA-87 key and a pay address. The first producer run registers the bond on-chain (collateral is locked, and slashable — that is what makes the work trustable); the printed outpoint then mines. Panel duty comes with it.',
      active: role === 'producer',
    },
  ]
  return (
    <section className="card p-4">
      <h3 className="text-sm font-semibold">Participation ladder</h3>
      <p className="mt-1 text-[0.7rem] text-ink-500 dark:text-ink-400">
        Every rung is the same node with more at stake. There is no separate miner on this network — the thing that runs the
        model is the thing that makes the block.
      </p>
      <div className="mt-3 space-y-2">
        {rungs.map((rung) => (
          <div
            key={rung.id}
            className={`rounded-lg border p-3 ${rung.active ? 'border-arc-500 bg-arc-500/5' : 'border-ink-200 dark:border-ink-800'}`}
          >
            <div className="flex items-center gap-2">
              <span className="text-xs font-semibold">{rung.title}</span>
              {rung.active && <span className="badge bg-arc-600 text-white">current role</span>}
            </div>
            <p className="mt-1 text-[0.7rem] leading-relaxed text-ink-500 dark:text-ink-400">{rung.requires}</p>
          </div>
        ))}
      </div>
      {supervised && role === 'producer' && (
        <p className="mt-3 flex gap-2 rounded-lg bg-amber-50 p-2 text-[0.7rem] text-amber-800 dark:bg-amber-950/40 dark:text-amber-300">
          <Icon name="warning" className="mt-0.5 size-4 shrink-0" />
          <span>
            Every block you produce opens a claim that lives on chain for hours, and your node must stay up to serve its
            material — a producer stopped with claims in flight defaults them <em>against its bond</em>. Stop mining only when
            you can leave the node running for the day.
          </span>
        </p>
      )}
    </section>
  )
}

function NodeSettingsPanel({ settings, save }: { settings: Settings; save: (s: Settings) => Promise<void> }) {
  const [draft, setDraft] = useState(settings.node)
  useEffect(() => setDraft(settings.node), [settings.node])
  const dirty = JSON.stringify(draft) !== JSON.stringify(settings.node)

  const set = <K extends keyof Settings['node']>(key: K, value: Settings['node'][K]) => setDraft({ ...draft, [key]: value })
  const text = (value: string) => (value.trim() === '' ? null : value)

  return (
    <Section title="Node configuration" description="Applied on the next node start. Producing fields follow the misakas join runbook.">
      <Field label="Network">
        <select className="input mt-1" value={draft.network} onChange={(e) => set('network', e.target.value as Settings['node']['network'])}>
          <option value="testnet11">testnet-11 — the live PALW network</option>
          <option value="devnet">devnet — local development (PALW v1, no class economy)</option>
          <option value="simnet">simnet — simulation, no proof-of-work</option>
        </select>
      </Field>
      <Field label="Role">
        <select className="input mt-1" value={draft.role} onChange={(e) => set('role', e.target.value as Settings['node']['role'])}>
          <option value="observer">Observer — read an existing node</option>
          <option value="verifier">Verifier — run a full node</option>
          <option value="producer">Producer — mine (bonded key required)</option>
        </select>
      </Field>
      <Field label="kaspad path" hint="Empty looks beside the Studio and on PATH.">
        <input className="input mt-1" placeholder="/path/to/kaspad" value={draft.kaspad_path ?? ''} onChange={(e) => set('kaspad_path', text(e.target.value))} />
      </Field>
      <Field label="Attach to RPC" hint="Watch an already-running node instead of launching one. host:port of its --rpclisten-json endpoint.">
        <input className="input mt-1" placeholder="127.0.0.1:28210" value={draft.rpc_url ?? ''} onChange={(e) => set('rpc_url', text(e.target.value))} />
      </Field>

      {draft.role === 'producer' && (
        <>
          <Field label="Producer key file" hint="32-byte ML-DSA-87 seed — generate with `misaka key gen`. The Studio passes the path; it never reads the key.">
            <input className="input mt-1" placeholder="~/.misaka/miner.seed" value={draft.producer_key_path ?? ''} onChange={(e) => set('producer_key_path', text(e.target.value))} />
          </Field>
          <Field label="Pay address" hint="Where rewards are paid, and where collateral returns when the bond retires.">
            <input className="input mt-1" placeholder="misakatest:…" value={draft.mining_address ?? ''} onChange={(e) => set('mining_address', text(e.target.value))} />
          </Field>
          <Field label="Bond outpoint" hint="txid:index, printed once by the registration run. Empty = the next start registers a bond and prints it.">
            <input className="input mt-1" placeholder="<txid>:0" value={draft.producer_bond ?? ''} onChange={(e) => set('producer_bond', text(e.target.value))} />
          </Field>
          <Field label="Fee outpoint" hint="Usually your bond carrier's change (txid:1). Empty = panel runs receipts-only.">
            <input className="input mt-1" placeholder="<txid>:1" value={draft.fee_outpoint ?? ''} onChange={(e) => set('fee_outpoint', text(e.target.value))} />
          </Field>
          <Field label="Class id" hint="Empty mines the floor (BASE-0, no artifact). Paste a model class id to mine that class.">
            <input className="input mt-1 mono" placeholder="(floor)" value={draft.producer_class ?? ''} onChange={(e) => set('producer_class', text(e.target.value))} />
          </Field>
          <Field label="Class artifact" hint="Path to the class artifact file, for a model class.">
            <input className="input mt-1" placeholder="/models/qwen36.palwq36" value={draft.class_artifact ?? ''} onChange={(e) => set('class_artifact', text(e.target.value))} />
          </Field>
        </>
      )}

      <Field label="Extra arguments" hint="Appended verbatim, one per line — e.g. --addpeer=…, --nodnsseed for isolated setups.">
        <textarea
          className="input mono mt-1 min-h-16"
          value={draft.extra_args.join('\n')}
          onChange={(e) => set('extra_args', e.target.value.split('\n').map((line) => line.trim()).filter(Boolean))}
        />
      </Field>

      {dirty && (
        <div className="flex justify-end gap-2">
          <button type="button" className="btn-ghost" onClick={() => setDraft(settings.node)}>
            Discard
          </button>
          <button type="button" className="btn-primary" onClick={() => void save({ ...settings, node: draft })}>
            Save node settings
          </button>
        </div>
      )}
    </Section>
  )
}
