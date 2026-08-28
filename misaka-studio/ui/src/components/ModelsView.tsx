// Models: what is installed, what can be downloaded, and what is downloading now.
//
// The organising idea is that **a model's size is not the question**. "4.7 GB" tells nobody
// whether their laptop can run it. So every model — installed or not — is shown with a verdict
// for this machine, and the installed list gets the runtime's real arithmetic (weights + KV cache
// at the context it would actually load with) rather than a guess.

import { useEffect, useState } from 'react'
import { api } from '../lib/api'
import { bytes, count, eta, params, rate, relativeTime, tokens } from '../lib/format'
import type { CatalogEntry, CatalogRepo, DownloadProgress, HardwareSnapshot } from '../lib/types'
import { useStudio } from '../store/studio'
import { EmptyState, FitBadge, Icon, QuantBadge, Spinner } from './common'

export function ModelsView() {
  const [tab, setTab] = useState<'installed' | 'discover'>('installed')
  const models = useStudio((s) => s.models)
  const downloads = useStudio((s) => s.downloads)
  const refreshModels = useStudio((s) => s.refreshModels)

  const active = downloads.filter((d) => d.status === 'downloading' || d.status === 'verifying')

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <div className="flex items-center gap-2 border-b border-ink-200 px-4 py-2.5 dark:border-ink-800">
        <div className="flex gap-1 rounded-lg bg-ink-100 p-1 dark:bg-ink-900">
          {(['installed', 'discover'] as const).map((value) => (
            <button
              key={value}
              type="button"
              onClick={() => setTab(value)}
              className={`rounded-md px-3 py-1 text-sm capitalize transition-colors ${
                tab === value ? 'bg-white font-medium shadow-sm dark:bg-ink-800' : 'text-ink-600 dark:text-ink-400'
              }`}
            >
              {value}
              {value === 'installed' && models.length > 0 && <span className="ml-1.5 text-xs text-ink-500">{models.length}</span>}
            </button>
          ))}
        </div>
        {tab === 'installed' && (
          <button type="button" className="btn-ghost ml-auto" onClick={() => void refreshModels()}>
            <Icon name="refresh" className="size-3.5" />
            Rescan
          </button>
        )}
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto">
        {active.length > 0 && <DownloadsPanel downloads={downloads} />}
        {tab === 'installed' ? <InstalledList /> : <DiscoverPanel />}
      </div>
    </div>
  )
}

function DownloadsPanel({ downloads }: { downloads: DownloadProgress[] }) {
  const cancel = async (id: string) => {
    try {
      await api.cancelDownload(id)
    } catch {
      /* the store's SSE stream reports the outcome */
    }
  }

  return (
    <div className="border-b border-ink-200 bg-ink-100/60 px-4 py-3 dark:border-ink-800 dark:bg-ink-900/40">
      <h3 className="mb-2 text-xs font-semibold uppercase tracking-wide text-ink-500 dark:text-ink-400">Downloading</h3>
      <div className="space-y-3">
        {downloads
          .filter((d) => d.status === 'downloading' || d.status === 'verifying')
          .map((download) => {
            const percent = download.total ? (download.downloaded / download.total) * 100 : null
            const remaining =
              download.total && download.bytes_per_second > 1 ? (download.total - download.downloaded) / download.bytes_per_second : null
            return (
              <div key={download.id} className="card p-3">
                <div className="flex items-center gap-2">
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-sm font-medium">{download.model_id}</div>
                    <div className="truncate text-[0.7rem] text-ink-500 dark:text-ink-400">{download.repo}</div>
                  </div>
                  <button type="button" className="btn-ghost px-1.5 py-1" title="Cancel" onClick={() => void cancel(download.id)}>
                    <Icon name="x" className="size-3.5" />
                  </button>
                </div>
                <div className="mt-2 h-1.5 w-full overflow-hidden rounded-full bg-ink-200 dark:bg-ink-800">
                  <div
                    className={`h-full rounded-full transition-[width] duration-300 ${download.status === 'verifying' ? 'bg-emerald-500' : 'bg-arc-500'}`}
                    style={{ width: `${percent ?? 100}%` }}
                  />
                </div>
                <div className="mt-1 flex justify-between text-[0.7rem] text-ink-500 dark:text-ink-400">
                  <span>
                    {download.status === 'verifying'
                      ? 'Verifying the published digest…'
                      : `${bytes(download.downloaded)} of ${bytes(download.total)} · ${rate(download.bytes_per_second)}`}
                  </span>
                  <span>{download.status === 'verifying' ? '' : remaining !== null ? eta(remaining) : ''}</span>
                </div>
              </div>
            )
          })}
      </div>
    </div>
  )
}

