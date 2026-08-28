// The client for the MISAKA Runtime API.
//
// One rule runs through this file: **the UI is an ordinary client of the public API.** Chat goes
// through `/v1/chat/completions` exactly as a third-party OpenAI client would, so the endpoint
// other applications depend on is the one this app exercises every time someone types a message.
// A private channel for the UI would let that endpoint rot unnoticed.

import type {
  BackendInfo,
  CatalogEntry,
  CatalogRepo,
  DownloadProgress,
  InferenceRecord,
  ModelView,
  RuntimeSample,
  RuntimeStatus,
  Settings,
  SystemInfo,
} from './types'

/**
 * Where the runtime is.
 *
 * Empty — same origin — when the runtime is serving this bundle over HTTP, and when the Vite dev
 * server is proxying to it. The desktop shell loads the bundle from disk instead, where there is
 * no origin to be same as, so it injects the runtime's URL before this module runs. One bundle,
 * three ways of reaching the same API.
 */
export const API_BASE: string = (globalThis as { __MISAKA_STUDIO_API__?: string }).__MISAKA_STUDIO_API__ ?? ''

export class ApiError extends Error {
  constructor(
    message: string,
    readonly status: number,
    readonly code?: string,
  ) {
    super(message)
    this.name = 'ApiError'
  }
}

async function parseError(response: Response): Promise<ApiError> {
  // The runtime answers in OpenAI's error shape. Falling back to the status text matters for the
  // cases that never reach our handler — a proxy 502, a body that is not JSON — where "Failed to
  // fetch" would otherwise be all the user sees.
  try {
    const body = await response.json()
    const message = body?.error?.message ?? body?.message
    if (typeof message === 'string') return new ApiError(message, response.status, body?.error?.code)
  } catch {
    /* fall through */
  }
  return new ApiError(`${response.status} ${response.statusText}`, response.status)
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`${API_BASE}${path}`, {
    ...init,
    headers: { 'content-type': 'application/json', ...(init?.headers ?? {}) },
  })
  if (!response.ok) throw await parseError(response)
  if (response.status === 204) return undefined as T
  return (await response.json()) as T
}

export const api = {
  health: () => request<{ status: string; version: string }>('/api/v1/health'),
  system: () => request<SystemInfo>('/api/v1/system'),
  metrics: () => request<RuntimeSample>('/api/v1/metrics'),

  models: () => request<ModelView[]>('/api/v1/models'),
  refreshModels: () => request<ModelView[]>('/api/v1/models/refresh', { method: 'POST' }),
  model: (id: string) => request<ModelView>(`/api/v1/models/${encodeURIComponent(id)}`),
  deleteModel: (id: string) => request<{ deleted: string }>(`/api/v1/models/${encodeURIComponent(id)}`, { method: 'DELETE' }),
  loadModel: (id: string, contextSize?: number) =>
    request<RuntimeStatus>(`/api/v1/models/${encodeURIComponent(id)}/load`, {
      method: 'POST',
      body: JSON.stringify({ context_size: contextSize ?? null }),
    }),
  unloadModel: () => request<RuntimeStatus>('/api/v1/models/unload', { method: 'POST' }),
  hashModel: (id: string) => request<ModelView>(`/api/v1/models/${encodeURIComponent(id)}/hash`, { method: 'POST' }),

  runtime: () => request<RuntimeStatus>('/api/v1/runtime'),
  backends: () => request<BackendInfo[]>('/api/v1/runtime/backends'),

  search: (q: string, limit = 24) => request<CatalogEntry[]>(`/api/v1/catalog/search?q=${encodeURIComponent(q)}&limit=${limit}`),
  repo: (id: string) => request<CatalogRepo>(`/api/v1/catalog/repo/${id}`),

  downloads: () => request<DownloadProgress[]>('/api/v1/downloads'),
  startDownload: (body: { repo: string; revision?: string | null; file: string; sha256?: string | null; size?: number | null; base_model?: string | null }) =>
    request<DownloadProgress>('/api/v1/downloads', { method: 'POST', body: JSON.stringify(body) }),
  cancelDownload: (id: string) => request<{ cancelling: string }>(`/api/v1/downloads/${id}`, { method: 'DELETE' }),

  settings: () => request<Settings>('/api/v1/settings'),
  saveSettings: (settings: Settings) => request<Settings>('/api/v1/settings', { method: 'PUT', body: JSON.stringify(settings) }),

  records: (limit = 50) => request<InferenceRecord[]>(`/api/v1/records?limit=${limit}`),
}

