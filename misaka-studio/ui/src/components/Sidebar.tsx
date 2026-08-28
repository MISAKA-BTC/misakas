// Navigation and conversation history.
//
// The connection dot is not decoration: this window talks to a separate process, and when that
// process is not there every other part of the UI is showing stale data. Saying so in one place,
// permanently, beats a toast that has already faded by the time someone looks up.

import { relativeTime } from '../lib/format'
import { useStudio, type View } from '../store/studio'
import { Icon, type IconName } from './common'

const NAV: { view: View; label: string; icon: IconName }[] = [
  { view: 'chat', label: 'Chat', icon: 'chat' },
  { view: 'models', label: 'Models', icon: 'cube' },
  { view: 'monitor', label: 'Monitor', icon: 'gauge' },
  { view: 'settings', label: 'Settings', icon: 'settings' },
]

export function Sidebar() {
  const view = useStudio((s) => s.view)
  const setView = useStudio((s) => s.setView)
  const conversations = useStudio((s) => s.conversations)
  const activeId = useStudio((s) => s.activeConversationId)
  const newConversation = useStudio((s) => s.newConversation)
  const selectConversation = useStudio((s) => s.selectConversation)
  const deleteConversation = useStudio((s) => s.deleteConversation)
  const connected = useStudio((s) => s.connected)
  const runtime = useStudio((s) => s.runtime)
  const downloads = useStudio((s) => s.downloads)

  const active = downloads.filter((d) => d.status === 'downloading' || d.status === 'verifying').length

  return (
    <aside className="flex h-full w-64 shrink-0 flex-col border-r border-ink-200 bg-white dark:border-ink-800 dark:bg-ink-900">
      <div className="flex items-center gap-2.5 px-4 py-4">
        <div className="flex size-8 items-center justify-center rounded-lg bg-arc-600 text-sm font-bold text-white">M</div>
        <div className="min-w-0">
          <div className="truncate text-sm font-semibold">MISAKA Studio</div>
          <div className="flex items-center gap-1.5 text-[0.7rem] text-ink-500 dark:text-ink-400">
            <span className={`size-1.5 rounded-full ${connected ? 'bg-emerald-500' : 'bg-red-500'}`} />
            {connected ? (runtime?.model_id ? 'model loaded' : 'runtime ready') : 'runtime unreachable'}
          </div>
        </div>
      </div>

      <nav className="px-2">
        {NAV.map((item) => (
          <button
            key={item.view}
            type="button"
            onClick={() => setView(item.view)}
            className={`mb-0.5 flex w-full items-center gap-2.5 rounded-lg px-3 py-2 text-sm transition-colors ${
              view === item.view
                ? 'bg-arc-600/12 font-medium text-arc-700 dark:text-arc-300'
                : 'text-ink-600 hover:bg-ink-100 dark:text-ink-300 dark:hover:bg-ink-800'
            }`}
          >
            <Icon name={item.icon} />
            {item.label}
            {item.view === 'models' && active > 0 && (
              <span className="ml-auto badge bg-arc-600 text-white">{active}</span>
            )}
          </button>
        ))}
      </nav>

      <div className="mt-4 flex items-center justify-between px-4 pb-1">
        <span className="text-[0.7rem] font-semibold uppercase tracking-wide text-ink-500 dark:text-ink-400">Conversations</span>
        <button
          type="button"
          className="btn-ghost px-1.5 py-1"
          title="New chat"
          onClick={() => {
            newConversation()
            setView('chat')
          }}
        >
          <Icon name="plus" className="size-3.5" />
        </button>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-3">
        {conversations.length === 0 && <p className="px-3 py-2 text-xs text-ink-500 dark:text-ink-400">Nothing yet.</p>}
        {conversations.map((conversation) => (
          <div
            key={conversation.id}
            className={`group mb-0.5 flex items-center gap-1 rounded-lg px-3 py-2 text-sm transition-colors ${
              conversation.id === activeId ? 'bg-ink-100 dark:bg-ink-800' : 'hover:bg-ink-100 dark:hover:bg-ink-800'
            }`}
          >
            <button
              type="button"
              className="min-w-0 flex-1 text-left"
              onClick={() => {
                selectConversation(conversation.id)
                setView('chat')
              }}
            >
              <div className="truncate">{conversation.title}</div>
              <div className="text-[0.7rem] text-ink-500 dark:text-ink-400">{relativeTime(conversation.updatedAt / 1000)}</div>
            </button>
            <button
              type="button"
              className="btn-ghost px-1 py-1 opacity-0 transition-opacity group-hover:opacity-100"
              title="Delete conversation"
              onClick={() => deleteConversation(conversation.id)}
            >
              <Icon name="trash" className="size-3.5" />
            </button>
          </div>
        ))}
      </div>
    </aside>
  )
}