function InstalledList() {
  const models = useStudio((s) => s.models)
  const runtime = useStudio((s) => s.runtime)
  const loadingModelId = useStudio((s) => s.loadingModelId)
  const loadModel = useStudio((s) => s.loadModel)
  const unloadModel = useStudio((s) => s.unloadModel)
  const deleteModel = useStudio((s) => s.deleteModel)
  const hashModel = useStudio((s) => s.hashModel)
  const system = useStudio((s) => s.system)
  const setView = useStudio((s) => s.setView)
  const [confirming, setConfirming] = useState<string | null>(null)
  const [hashing, setHashing] = useState<string | null>(null)

  if (models.length === 0) {
    return (
      <EmptyState icon="cube" title="No models installed">
        Models live in <code className="mono">{system?.models_dir ?? 'the model directory'}</code>. Put a <code className="mono">.gguf</code>{' '}
        file there and rescan, or open <strong>Discover</strong> to download one.
      </EmptyState>
    )
  }

  return (
    <div className="space-y-3 p-4">
      {models.map((model) => {
        const isLoaded = runtime?.model_id === model.id
        return (
          <div key={model.id} className={`card p-4 ${isLoaded ? 'ring-1 ring-arc-500' : ''}`}>
            <div className="flex flex-wrap items-start gap-3">
              <div className="min-w-0 flex-1">
                <div className="flex flex-wrap items-center gap-2">
                  <h3 className="truncate text-sm font-semibold">{model.id}</h3>
                  <QuantBadge quantization={model.quantization} />
                  {isLoaded && <span className="badge bg-arc-600 text-white">loaded</span>}
                  {model.expert_count && <span className="badge bg-ink-100 text-ink-600 dark:bg-ink-800 dark:text-ink-300">MoE · {model.expert_count} experts</span>}
                </div>

                <div className="mt-1.5 flex flex-wrap gap-x-4 gap-y-1 text-xs text-ink-500 dark:text-ink-400">
                  <span>{bytes(model.size_bytes)}</span>
                  {model.parameter_count && <span>{params(model.parameter_count)} params</span>}
                  {model.architecture && <span className="mono">{model.architecture}</span>}
                  {model.context_length && <span>{tokens(model.context_length)} trained ctx</span>}
                  {model.block_count && <span>{model.block_count} layers</span>}
                  {model.modified_at && <span>added {relativeTime(model.modified_at)}</span>}
                </div>

                <div className="mt-2 flex flex-wrap items-center gap-2">
                  <FitBadge fit={model.fit} summary={model.fit_summary} />
                  <span className="text-[0.7rem] text-ink-500 dark:text-ink-400">
                    at {tokens(model.recommended_context)} context · {bytes(model.requirements.kv_cache_bytes)} of that is KV cache
                  </span>
                </div>

                {model.source.repo && (
                  <div className="mt-2 truncate text-[0.7rem] text-ink-500 dark:text-ink-400">
                    from <span className="mono">{model.source.repo}</span>
                    {model.source.revision && <span className="mono"> @ {model.source.revision.slice(0, 7)}</span>}
                  </div>
                )}
                {model.identity && (
                  <div className="mt-1 truncate text-[0.7rem] text-ink-500 dark:text-ink-400">
                    h_M <span className="mono">{model.identity.h_m.slice(0, 24)}…</span>
                  </div>
                )}
              </div>

              <div className="flex shrink-0 flex-col items-end gap-2">
                {isLoaded ? (
                  <button type="button" className="btn-outline" onClick={() => void unloadModel()}>
                    <Icon name="power" className="size-3.5" />
                    Unload
                  </button>
                ) : (
                  <button
                    type="button"
                    className="btn-primary"
                    disabled={loadingModelId !== null}
                    onClick={async () => {
                      await loadModel(model.id)
                      setView('chat')
                    }}
                  >
                    {loadingModelId === model.id ? <Spinner className="size-3.5" /> : <Icon name="power" className="size-3.5" />}
                    Load
                  </button>
                )}

                <div className="flex gap-1">
                  {!model.sha256 && (
                    <button
                      type="button"
                      className="btn-ghost"
                      title="Read the file once and record its identity"
                      disabled={hashing === model.id}
                      onClick={async () => {
                        setHashing(model.id)
                        await hashModel(model.id)
                        setHashing(null)
                      }}
                    >
                      {hashing === model.id ? <Spinner className="size-3.5" /> : <Icon name="shield" className="size-3.5" />}
                      Identify
                    </button>
                  )}
                  {confirming === model.id ? (
                    <>
                      <button
                        type="button"
                        className="btn-danger"
                        onClick={async () => {
                          setConfirming(null)
                          await deleteModel(model.id)
                        }}
                      >
                        Delete {bytes(model.size_bytes)}
                      </button>
                      <button type="button" className="btn-ghost" onClick={() => setConfirming(null)}>
                        Cancel
                      </button>
                    </>
                  ) : (
                    <button type="button" className="btn-ghost" title="Delete this model" onClick={() => setConfirming(model.id)}>
                      <Icon name="trash" className="size-3.5" />
                    </button>
                  )}
                </div>
              </div>
            </div>
          </div>
        )
      })}
    </div>
  )
}

