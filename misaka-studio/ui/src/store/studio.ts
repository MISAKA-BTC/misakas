// One store for everything the window shows.
//
// The split that matters: **conversations are the only thing persisted here.** Models, runtime
// status, downloads and metrics all live in the runtime, and caching them in localStorage would
// mean opening the app to a stale model list from three days ago. They are fetched at startup and
// kept current by SSE.
//
// Conversations are the opposite: they exist only in the window. Persisting them locally is what
// makes closing the app safe. (Moving them into the runtime is a later change — the chat history
// is not part of the API this version commits to.)

import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import { api, streamChat } from '../lib/api'
import type { ChatMessage, Conversation, DownloadProgress, ModelView, RuntimeSample, RuntimeStatus, Settings, SystemInfo } from '../lib/types'

export type View = 'chat' | 'models' | 'network' | 'monitor' | 'settings'

/** A message shown to the user about something that just happened. */
export type Toast = { id: string; kind: 'info' | 'error' | 'success'; text: string }

type StudioState = {
  view: View
  system: SystemInfo | null
  settings: Settings | null
  models: ModelView[]
  runtime: RuntimeStatus | null
  downloads: DownloadProgress[]
  sample: RuntimeSample | null
  connected: boolean
  loadingModelId: string | null
  toasts: Toast[]

  conversations: Conversation[]
  activeConversationId: string | null

  setView: (view: View) => void
  toast: (kind: Toast['kind'], text: string) => void
  dismissToast: (id: string) => void

  bootstrap: () => Promise<void>
  refreshModels: () => Promise<void>
  refreshRuntime: () => Promise<void>
  refreshDownloads: () => Promise<void>
  loadModel: (id: string, contextSize?: number) => Promise<void>
  unloadModel: () => Promise<void>
  deleteModel: (id: string) => Promise<void>
  hashModel: (id: string) => Promise<void>
  saveSettings: (settings: Settings) => Promise<void>
  setSample: (sample: RuntimeSample) => void
  setDownload: (progress: DownloadProgress) => void
  setConnected: (connected: boolean) => void

  newConversation: () => string
  selectConversation: (id: string) => void
  deleteConversation: (id: string) => void
  renameConversation: (id: string, title: string) => void
  send: (text: string) => Promise<void>
  regenerate: () => Promise<void>
  editMessage: (messageId: string, content: string) => Promise<void>
  stop: () => void
  isGenerating: () => boolean
}

/** The in-flight generation. Outside the store: it is not state anyone renders, and it must not
 *  be serialised into localStorage. */
let inFlight: AbortController | null = null

const uid = () => Math.random().toString(36).slice(2, 10) + Date.now().toString(36)

function emptyConversation(): Conversation {
  const now = Date.now()
  return { id: uid(), title: 'New chat', createdAt: now, updatedAt: now, modelId: null, messages: [] }
}

/** A conversation's title, from its first user message. */
function deriveTitle(text: string): string {
  const clean = text.trim().replace(/\s+/g, ' ')
  return clean.length > 48 ? `${clean.slice(0, 48)}…` : clean || 'New chat'
}

