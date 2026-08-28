// The chat surface.
//
// Three behaviours here are worth more than they look:
//
// * **Auto-scroll that stops when you scroll up.** A view that yanks itself back to the bottom
//   while someone is reading the middle of a long answer is unusable; one that never follows the
//   stream is equally so. So it follows only while the user is already at the bottom.
// * **Editing a message re-runs from that point.** The turns after it answered a question that no
//   longer exists, so they go. The alternative — leaving them — produces a transcript the model
//   never actually produced.
// * **Stop keeps what was generated.** The user asked it to stop, not to undo.

import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import { duration } from '../lib/format'
import type { ChatMessage } from '../lib/types'
import { useStudio } from '../store/studio'
import { CopyButton, EmptyState, Icon, Spinner } from './common'
import { Markdown } from './Markdown'
import { ModelBar } from './ModelBar'

export function ChatView() {
  const conversations = useStudio((s) => s.conversations)
  const activeId = useStudio((s) => s.activeConversationId)
  const send = useStudio((s) => s.send)
  const stop = useStudio((s) => s.stop)
  const regenerate = useStudio((s) => s.regenerate)
  const editMessage = useStudio((s) => s.editMessage)
  const runtime = useStudio((s) => s.runtime)
  const models = useStudio((s) => s.models)

  const conversation = conversations.find((c) => c.id === activeId) ?? null
  const messages = conversation?.messages ?? []
  const generating = messages.some((m) => m.streaming)

  const [draft, setDraft] = useState('')
  const [editingId, setEditingId] = useState<string | null>(null)
  const scrollRef = useRef<HTMLDivElement>(null)
  const followRef = useRef(true)

  // Follow the stream only while the user is at the bottom.
  useLayoutEffect(() => {
    const element = scrollRef.current
    if (!element || !followRef.current) return
    element.scrollTop = element.scrollHeight
  }, [messages])

  const onScroll = () => {
    const element = scrollRef.current
    if (!element) return
    followRef.current = element.scrollHeight - element.scrollTop - element.clientHeight < 80
  }

  const submit = async () => {
    const text = draft
    setDraft('')
    followRef.current = true
    await send(text)
  }

  return (
    <div className="flex h-full min-w-0 flex-col">
      <ModelBar />

      <div ref={scrollRef} onScroll={onScroll} className="min-h-0 flex-1 overflow-y-auto">
        {messages.length === 0 ? (
          <EmptyState icon="chat" title={runtime?.model_id ? `Chatting with ${runtime.model_id}` : 'No model loaded'}>
            {models.length === 0 ? (
              <>
                No models are installed yet. Open <strong>Models → Discover</strong> to find one on Hugging Face — the list shows what
                fits this machine before you download anything.
              </>
            ) : runtime?.model_id ? (
              <>Ask anything. Everything runs on this machine; nothing leaves it.</>
            ) : (
              <>Pick a model above to load it, or just send a message — the runtime will load the first one for you.</>
            )}
          </EmptyState>
        ) : (
          <div className="mx-auto w-full max-w-3xl px-4 py-6">
            {messages.map((message, index) => (
              <Message
                key={message.id}
                message={message}
                editing={editingId === message.id}
                onEdit={() => setEditingId(message.id)}
                onCancelEdit={() => setEditingId(null)}
                onSaveEdit={async (content) => {
                  setEditingId(null)
                  followRef.current = true
                  await editMessage(message.id, content)
                }}
                onRegenerate={
                  message.role === 'assistant' && index === messages.length - 1 && !generating
                    ? async () => {
                        followRef.current = true
                        await regenerate()
                      }
                    : undefined
                }
              />
            ))}
          </div>
        )}
      </div>

      <div className="border-t border-ink-200 bg-ink-50/80 px-4 py-3 backdrop-blur dark:border-ink-800 dark:bg-ink-950/80">
        <div className="mx-auto flex w-full max-w-3xl items-end gap-2">
          <textarea
            className="input max-h-52 min-h-[2.75rem] resize-y py-2.5"
            rows={1}
            placeholder={generating ? 'Generating…' : 'Send a message  (Enter to send, Shift+Enter for a new line)'}
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) {
                event.preventDefault()
                if (!generating) void submit()
              }
            }}
          />
          {generating ? (
            <button type="button" className="btn-outline h-11 px-4" onClick={stop} title="Stop generating">
              <Icon name="stop" />
              Stop
            </button>
          ) : (
            <button type="button" className="btn-primary h-11 px-4" onClick={() => void submit()} disabled={!draft.trim()}>
              <Icon name="send" />
              Send
            </button>
          )}
        </div>
      </div>
    </div>
  )
}