/** One event from a streamed completion. */
export type ChatStreamEvent =
  | { type: 'delta'; text: string }
  | { type: 'done'; finishReason: string; usage: { prompt_tokens: number; completion_tokens: number; total_tokens: number } }
  | { type: 'error'; message: string }

export type ChatRequest = {
  model?: string
  messages: { role: string; content: string }[]
  temperature?: number
  top_p?: number
  top_k?: number
  min_p?: number
  repeat_penalty?: number
  max_tokens?: number
  seed?: number | null
}

/**
 * Stream a chat completion, yielding events as they arrive.
 *
 * `signal` is what the Stop button pulls: aborting the fetch closes the connection, which is what
 * tells the runtime — and through it the engine — to stop generating. A "stop" that only hid the
 * output would leave the GPU busy for another thousand tokens.
 */
export async function* streamChat(request: ChatRequest, signal: AbortSignal): AsyncGenerator<ChatStreamEvent> {
  const response = await fetch(`${API_BASE}/v1/chat/completions`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ ...request, stream: true }),
    signal,
  })
  if (!response.ok) throw await parseError(response)
  if (!response.body) throw new ApiError('the runtime returned no body', 500)

  const reader = response.body.getReader()
  const decoder = new TextDecoder()
  let buffer = ''

  while (true) {
    const { done, value } = await reader.read()
    if (done) break
    buffer += decoder.decode(value, { stream: true })

    // SSE frames end at a newline; a chunk boundary can land anywhere, including inside one.
    let newline: number
    while ((newline = buffer.indexOf('\n')) !== -1) {
      const line = buffer.slice(0, newline).trim()
      buffer = buffer.slice(newline + 1)
      if (!line.startsWith('data:')) continue
      const payload = line.slice(5).trim()
      if (payload === '[DONE]' || payload === '') continue

      let json: any
      try {
        json = JSON.parse(payload)
      } catch {
        continue
      }
      if (json.error) {
        yield { type: 'error', message: String(json.error.message ?? 'the runtime reported an error') }
        continue
      }
      const choice = json.choices?.[0]
      const text = choice?.delta?.content
      if (typeof text === 'string' && text.length > 0) yield { type: 'delta', text }
      if (choice?.finish_reason) {
        yield {
          type: 'done',
          finishReason: String(choice.finish_reason),
          usage: json.usage ?? { prompt_tokens: 0, completion_tokens: 0, total_tokens: 0 },
        }
      }
    }
  }
}

/**
 * Subscribe to a server-sent-event endpoint, reconnecting when it drops.
 *
 * Returns an unsubscribe function. The reconnect exists because the runtime restarts — a settings
 * change, a crash, the sidecar being replaced — and a monitor that silently stops updating is
 * worse than one that visibly reconnects.
 */
export function subscribe<T>(path: string, onMessage: (value: T) => void, onError?: (error: unknown) => void): () => void {
  let source: EventSource | null = null
  let closed = false
  let retry = 1000

  const connect = () => {
    if (closed) return
    source = new EventSource(`${API_BASE}${path}`)
    source.onmessage = (event) => {
      retry = 1000
      try {
        onMessage(JSON.parse(event.data) as T)
      } catch {
        /* a frame we cannot parse is not a reason to tear down the stream */
      }
    }
    source.onerror = (error) => {
      onError?.(error)
      source?.close()
      if (closed) return
      // Back off to 10s: a runtime that is down stays down for a while, and a tight retry loop
      // is a busy CPU core for as long as the window is open.
      setTimeout(connect, retry)
      retry = Math.min(retry * 2, 10_000)
    }
  }

  connect()
  return () => {
    closed = true
    source?.close()
  }
}