export const useStudio = create<StudioState>()(
  persist(
    (set, get) => ({
      view: 'chat',
      system: null,
      settings: null,
      models: [],
      runtime: null,
      downloads: [],
      sample: null,
      connected: false,
      loadingModelId: null,
      toasts: [],
      conversations: [],
      activeConversationId: null,

      setView: (view) => set({ view }),

      toast: (kind, text) => {
        const toast: Toast = { id: uid(), kind, text }
        set((s) => ({ toasts: [...s.toasts, toast] }))
        // Errors stay until dismissed; the rest clear themselves. An error that vanishes before
        // it is read is an error that gets reported as "it just doesn't work".
        if (kind !== 'error') setTimeout(() => get().dismissToast(toast.id), 4000)
      },
      dismissToast: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),

      bootstrap: async () => {
        try {
          const [system, settings, models, runtime, downloads] = await Promise.all([
            api.system(),
            api.settings(),
            api.models(),
            api.runtime(),
            api.downloads(),
          ])
          set({ system, settings, models, runtime, downloads, connected: true })
        } catch (error) {
          set({ connected: false })
          get().toast('error', `Cannot reach the MISAKA Runtime: ${(error as Error).message}`)
        }
      },

      refreshModels: async () => {
        try {
          set({ models: await api.refreshModels() })
        } catch (error) {
          get().toast('error', (error as Error).message)
        }
      },

      refreshRuntime: async () => {
        try {
          set({ runtime: await api.runtime(), connected: true })
        } catch {
          set({ connected: false })
        }
      },

      refreshDownloads: async () => {
        try {
          set({ downloads: await api.downloads() })
        } catch {
          /* the downloads panel is not worth a toast */
        }
      },

      loadModel: async (id, contextSize) => {
        set({ loadingModelId: id })
        try {
          const runtime = await api.loadModel(id, contextSize)
          set({ runtime })
          get().toast('success', `${id} loaded in ${((runtime.load_ms ?? 0) / 1000).toFixed(1)}s`)
        } catch (error) {
          get().toast('error', (error as Error).message)
        } finally {
          set({ loadingModelId: null })
        }
      },

      unloadModel: async () => {
        try {
          set({ runtime: await api.unloadModel() })
        } catch (error) {
          get().toast('error', (error as Error).message)
        }
      },

      deleteModel: async (id) => {
        try {
          await api.deleteModel(id)
          await get().refreshModels()
          await get().refreshRuntime()
          get().toast('success', `${id} deleted`)
        } catch (error) {
          get().toast('error', (error as Error).message)
        }
      },

      hashModel: async (id) => {
        try {
          const model = await api.hashModel(id)
          set((s) => ({ models: s.models.map((m) => (m.id === model.id ? model : m)) }))
          await get().refreshRuntime()
          get().toast('success', `${id} hashed — model identity available`)
        } catch (error) {
          get().toast('error', (error as Error).message)
        }
      },

      saveSettings: async (settings) => {
        try {
          const saved = await api.saveSettings(settings)
          set({ settings: saved })
          await Promise.all([get().refreshModels(), get().refreshRuntime()])
          get().toast('success', 'Settings saved')
        } catch (error) {
          get().toast('error', (error as Error).message)
        }
      },

      setSample: (sample) => set({ sample, connected: true }),
      setConnected: (connected) => set({ connected }),

      setDownload: (progress) => {
        set((s) => {
          const downloads = s.downloads.some((d) => d.id === progress.id)
            ? s.downloads.map((d) => (d.id === progress.id ? progress : d))
            : [...s.downloads, progress]
          return { downloads }
        })
        if (progress.status === 'completed') {
          get().toast('success', `${progress.model_id} downloaded`)
          void get().refreshModels()
        }
        if (progress.status === 'failed') get().toast('error', progress.error ?? `${progress.file} failed`)
      },

      newConversation: () => {
        const conversation = emptyConversation()
        set((s) => ({ conversations: [conversation, ...s.conversations], activeConversationId: conversation.id }))
        return conversation.id
      },

      selectConversation: (id) => set({ activeConversationId: id }),

      deleteConversation: (id) =>
        set((s) => {
          const conversations = s.conversations.filter((c) => c.id !== id)
          return {
            conversations,
            activeConversationId: s.activeConversationId === id ? (conversations[0]?.id ?? null) : s.activeConversationId,
          }
        }),

      renameConversation: (id, title) =>
        set((s) => ({ conversations: s.conversations.map((c) => (c.id === id ? { ...c, title } : c)) })),

      send: async (text) => {
        const trimmed = text.trim()
        if (!trimmed) return
        let conversationId = get().activeConversationId
        if (!conversationId || !get().conversations.some((c) => c.id === conversationId)) conversationId = get().newConversation()

        const message: ChatMessage = { id: uid(), role: 'user', content: trimmed }
        set((s) => ({
          conversations: s.conversations.map((c) =>
            c.id === conversationId
              ? {
                  ...c,
                  messages: [...c.messages, message],
                  title: c.messages.length === 0 ? deriveTitle(trimmed) : c.title,
                  updatedAt: Date.now(),
                }
              : c,
          ),
        }))
        await runGeneration(set, get, conversationId)
      },

      regenerate: async () => {
        const conversationId = get().activeConversationId
        if (!conversationId) return
        const conversation = get().conversations.find((c) => c.id === conversationId)
        if (!conversation) return
        // Drop trailing assistant turns, then generate again from the same user message. Editing
        // history in place would leave two answers to one question with no way to tell which the
        // model actually produced.
        const messages = [...conversation.messages]
        while (messages.length > 0 && messages[messages.length - 1]?.role === 'assistant') messages.pop()
        if (messages.length === 0) return
        set((s) => ({ conversations: s.conversations.map((c) => (c.id === conversationId ? { ...c, messages } : c)) }))
        await runGeneration(set, get, conversationId)
      },

      editMessage: async (messageId, content) => {
        const conversationId = get().activeConversationId
        if (!conversationId) return
        const conversation = get().conversations.find((c) => c.id === conversationId)
        if (!conversation) return
        const index = conversation.messages.findIndex((m) => m.id === messageId)
        if (index === -1) return
        // Everything after an edited message answered a question that no longer exists.
        const messages = conversation.messages.slice(0, index + 1).map((m) => (m.id === messageId ? { ...m, content } : m))
        set((s) => ({
          conversations: s.conversations.map((c) => (c.id === conversationId ? { ...c, messages, updatedAt: Date.now() } : c)),
        }))
        await runGeneration(set, get, conversationId)
      },

      stop: () => {
        inFlight?.abort()
        inFlight = null
      },

      isGenerating: () => {
        const id = get().activeConversationId
        const conversation = get().conversations.find((c) => c.id === id)
        return conversation?.messages.some((m) => m.streaming) ?? false
      },
    }),
    {
      name: 'misaka-studio.session',
      // Only the conversation history and the current view. Everything else is the runtime's.
      partialize: (state) => ({
        conversations: state.conversations,
        activeConversationId: state.activeConversationId,
        view: state.view,
      }),
      version: 1,
    },
  ),
)

