// The provenance panel: which artifact answered, under which runtime.
//
// This is the part of MISAKA Studio that no other local-LLM app has, and it is deliberately shown
// rather than logged. `h_M` and `h_R` are the same identities the chain derives, so what is on
// screen here is exactly what a validator would compute for the same model and engine — and the
// path from "an answer appeared" to "an answer that can be checked" starts with the user being
// able to see the difference.

import { shortHash } from '../lib/format'
import type { ModelIdentity, RuntimeStatus } from '../lib/types'
import { CopyButton, Icon } from './common'

function HashRow({ label, hash, hint }: { label: string; hash: string | null | undefined; hint?: string }) {
  return (
    <div className="flex items-center justify-between gap-3 py-1.5">
      <div className="min-w-0">
        <div className="text-[0.7rem] font-medium text-ink-600 dark:text-ink-300">{label}</div>
        {hint && <div className="text-[0.65rem] text-ink-500 dark:text-ink-500">{hint}</div>}
      </div>
      <div className="flex shrink-0 items-center gap-1">
        <code className="mono text-[0.7rem] text-ink-500 dark:text-ink-400">{shortHash(hash)}</code>
        {hash && <CopyButton text={hash} label={`Copy ${label}`} className="btn-ghost px-1.5 py-1" />}
      </div>
    </div>
  )
}

export function ProvenancePanel({
  runtime,
  identity,
  onHash,
  hashing,
}: {
  runtime: RuntimeStatus | null
  identity: ModelIdentity | null
  onHash?: () => void
  hashing?: boolean
}) {
  if (!runtime) return null
  const descriptor = runtime.descriptor

  return (
    <div className="card p-4">
      <div className="flex items-center gap-2">
        <Icon name="shield" className="size-4 text-arc-600 dark:text-arc-400" />
        <h3 className="text-sm font-semibold">Provenance</h3>
      </div>
      <p className="mt-1 text-[0.7rem] leading-relaxed text-ink-500 dark:text-ink-400">
        The identities MISAKA consensus derives for a model and a runtime. Recorded with every completion.
      </p>

      <div className="mt-3 divide-y divide-ink-100 dark:divide-ink-800">
        <HashRow label="Model identity (h_M)" hash={runtime.model_hash} hint="SHA-256 of the GGUF, its size, filename, repo and revision" />
        <HashRow label="Runtime identity (h_R)" hash={runtime.runtime_hash} hint="engine commit, patch, build number and build profile" />
        <HashRow label="Determinism class" hash={runtime.runtime_class_id} hint="the set of runtimes expected to agree bit for bit" />
      </div>

      {!runtime.model_hash && runtime.model_id && (
        <button type="button" className="btn-outline mt-3 w-full" onClick={onHash} disabled={hashing}>
          {hashing ? 'Hashing…' : 'Compute model identity'}
        </button>
      )}
      {!runtime.model_hash && runtime.model_id && (
        <p className="mt-2 text-[0.7rem] text-ink-500 dark:text-ink-400">
          Reads the whole file once, then caches the digest beside it. Until then, completions are recorded without a model identity
          rather than with a guessed one.
        </p>
      )}

      {identity?.base_repo && (
        <p className="mt-3 text-[0.7rem] text-ink-500 dark:text-ink-400">
          Converted from <span className="mono">{identity.base_repo}</span>
          {identity.base_revision && <span className="mono"> @ {shortHash(identity.base_revision, 7, 0)}</span>}
        </p>
      )}

      {descriptor && (
        <dl className="mt-3 space-y-1 border-t border-ink-100 pt-3 text-[0.7rem] dark:border-ink-800">
          <div className="flex justify-between gap-3">
            <dt className="text-ink-500 dark:text-ink-400">Backend</dt>
            <dd className="mono">{descriptor.backend}</dd>
          </div>
          <div className="flex justify-between gap-3">
            <dt className="text-ink-500 dark:text-ink-400">Engine</dt>
            <dd className="mono truncate" title={descriptor.engine_commit}>
              {descriptor.engine_commit === 'unknown' ? 'unidentified build' : shortHash(descriptor.engine_commit, 8, 0)}
              {descriptor.engine_build_number > 0 && ` (b${descriptor.engine_build_number})`}
            </dd>
          </div>
          <div className="flex justify-between gap-3">
            <dt className="text-ink-500 dark:text-ink-400">Class</dt>
            <dd className="mono truncate" title={descriptor.class_tag}>
              {descriptor.class_tag}
            </dd>
          </div>
        </dl>
      )}

      {descriptor?.engine_patch_sha256 === 'unknown' && (
        <p className="mt-3 flex gap-2 rounded-lg bg-amber-50 p-2 text-[0.7rem] text-amber-800 dark:bg-amber-950/40 dark:text-amber-300">
          <Icon name="warning" className="mt-0.5 size-3.5 shrink-0" />
          <span>
            This engine was installed outside the Studio, so its build flags cannot be proven — the identity records that honestly
            instead of claiming a profile it cannot verify.
          </span>
        </p>
      )}
    </div>
  )
}
