// Sampling parameters, and the system prompt.
//
// Every control here maps to one field of the request the runtime sends the engine, and to one
// field of the `SamplingCommitment` the inference record commits to — so what the user sets is
// literally what gets hashed. That is why the seed is in this panel rather than buried in
// settings: it is the difference between a run that can be reproduced and one that cannot, and
// the panel says so.

import { useEffect, useState } from 'react'
import { tokens } from '../lib/format'
import type { Settings } from '../lib/types'
import { useStudio } from '../store/studio'
import { Slider } from './common'

export function ParametersPanel() {
  const settings = useStudio((s) => s.settings)
  const saveSettings = useStudio((s) => s.saveSettings)
  const runtime = useStudio((s) => s.runtime)
  const models = useStudio((s) => s.models)
  const [draft, setDraft] = useState<Settings | null>(settings)

  useEffect(() => setDraft(settings), [settings])
  if (!draft) return null

  const generation = draft.generation
  const loaded = models.find((m) => m.id === runtime?.model_id)
  const maxContext = loaded?.context_length ?? 131_072

  const update = (patch: Partial<Settings['generation']>) => {
    const next = { ...draft, generation: { ...draft.generation, ...patch } }
    setDraft(next)
  }

  // Saved on release rather than on every drag: a PUT per slider pixel would rewrite the settings
  // file a hundred times a second.
  const commit = () => {
    if (draft && settings && JSON.stringify(draft.generation) !== JSON.stringify(settings.generation)) void saveSettings(draft)
  }

  return (
    <div className="space-y-4" onPointerUp={commit} onBlur={commit}>
      <label className="block">
        <span className="text-xs font-medium text-ink-600 dark:text-ink-300">System prompt</span>
        <textarea
          className="input mt-1 min-h-20 resize-y"
          placeholder="Instructions the model sees before every conversation. Leave empty to use the model's own default."
          value={generation.system_prompt}
          onChange={(event) => update({ system_prompt: event.target.value })}
        />
      </label>

      <div className="grid gap-x-6 gap-y-4 sm:grid-cols-2">
        <Slider
          label="Temperature"
          value={generation.temperature}
          min={0}
          max={2}
          step={0.05}
          onChange={(temperature) => update({ temperature })}
          format={(v) => v.toFixed(2)}
          hint={generation.temperature === 0 ? 'Greedy decoding — reproducible without a seed' : 'Higher is more varied, and less repeatable'}
        />
        <Slider label="Top P" value={generation.top_p} min={0} max={1} step={0.01} onChange={(top_p) => update({ top_p })} format={(v) => v.toFixed(2)} />
        <Slider label="Top K" value={generation.top_k} min={0} max={200} step={1} onChange={(top_k) => update({ top_k })} hint="0 disables it" />
        <Slider label="Min P" value={generation.min_p} min={0} max={1} step={0.01} onChange={(min_p) => update({ min_p })} format={(v) => v.toFixed(2)} />
        <Slider
          label="Repeat penalty"
          value={generation.repeat_penalty}
          min={1}
          max={2}
          step={0.01}
          onChange={(repeat_penalty) => update({ repeat_penalty })}
          format={(v) => v.toFixed(2)}
        />
        <Slider
          label="Max tokens"
          value={generation.max_tokens}
          min={64}
          max={32_768}
          step={64}
          onChange={(max_tokens) => update({ max_tokens })}
          format={(v) => tokens(v)}
          hint="The ceiling for one reply"
        />
      </div>

      <div className="grid gap-4 sm:grid-cols-2">
        <label className="block">
          <span className="text-xs font-medium text-ink-600 dark:text-ink-300">Context size</span>
          <input
            className="input mt-1"
            type="number"
            min={512}
            max={maxContext}
            step={512}
            placeholder={`auto — ${tokens(loaded?.recommended_context ?? 4096)} for this model and machine`}
            value={generation.context_size ?? ''}
            onChange={(event) => update({ context_size: event.target.value === '' ? null : Number(event.target.value) })}
          />
          <span className="mt-1 block text-[0.7rem] text-ink-500 dark:text-ink-400">
            Applied on the next load. Trained for {tokens(loaded?.context_length)}; a longer context costs KV-cache memory.
          </span>
        </label>

        <label className="block">
          <span className="text-xs font-medium text-ink-600 dark:text-ink-300">Seed</span>
          <input
            className="input mt-1"
            type="number"
            placeholder="random — each run differs"
            value={generation.seed ?? ''}
            onChange={(event) => update({ seed: event.target.value === '' ? null : Number(event.target.value) })}
          />
          <span className="mt-1 block text-[0.7rem] text-ink-500 dark:text-ink-400">
            {generation.seed === null && generation.temperature > 0
              ? 'Without a seed, a sampled run cannot be reproduced — records say so.'
              : 'Fixed seed: the same prompt and settings reproduce the same answer.'}
          </span>
        </label>
      </div>
    </div>
  )
}
