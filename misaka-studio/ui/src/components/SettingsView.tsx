// Settings.
//
// Everything here writes to the runtime's settings file through the API, not to browser storage —
// the runtime is what acts on these, and a setting that lived only in the window would be a
// setting the headless `misaka-studiod` ignored.
//
// Two are guarded rather than merely offered: binding the server to a non-loopback address without
// an API key is refused by the runtime (it would be an open inference endpoint), and keeping
// transcripts is off by default and says exactly what it does before it is turned on.

import { useEffect, useState } from 'react'
import { api } from '../lib/api'
import { bytes } from '../lib/format'
import type { BackendInfo, Settings } from '../lib/types'
import { useStudio } from '../store/studio'
import { Field, Icon, Section, Toggle } from './common'

export function SettingsView() {
  const settings = useStudio((s) => s.settings)
  const save = useStudio((s) => s.saveSettings)
  const system = useStudio((s) => s.system)
  const [draft, setDraft] = useState<Settings | null>(settings)
  const [backends, setBackends] = useState<BackendInfo[]>([])

  useEffect(() => setDraft(settings), [settings])
  useEffect(() => {
    api.backends().then(setBackends).catch(() => setBackends([]))
  }, [])

  if (!draft) return null
  const dirty = JSON.stringify(draft) !== JSON.stringify(settings)

  const set = <K extends keyof Settings>(key: K, value: Settings[K]) => setDraft({ ...draft, [key]: value })

  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto max-w-3xl space-y-4 p-4 pb-24">
        <Section title="Models" description="Where GGUF files live. Moving this rescans; it does not move any files.">
          <Field label="Model directory">
            <input className="input mt-1" value={draft.models_dir} onChange={(e) => set('models_dir', e.target.value)} />
          </Field>
          {system && (
            <p className="text-[0.7rem] text-ink-500 dark:text-ink-400">
              Application data lives in <span className="mono">{system.data_dir}</span>.
            </p>
          )}
        </Section>

        <Section title="Backend" description="Which engine runs the model. Changing it unloads whatever is loaded.">
          <Field label="Engine">
            <select
              className="input mt-1"
              value={draft.backend.kind}
              onChange={(e) => set('backend', { ...draft.backend, kind: e.target.value as Settings['backend']['kind'] })}
            >
              <option value="auto">Auto — MLX on Apple Silicon, llama.cpp elsewhere</option>
              <option value="llama_cpp">llama.cpp (llama-server)</option>
              <option value="mlx">MLX (Apple Silicon)</option>
              <option value="mock">Mock — canned replies, no model needed</option>
            </select>
          </Field>

          {backends.length > 0 && (
            <ul className="space-y-1.5 text-xs">
              {backends.map((backend) => (
                <li key={backend.name} className="flex items-start gap-2">
                  <span className={`mt-1 size-1.5 shrink-0 rounded-full ${backend.availability.state === 'available' ? 'bg-emerald-500' : 'bg-ink-400'}`} />
                  <span>
                    <span className="mono font-medium">{backend.name}</span>{' '}
                    {backend.availability.state === 'available' ? (
                      <span className="text-ink-500 dark:text-ink-400">{backend.availability.detail}</span>
                    ) : (
                      <span className="text-ink-500 dark:text-ink-400">
                        {backend.availability.reason}. {backend.availability.remedy}
                      </span>
                    )}
                  </span>
                </li>
              ))}
            </ul>
          )}

          <Field label="llama-server path" hint="Leave empty to use the one on PATH, or the one packaged beside the app.">
            <input
              className="input mt-1"
              placeholder="/usr/local/bin/llama-server"
              value={draft.backend.llama_server_path ?? ''}
              onChange={(e) => set('backend', { ...draft.backend, llama_server_path: e.target.value || null })}
            />
          </Field>

          <Field label="GPU offload">
            <select
              className="input mt-1"
              value={draft.backend.gpu_layers.mode}
              onChange={(e) => {
                const mode = e.target.value as 'auto' | 'all' | 'none' | 'fixed'
                set('backend', { ...draft.backend, gpu_layers: mode === 'fixed' ? { mode, layers: 20 } : { mode } })
              }}
            >
              <option value="auto">Auto — as many layers as fit</option>
              <option value="all">All layers</option>
              <option value="none">CPU only</option>
              <option value="fixed">A fixed number of layers</option>
            </select>
          </Field>
          {draft.backend.gpu_layers.mode === 'fixed' && (
            <Field label="Layers on the GPU">
              <input
                className="input mt-1"
                type="number"
                min={0}
                value={draft.backend.gpu_layers.layers}
                onChange={(e) => set('backend', { ...draft.backend, gpu_layers: { mode: 'fixed', layers: Number(e.target.value) } })}
              />
            </Field>
          )}

          <div className="grid gap-4 sm:grid-cols-2">
            <Field label="Threads" hint="Empty lets the engine choose.">
              <input
                className="input mt-1"
                type="number"
                min={1}
                placeholder="auto"
                value={draft.backend.threads ?? ''}
                onChange={(e) => set('backend', { ...draft.backend, threads: e.target.value === '' ? null : Number(e.target.value) })}
              />
            </Field>
            <Field label="Load timeout (seconds)" hint="A large model on a slow disk genuinely takes minutes.">
              <input
                className="input mt-1"
                type="number"
                min={30}
                value={draft.backend.startup_timeout_secs}
                onChange={(e) => set('backend', { ...draft.backend, startup_timeout_secs: Number(e.target.value) })}
              />
            </Field>
          </div>

          <div className="space-y-2.5">
            <Toggle
              label="Flash attention"
              checked={draft.backend.flash_attention}
              onChange={(flash_attention) => set('backend', { ...draft.backend, flash_attention })}
              hint="Large memory saving on long contexts. Not every engine build supports it."
            />
            <Toggle
              label="Memory-map the model"
              checked={draft.backend.use_mmap}
              onChange={(use_mmap) => set('backend', { ...draft.backend, use_mmap })}
              hint="Faster loads and lower memory. Turn off only if a model fails to load."
            />
            <Toggle
              label="Lock the model in RAM"
              checked={draft.backend.use_mlock}
              onChange={(use_mlock) => set('backend', { ...draft.backend, use_mlock })}
              hint="Stops the OS swapping weights out mid-generation — and stops anything loading that does not fit."
            />
          </div>
        </Section>

        <Section title="API" description="The OpenAI-compatible endpoint other applications can point at.">
          <div className="grid gap-4 sm:grid-cols-2">
            <Field label="Host">
              <input className="input mt-1" value={draft.server.host} onChange={(e) => set('server', { ...draft.server, host: e.target.value })} />
            </Field>
            <Field label="Port">
              <input
                className="input mt-1"
                type="number"
                value={draft.server.port}
                onChange={(e) => set('server', { ...draft.server, port: Number(e.target.value) })}
              />
            </Field>
          </div>
          <Field label="API key" hint="Required when the host is not a loopback address; optional otherwise.">
            <input
              className="input mt-1"
              type="password"
              placeholder="none"
              value={draft.server.api_key ?? ''}
              onChange={(e) => set('server', { ...draft.server, api_key: e.target.value || null })}
            />
          </Field>
          {draft.server.host !== '127.0.0.1' && draft.server.host !== 'localhost' && !draft.server.api_key && (
            <p className="flex gap-2 rounded-lg bg-amber-50 p-2 text-xs text-amber-800 dark:bg-amber-950/40 dark:text-amber-300">
              <Icon name="warning" className="mt-0.5 size-4 shrink-0" />
              Binding to {draft.server.host} without an API key would let anyone on the network use this model. The runtime will refuse
              to save this.
            </p>
          )}
          <p className="text-[0.7rem] text-ink-500 dark:text-ink-400">
            Changes take effect the next time the runtime starts. Point any OpenAI client at{' '}
            <span className="mono">
              http://{draft.server.host}:{draft.server.port}/v1
            </span>
            .
          </p>
        </Section>

        <Section title="Hugging Face" description="Where models are searched for and downloaded from.">
          <Field label="Endpoint" hint="Change this for a mirror or an internal proxy.">
            <input
              className="input mt-1"
              value={draft.huggingface.endpoint}
              onChange={(e) => set('huggingface', { ...draft.huggingface, endpoint: e.target.value })}
            />
          </Field>
          <Field label="Access token" hint="Needed for gated repositories, and it raises the rate limit.">
            <input
              className="input mt-1"
              type="password"
              placeholder="none"
              value={draft.huggingface.token ?? ''}
              onChange={(e) => set('huggingface', { ...draft.huggingface, token: e.target.value || null })}
            />
          </Field>
        </Section>

        <Section title="Provenance" description="What the Studio records about its own inferences.">
          <Toggle
            label="Record an inference record per completion"
            checked={draft.provenance.record_inferences}
            onChange={(record_inferences) => set('provenance', { ...draft.provenance, record_inferences })}
            hint="Model identity, runtime identity, and commitments to the prompt and the answer. This is what a future verification layer reads."
          />
          <Toggle
            label="Keep prompt and completion text with each record"
            checked={draft.provenance.keep_transcripts}
            onChange={(keep_transcripts) => set('provenance', { ...draft.provenance, keep_transcripts })}
            hint="Off by default. Records commit to the text with a hash; storing the text as well makes the log a second copy of every conversation. Turn it on only if you need runs to be replayable."
          />
          <Field label="Records kept">
            <input
              className="input mt-1"
              type="number"
              min={100}
              step={100}
              value={draft.provenance.max_records}
              onChange={(e) => set('provenance', { ...draft.provenance, max_records: Number(e.target.value) })}
            />
          </Field>
        </Section>

        <Section title="Appearance" description="">
          <Field label="Theme">
            <select className="input mt-1" value={draft.ui.theme} onChange={(e) => set('ui', { ...draft.ui, theme: e.target.value as Settings['ui']['theme'] })}>
              <option value="system">Match the system</option>
              <option value="light">Light</option>
              <option value="dark">Dark</option>
            </select>
          </Field>
          <Toggle label="Show the provenance panel" checked={draft.ui.show_provenance} onChange={(show_provenance) => set('ui', { ...draft.ui, show_provenance })} />
          <Toggle label="Show performance figures while generating" checked={draft.ui.show_performance} onChange={(show_performance) => set('ui', { ...draft.ui, show_performance })} />
        </Section>

        {system && (
          <p className="text-center text-[0.7rem] text-ink-500 dark:text-ink-400">
            {system.hardware.cpu_name} · {bytes(system.hardware.total_memory)} · {system.hardware.os}
          </p>
        )}
      </div>

      {dirty && (
        <div className="sticky bottom-0 border-t border-ink-200 bg-white/90 px-4 py-3 backdrop-blur dark:border-ink-800 dark:bg-ink-900/90">
          <div className="mx-auto flex max-w-3xl items-center justify-end gap-2">
            <button type="button" className="btn-ghost" onClick={() => setDraft(settings)}>
              Discard
            </button>
            <button type="button" className="btn-primary" onClick={() => void save(draft)}>
              Save settings
            </button>
          </div>
        </div>
      )}
    </div>
  )
}