/**
 * A rough fit check for a file that is not downloaded yet.
 *
 * Only the size is known before download — no layer count, no KV-cache shape — so this adds a
 * flat 15 % for runtime overhead and says "estimated". Once the file is on disk the runtime does
 * the real arithmetic; presenting this as anything more than a hint would be dishonest.
 */
function estimateFit(size: number | null, hardware: HardwareSnapshot | undefined): { label: string; tone: string } | null {
  if (!size || !hardware) return null
  const gpu = Math.max(0, ...hardware.accelerators.filter((a) => a.kind !== 'cpu').map((a) => a.usable_memory ?? 0))
  const ram = Math.max(0, hardware.total_memory - 2 * 1024 ** 3)
  const needed = size * 1.15
  if (gpu > 0 && needed <= gpu) return { label: 'fits in VRAM (est.)', tone: 'bg-emerald-100 text-emerald-800 dark:bg-emerald-950 dark:text-emerald-300' }
  if (needed <= ram) return { label: 'fits in RAM (est.)', tone: 'bg-amber-100 text-amber-800 dark:bg-amber-950/60 dark:text-amber-300' }
  return { label: 'too large for this machine (est.)', tone: 'bg-red-100 text-red-800 dark:bg-red-950/60 dark:text-red-300' }
}

function DiscoverPanel() {
  const [query, setQuery] = useState('')
  const [results, setResults] = useState<CatalogEntry[] | null>(null)
  const [searching, setSearching] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [openRepo, setOpenRepo] = useState<string | null>(null)
  const system = useStudio((s) => s.system)

  const search = async (text: string) => {
    if (!text.trim()) return
    setSearching(true)
    setError(null)
    setOpenRepo(null)
    try {
      setResults(await api.search(text.trim()))
    } catch (e) {
      setError((e as Error).message)
      setResults(null)
    } finally {
      setSearching(false)
    }
  }

  return (
    <div className="p-4">
      <form
        className="flex gap-2"
        onSubmit={(event) => {
          event.preventDefault()
          void search(query)
        }}
      >
        <div className="relative flex-1">
          <Icon name="search" className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-ink-400" />
          <input
            className="input pl-9"
            placeholder="Search Hugging Face for GGUF models — qwen3, llama, gemma, deepseek…"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
        </div>
        <button type="submit" className="btn-primary" disabled={searching || !query.trim()}>
          {searching ? <Spinner className="size-3.5" /> : <Icon name="search" className="size-3.5" />}
          Search
        </button>
      </form>

      <p className="mt-2 text-[0.7rem] text-ink-500 dark:text-ink-400">
        Searching <span className="mono">{system?.catalog_endpoint ?? 'huggingface.co'}</span> · only repositories with GGUF files are
        listed, because those are the ones this runtime can load.
      </p>

      {error && (
        <div className="card mt-4 border-red-300 p-3 text-sm text-red-700 dark:border-red-900 dark:text-red-300">
          <div className="flex gap-2">
            <Icon name="warning" className="mt-0.5 size-4 shrink-0" />
            <span>{error}</span>
          </div>
        </div>
      )}

      {results?.length === 0 && <EmptyState icon="search" title="Nothing found">Try a shorter query — a model family name usually works better than a full repository name.</EmptyState>}

      <div className="mt-4 space-y-2">
        {results?.map((entry) => (
          <RepoCard key={entry.id} entry={entry} open={openRepo === entry.id} onToggle={() => setOpenRepo(openRepo === entry.id ? null : entry.id)} />
        ))}
      </div>
    </div>
  )
}