type Setter = (partial: Partial<StudioState> | ((s: StudioState) => Partial<StudioState>)) => void
type Getter = () => StudioState

/**
 * Run one generation into `conversationId`.
 *
 * Shared by send, regenerate and edit because all three are the same operation: take the
 * conversation as it now stands, ask for the next assistant turn, stream it in.
 */
async function runGeneration(set: Setter, get: Getter, conversationId: string) {
  const state = get()
  const conversation = state.conversations.find((c) => c.id === conversationId)
  if (!conversation) return

  const settings = state.settings
  const modelId = state.runtime?.model_id ?? state.models[0]?.id
  if (!modelId) {
    state.toast('error', 'No model is available. Download one from the Models tab.')
    return
  }

  const systemPrompt = settings?.generation.system_prompt?.trim()
  const history = conversation.messages.map((m) => ({ role: m.role, content: m.content }))
  const messages = systemPrompt ? [{ role: 'system', content: systemPrompt }, ...history] : history

  const assistantId = uid()
  const placeholder: ChatMessage = { id: assistantId, role: 'assistant', content: '', streaming: true }
  set((s) => ({
    conversations: s.conversations.map((c) =>
      c.id === conversationId ? { ...c, messages: [...c.messages, placeholder], modelId, updatedAt: Date.now() } : c,
    ),
  }))

  const update = (patch: Partial<ChatMessage>) =>
    set((s) => ({
      conversations: s.conversations.map((c) =>
        c.id === conversationId
          ? { ...c, messages: c.messages.map((m) => (m.id === assistantId ? { ...m, ...patch } : m)), updatedAt: Date.now() }
          : c,
      ),
    }))

  const controller = new AbortController()
  inFlight = controller
  const startedAt = performance.now()
  let firstTokenAt: number | null = null
  let text = ''

  try {
    const generator = streamChat(
      {
        model: modelId,
        messages,
        temperature: settings?.generation.temperature,
        top_p: settings?.generation.top_p,
        top_k: settings?.generation.top_k,
        min_p: settings?.generation.min_p,
        repeat_penalty: settings?.generation.repeat_penalty,
        max_tokens: settings?.generation.max_tokens,
        seed: settings?.generation.seed ?? null,
      },
      controller.signal,
    )

    for await (const event of generator) {
      if (event.type === 'delta') {
        if (firstTokenAt === null) firstTokenAt = performance.now()
        text += event.text
        update({ content: text })
      } else if (event.type === 'error') {
        update({ error: event.message })
      } else {
        const elapsed = performance.now() - startedAt
        update({
          streaming: false,
          stats: {
            // The runtime's token counts, the window's clock. Neither knows both halves: the
            // engine counts tokens, and only the client knows when the user's request started.
            completionTokens: event.usage.completion_tokens,
            promptTokens: event.usage.prompt_tokens,
            tokensPerSecond: elapsed > 0 ? (event.usage.completion_tokens * 1000) / elapsed : 0,
            timeToFirstTokenMs: firstTokenAt === null ? null : firstTokenAt - startedAt,
            model: modelId,
            finishReason: event.finishReason,
          },
        })
      }
    }
    update({ streaming: false })
  } catch (error) {
    if ((error as Error).name === 'AbortError') {
      // A stopped generation keeps what it produced: the user asked it to stop, not to undo.
      update({ streaming: false, stats: undefined })
    } else {
      update({ streaming: false, error: (error as Error).message })
      get().toast('error', (error as Error).message)
    }
  } finally {
    if (inFlight === controller) inFlight = null
    void get().refreshRuntime()
  }
}
