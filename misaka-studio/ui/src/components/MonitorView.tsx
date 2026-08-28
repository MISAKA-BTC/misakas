// The performance monitor.
//
// The numbers are chosen to answer one question — *is this configuration the right one for this
// machine?* — so throughput sits next to memory pressure rather than on its own page. 12 tokens a
// second at 40 % of VRAM means try something bigger; 12 at 99 % means the context is too long.
//
// The sparkline is drawn as an SVG polyline from a fixed-length ring of samples. No chart library:
// it is sixty numbers and two axes, and a charting dependency would be larger than the runtime
// binary's own metrics code.

import { useEffect, useRef, useState } from 'react'
import { bytes, duration } from '../lib/format'
import { useStudio } from '../store/studio'
import { EmptyState, Meter, Stat } from './common'
import { ProvenancePanel } from './Provenance'

/** How many samples the sparklines keep. At one sample a second, two minutes of history. */
const HISTORY = 120

export function MonitorView() {
  const sample = useStudio((s) => s.sample)
  const system = useStudio((s) => s.system)
  const runtime = useStudio((s) => s.runtime)
  const models = useStudio((s) => s.models)
  const hashModel = useStudio((s) => s.hashModel)
  const [hashing, setHashing] = useState(false)

  const cpuHistory = useRef<number[]>([])
  const memHistory = useRef<number[]>([])
  const [, forceRender] = useState(0)

  useEffect(() => {
    if (!sample) return
    const push = (ring: number[], value: number) => {
      ring.push(value)
      if (ring.length > HISTORY) ring.shift()
    }
    push(cpuHistory.current, sample.hardware.cpu_percent)
    push(memHistory.current, (sample.hardware.memory_used / Math.max(1, sample.hardware.memory_total)) * 100)
    forceRender((n) => n + 1)
  }, [sample])

  if (!system) {
    return <EmptyState icon="gauge" title="Waiting for the runtime">Nothing to show until the runtime answers.</EmptyState>
  }

  const hardware = system.hardware
  const loaded = models.find((m) => m.id === runtime?.model_id)

  return (
    <div className="h-full overflow-y-auto p-4">
      <div className="grid gap-4 lg:grid-cols-3">
        <div className="space-y-4 lg:col-span-2">
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
            <Stat
              label="Tokens / sec"
              value={sample ? sample.generation.last_tokens_per_second.toFixed(1) : '—'}
              sub={sample?.generation.last_time_to_first_token_ms ? `first token ${duration(sample.generation.last_time_to_first_token_ms)}` : 'last generation'}
            />
            <Stat label="Generations" value={sample?.generation.total_generations ?? 0} sub={`${sample?.generation.total_tokens ?? 0} tokens total`} />
            <Stat label="CPU" value={`${(sample?.hardware.cpu_percent ?? 0).toFixed(0)}%`} sub={`${hardware.logical_cores} threads`} />
            <Stat
              label="Memory"
              value={bytes(sample?.hardware.memory_used ?? 0)}
              sub={`of ${bytes(hardware.total_memory)}`}
            />
          </div>

          <div className="card p-4">
            <h3 className="text-sm font-semibold">Last two minutes</h3>
            <div className="mt-4 space-y-4">
              <Sparkline label="CPU" values={cpuHistory.current} max={100} suffix="%" />
              <Sparkline label="Memory" values={memHistory.current} max={100} suffix="%" />
            </div>
          </div>

          <div className="card p-4">
            <h3 className="text-sm font-semibold">Devices</h3>
            <div className="mt-3 space-y-4">
              <div>
                <div className="flex justify-between text-xs">
                  <span className="font-medium">{hardware.cpu_name}</span>
                  <span className="text-ink-500 dark:text-ink-400">
                    {bytes(sample?.hardware.memory_used ?? 0)} / {bytes(hardware.total_memory)}
                  </span>
                </div>
                <div className="mt-1.5">
                  <Meter value={sample?.hardware.memory_used ?? 0} max={hardware.total_memory} />
                </div>
              </div>

              {hardware.accelerators
                .filter((a) => a.kind !== 'cpu')
                .map((accelerator) => {
                  const live = sample?.hardware.accelerators.find((a) => a.index === accelerator.index)
                  const used = live?.memory_used ?? null
                  const total = live?.memory_total ?? accelerator.total_memory ?? 0
                  return (
                    <div key={`${accelerator.kind}-${accelerator.index}`}>
                      <div className="flex justify-between text-xs">
                        <span className="font-medium">
                          {accelerator.name}
                          <span className="ml-2 badge bg-ink-100 text-ink-600 dark:bg-ink-800 dark:text-ink-300">{accelerator.kind}</span>
                        </span>
                        <span className="text-ink-500 dark:text-ink-400">
                          {used !== null ? `${bytes(used)} / ${bytes(total)}` : `${bytes(accelerator.usable_memory)} usable`}
                          {live?.utilization_percent !== null && live?.utilization_percent !== undefined && ` · ${live.utilization_percent.toFixed(0)}%`}
                          {live?.temperature_c ? ` · ${live.temperature_c.toFixed(0)}°C` : ''}
                        </span>
                      </div>
                      {used !== null && total > 0 && (
                        <div className="mt-1.5">
                          <Meter value={used} max={total} />
                        </div>
                      )}
                      {accelerator.kind === 'apple_unified' && (
                        <p className="mt-1 text-[0.7rem] text-ink-500 dark:text-ink-400">
                          Unified memory: the GPU shares the system pool, and {bytes(accelerator.usable_memory)} of it may be wired for
                          a model.
                        </p>
                      )}
                    </div>
                  )
                })}

              {!hardware.accelerators.some((a) => a.kind !== 'cpu') && (
                <p className="text-xs text-ink-500 dark:text-ink-400">
                  No accelerator detected — models run on the CPU. On a machine with an NVIDIA card, installing the driver and its
                  <span className="mono"> nvidia-smi </span> makes it appear here.
                </p>
              )}
            </div>
          </div>
        </div>

        <div className="space-y-4">
          <ProvenancePanel
            runtime={runtime}
            identity={loaded?.identity ?? null}
            hashing={hashing}
            onHash={async () => {
              if (!runtime?.model_id) return
              setHashing(true)
              await hashModel(runtime.model_id)
              setHashing(false)
            }}
          />

          <div className="card p-4">
            <h3 className="text-sm font-semibold">This machine</h3>
            <dl className="mt-3 space-y-1.5 text-xs">
              <Row label="OS" value={`${hardware.os} · ${hardware.arch}`} />
              <Row label="CPU" value={hardware.cpu_name} />
              <Row label="Cores" value={`${hardware.physical_cores ?? '?'} physical · ${hardware.logical_cores} logical`} />
              <Row label="Memory" value={bytes(hardware.total_memory)} />
              <Row label="Models" value={system.models_dir} mono />
              <Row label="Records" value={system.records_path} mono />
            </dl>
          </div>
        </div>
      </div>
    </div>
  )
}