function RepoCard({ entry, open, onToggle }: { entry: CatalogEntry; open: boolean; onToggle: () => void }) {
  const [repo, setRepo] = useState<CatalogRepo | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const system = useStudio((s) => s.system)
  const models = useStudio((s) => s.models)
  const toast = useStudio((s) => s.toast)
  const setDownload = useStudio((s) => s.setDownload)

  useEffect(() => {
    if (!open || repo || loading) return
    setLoading(true)
    api
      .repo(entry.id)
      .then(setRepo)
      .catch((e) => setError((e as Error).message))
      .finally(() => setLoading(false))
  }, [open, repo, loading, entry.id])

  const start = async (file: string, sha256: string | null, size: number | null) => {
    try {
      const progress = await api.startDownload({
        repo: entry.id,
        revision: repo?.revision ?? null,
        file,
        sha256,
        size,
        base_model: repo?.base_model ?? null,
      })
      setDownload(progress)
      toast('info', `Downloading ${progress.model_id}`)
    } catch (e) {
      toast('error', (e as Error).message)
    }
  }

  return (
    <div className="card overflow-hidden">
      <button type="button" className="flex w-full items-center gap-3 p-4 text-left" onClick={onToggle}>
        <Icon name="chevron" className={`size-4 shrink-0 text-ink-400 transition-transform ${open ? 'rotate-90' : ''}`} />
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className="truncate text-sm font-medium">{entry.id}</span>
            {entry.gated && <span className="badge bg-amber-100 text-amber-800 dark:bg-amber-950/60 dark:text-amber-300">gated</span>}
          </div>
          <div className="mt-0.5 flex gap-3 text-[0.7rem] text-ink-500 dark:text-ink-400">
            <span>{count(entry.downloads)} downloads</span>
            <span>{count(entry.likes)} likes</span>
            {entry.last_modified && <span>updated {new Date(entry.last_modified).toLocaleDateString()}</span>}
          </div>
        </div>
      </button>

      {open && (
        <div className="border-t border-ink-200 dark:border-ink-800">
          {loading && (
            <div className="flex items-center gap-2 p-4 text-sm text-ink-500 dark:text-ink-400">
              <Spinner className="size-4" /> Reading the repository…
            </div>
          )}
          {error && <p className="p-4 text-sm text-red-600 dark:text-red-400">{error}</p>}
          {repo && repo.files.length === 0 && <p className="p-4 text-sm text-ink-500 dark:text-ink-400">No GGUF files in this repository.</p>}
          {repo && repo.files.length > 0 && (
            <table className="w-full text-sm">
              <tbody>
                {repo.files.map((file) => {
                  const installed = models.some((m) => m.id === file.path.replace(/\.gguf$/i, '').split('/').pop())
                  const fit = estimateFit(file.size, system?.hardware)
                  return (
                    <tr key={file.path} className="border-t border-ink-100 first:border-t-0 dark:border-ink-800">
                      <td className="px-4 py-2.5">
                        <div className="flex flex-wrap items-center gap-2">
                          <QuantBadge quantization={file.quantization} />
                          <span className="mono truncate text-xs">{file.path}</span>
                        </div>
                        <div className="mt-1 flex flex-wrap items-center gap-2 text-[0.7rem] text-ink-500 dark:text-ink-400">
                          <span>{bytes(file.size)}</span>
                          {fit && <span className={`badge ${fit.tone}`}>{fit.label}</span>}
                          {file.sha256 && <span className="mono">sha256 {file.sha256.slice(0, 10)}…</span>}
                        </div>
                      </td>
                      <td className="w-32 px-4 py-2.5 text-right">
                        {installed ? (
                          <span className="text-[0.7rem] text-ink-500 dark:text-ink-400">installed</span>
                        ) : (
                          <button type="button" className="btn-outline" onClick={() => void start(file.path, file.sha256, file.size)}>
                            <Icon name="download" className="size-3.5" />
                            Download
                          </button>
                        )}
                      </td>
                    </tr>
                  )
                })}
              </tbody>
            </table>
          )}
          {repo?.revision && (
            <p className="border-t border-ink-100 px-4 py-2 text-[0.7rem] text-ink-500 dark:border-ink-800 dark:text-ink-400">
              Downloads are pinned to revision <span className="mono">{repo.revision.slice(0, 12)}</span>, verified against the digest
              the repository publishes, and recorded with the model.
            </p>
          )}
        </div>
      )}
    </div>
  )
}
