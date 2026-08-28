// The bar above the chat: which model is loaded, and everything you would change about it.
//
// It is a header rather than a settings page because the model and its sampling settings are what
// a person changes *while* chatting — and a parameter panel two clicks away is a parameter panel
// nobody uses.

import { useState } from 'react'
import { bytes, tokens } from '../lib/format'
import { useStudio } from '../store/studio'
import { Icon, QuantBadge, Spinner } from './common'
import { ParametersPanel } from './ParametersPanel'

export function ModelBar() {
  const models = useStudio((s) => s.models)
  const runtime = useStudio((s) => s.runtime)
  const loadingModelId = useStudio((s) => s.loadingModelId)
  const loadModel = useStudio((s) => s.loadModel)
  const unloadModel = useStudio((s) => s.unloadModel)
  const setView = useStudio((s) => s.setView)
  const sample = useStudio((s) => s.sample)
  const [showParameters, setShowParameters] = useState(false)

  const loaded = models.find((m) => m.id === runtime?.model_id) ?? null
  const busy = loadingModelId !== null

  return (
    <>
      <div className="flex items-center gap-3 border-b border-ink-200 px-4 py-2.5 dark:border-ink-800">
        <div className="flex min-w-0 items-center gap-2">
          <select
            className="input max-w-72 py-1"
            value={runtime?.model_id ?? ''}
            disabled={busy || models.length === 0}
            onChange={(event) => {
              const id = event.target.value
              if (id) void loadModel(id)
              else void unloadModel()
            }}
          >
            {models.length === 0 && <option value="">No models installed</option>}
            {models.length > 0 && <option value="">— no model loaded —</option>}
            {models.map((model) => (
              <option key={model.id} value={model.id}>
                {model.id}
              </option>
            ))}
          </select>
          {busy && <Spinner className="size-4 text-arc-600" />}
        </div>

        {loaded && (
          <div className="hidden min-w-0 items-center gap-2 text-xs text-ink-500 md:flex dark:text-ink-400">
            <QuantBadge quantization={loaded.quantization} />
            <span>{bytes(loaded.size_bytes)}</span>
            {runtime?.context_size && <span>· {tokens(runtime.context_size)} ctx</span>}
            {runtime?.gpu_layers !== null && runtime?.gpu_layers !== undefined && loaded.block_count && (
              <span title="Layers on the accelerator">
                · {Math.min(runtime.gpu_layers, loaded.block_count)}/{loaded.block_count} on GPU
              </span>
            )}
          </div>
        )}

        <div className="ml-auto flex items-center gap-2">
          {sample && sample.generation.last_tokens_per_second > 0 && (
            <span className="mono hidden text-xs text-ink-500 sm:inline dark:text-ink-400">
              {sample.generation.last_tokens_per_second.toFixed(1)} tok/s
            </span>
          )}
          {runtime?.model_id && (
            <button type="button" className="btn-ghost" onClick={() => void unloadModel()} title="Unload the model and free its memory">
              <Icon name="power" className="size-3.5" />
              <span className="hidden sm:inline">Unload</span>
            </button>
          )}
          <button
            type="button"
            className={`btn-ghost ${showParameters ? 'bg-ink-100 dark:bg-ink-800' : ''}`}
            onClick={() => setShowParameters((v) => !v)}
          >
            <Icon name="settings" className="size-3.5" />
            <span className="hidden sm:inline">Parameters</span>
          </button>
          {models.length === 0 && (
            <button type="button" className="btn-primary" onClick={() => setView('models')}>
              <Icon name="download" className="size-3.5" />
              Get a model
            </button>
          )}
        </div>
      </div>

      {showParameters && (
        <div className="border-b border-ink-200 bg-ink-100/50 px-4 py-4 dark:border-ink-800 dark:bg-ink-900/40">
          <div className="mx-auto max-w-3xl">
            <ParametersPanel />
          </div>
        </div>
      )}
    </>
  )
}