function Row({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="flex justify-between gap-3">
      <dt className="shrink-0 text-ink-500 dark:text-ink-400">{label}</dt>
      <dd className={`truncate text-right ${mono ? 'mono text-[0.7rem]' : ''}`} title={value}>
        {value}
      </dd>
    </div>
  )
}

function Sparkline({ label, values, max, suffix }: { label: string; values: number[]; max: number; suffix: string }) {
  const width = 100
  const height = 28
  const points = values
    .map((value, index) => {
      const x = values.length <= 1 ? 0 : (index / (values.length - 1)) * width
      const y = height - Math.min(height, (Math.min(value, max) / max) * height)
      return `${x.toFixed(2)},${y.toFixed(2)}`
    })
    .join(' ')
  const latest = values.length > 0 ? values[values.length - 1] : null

  return (
    <div>
      <div className="flex justify-between text-xs">
        <span className="text-ink-600 dark:text-ink-300">{label}</span>
        <span className="mono text-ink-500 dark:text-ink-400">
          {latest !== null && latest !== undefined ? `${latest.toFixed(0)}${suffix}` : '—'}
        </span>
      </div>
      <svg viewBox={`0 0 ${width} ${height}`} preserveAspectRatio="none" className="mt-1 h-8 w-full" aria-hidden>
        {values.length > 1 && (
          <>
            <polyline points={`0,${height} ${points} ${width},${height}`} className="fill-arc-500/15" stroke="none" />
            <polyline points={points} className="stroke-arc-500" fill="none" strokeWidth={1.5} vectorEffect="non-scaling-stroke" />
          </>
        )}
      </svg>
    </div>
  )
}
