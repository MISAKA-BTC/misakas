// Markdown rendering for assistant messages.
//
// Two things this does that a bare `react-markdown` does not:
//
// * **A copy button on every code block.** The single most common thing anyone does with a local
//   model's answer is copy the code out of it.
// * **It renders partial markdown while streaming.** Text arrives token by token, which means the
//   renderer sees `` ```py `` with no closing fence for several seconds. `react-markdown` handles
//   that gracefully — the alternative, waiting for the whole message before rendering, is the
//   thing that makes a streaming app feel like a non-streaming one.
//
// Highlighting uses `rehype-highlight`'s default set — highlight.js's `common` grammars, about
// thirty languages. Naming a subset explicitly does not shrink it: the option registers languages
// *in addition to* that set, so the honest way to trim the bundle would be a custom lowlight
// instance, and thirty grammars is not yet worth one.

import { memo, type ReactNode } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import rehypeHighlight from 'rehype-highlight'
import { CopyButton } from './common'

function textOf(node: ReactNode): string {
  if (node === null || node === undefined || typeof node === 'boolean') return ''
  if (typeof node === 'string' || typeof node === 'number') return String(node)
  if (Array.isArray(node)) return node.map(textOf).join('')
  if (typeof node === 'object' && 'props' in (node as any)) return textOf((node as any).props?.children)
  return ''
}

export const Markdown = memo(function Markdown({ children }: { children: string }) {
  return (
    <div className="prose-chat">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        rehypePlugins={[[rehypeHighlight, { detect: true, ignoreMissing: true }]]}
        components={{
          pre({ children }) {
            const code = textOf(children)
            return (
              <div className="group relative">
                <div className="absolute right-2 top-2 opacity-0 transition-opacity group-hover:opacity-100">
                  <CopyButton
                    text={code}
                    label="Copy code"
                    className="btn rounded-md bg-ink-800/80 px-2 py-1 text-ink-200 hover:bg-ink-700"
                  />
                </div>
                <pre>{children}</pre>
              </div>
            )
          },
          // Links open outside the app. A local-LLM window is not a browser, and a model's
          // hallucinated URL should not replace the app with a 404.
          a({ href, children }) {
            return (
              <a href={href} target="_blank" rel="noreferrer noopener">
                {children}
              </a>
            )
          },
        }}
      >
        {children}
      </ReactMarkdown>
    </div>
  )
})