function Message({
  message,
  editing,
  onEdit,
  onCancelEdit,
  onSaveEdit,
  onRegenerate,
}: {
  message: ChatMessage
  editing: boolean
  onEdit: () => void
  onCancelEdit: () => void
  onSaveEdit: (content: string) => void
  onRegenerate?: () => void
}) {
  const [draft, setDraft] = useState(message.content)
  useEffect(() => setDraft(message.content), [message.content, editing])

  const isUser = message.role === 'user'

  return (
    <div className={`group mb-6 flex gap-3 ${isUser ? 'justify-end' : ''}`}>
      {!isUser && (
        <div className="mt-1 flex size-7 shrink-0 items-center justify-center rounded-lg bg-arc-600/15 text-[0.65rem] font-bold text-arc-700 dark:text-arc-300">
          MS
        </div>
      )}

      <div className={`min-w-0 ${isUser ? 'max-w-[85%]' : 'flex-1'}`}>
        {editing ? (
          <div className="card p-3">
            <textarea className="input min-h-24 resize-y" value={draft} onChange={(event) => setDraft(event.target.value)} />
            <div className="mt-2 flex justify-end gap-2">
              <button type="button" className="btn-ghost" onClick={onCancelEdit}>
                Cancel
              </button>
              <button type="button" className="btn-primary" onClick={() => onSaveEdit(draft)} disabled={!draft.trim()}>
                Save and re-run
              </button>
            </div>
          </div>
        ) : (
          <div
            className={
              isUser
                ? 'rounded-2xl rounded-br-md bg-arc-600 px-4 py-2.5 text-white'
                : 'rounded-2xl rounded-bl-md bg-white px-4 py-3 shadow-sm dark:bg-ink-900'
            }
          >
            {isUser ? (
              <p className="whitespace-pre-wrap text-[0.94rem] leading-relaxed">{message.content}</p>
            ) : message.content ? (
              <Markdown>{message.content}</Markdown>
            ) : message.streaming ? (
              <div className="flex items-center gap-2 py-1 text-sm text-ink-500 dark:text-ink-400">
                <Spinner className="size-3.5" />
                Thinking…
              </div>
            ) : null}

            {message.error && (
              <p className="mt-2 flex items-start gap-2 rounded-lg bg-red-50 p-2 text-xs text-red-700 dark:bg-red-950/40 dark:text-red-300">
                <Icon name="warning" className="mt-0.5 size-3.5 shrink-0" />
                {message.error}
              </p>
            )}
          </div>
        )}

        {!editing && (
          <div className={`mt-1.5 flex items-center gap-1 text-xs text-ink-500 opacity-0 transition-opacity group-hover:opacity-100 dark:text-ink-400 ${isUser ? 'justify-end' : ''}`}>
            {message.stats && (
              <span className="mono mr-1 opacity-100" title={`${message.stats.completionTokens} tokens · ${message.stats.finishReason}`}>
                {message.stats.tokensPerSecond.toFixed(1)} tok/s
                {message.stats.timeToFirstTokenMs !== null && ` · first token ${duration(message.stats.timeToFirstTokenMs)}`}
              </span>
            )}
            {message.content && <CopyButton text={message.content} label="Copy message" className="btn-ghost px-1.5 py-1" />}
            {isUser && (
              <button type="button" className="btn-ghost px-1.5 py-1" onClick={onEdit} title="Edit and re-run">
                <Icon name="edit" className="size-3.5" />
              </button>
            )}
            {onRegenerate && (
              <button type="button" className="btn-ghost px-1.5 py-1" onClick={onRegenerate} title="Regenerate">
                <Icon name="refresh" className="size-3.5" />
              </button>
            )}
          </div>
        )}
      </div>
    </div>
  )
}
