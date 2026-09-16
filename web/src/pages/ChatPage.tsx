import { useEffect, useRef, useState } from 'react'
import { Plus } from '@phosphor-icons/react'
import { Button } from '@/components/ui/button'
import { getRoles, getSession, interruptRun, streamChat, type Health, type RoleInfo, type StreamEvent } from '@/lib/api'
import { StatePanel, ErrorBanner } from '@/components/PageState'
import { emptyAssistant, appendSegment, updateTool, hydrate, IDLE_ACTIVITY, type ChatMessage, type ActivityState } from '@/components/chat/model'
import { Composer } from '@/components/chat/Composer'
import { MessageBubble, ActivityIndicator } from '@/components/chat/MessageBubble'

export function ChatPage({ health, onOpenProviders }: { health: Health; onOpenProviders: () => void }) {
  const [roles, setRoles] = useState<RoleInfo[]>([])
  const [role, setRole] = useState(() => localStorage.getItem('enx.role') ?? 'orchestrator')
  const [sessionId, setSessionId] = useState<string | undefined>(() => localStorage.getItem('enx.session') ?? undefined)
  const [title, setTitle] = useState('')
  const [messages, setMessages] = useState<ChatMessage[]>([])
  const [input, setInput] = useState('')
  const [loading, setLoading] = useState(Boolean(sessionId))
  const [streaming, setStreaming] = useState(false)
  const [activity, setActivity] = useState<ActivityState>(IDLE_ACTIVITY)
  const [activitySeconds, setActivitySeconds] = useState(0)
  const [error, setError] = useState('')
  const [usage, setUsage] = useState({ used: 0, window: health.context_window ?? 0 })
  const abortRef = useRef<(() => void) | null>(null)
  const transcriptRef = useRef<HTMLDivElement>(null)
  const inputRef = useRef<HTMLTextAreaElement>(null)
  const localSessionRef = useRef<string | undefined>(undefined)
  const sessionRef = useRef(sessionId)
  const pinnedRef = useRef(true)

  useEffect(() => {
    void getRoles()
      .then((items) => {
        setRoles(items)
        if (!items.some((item) => item.name === role) && items[0]) setRole(items[0].name)
      })
      .catch((cause) => setError(cause instanceof Error ? cause.message : String(cause)))
  }, [])

  useEffect(() => {
    if (!sessionId || localSessionRef.current === sessionId) {
      setLoading(false)
      return
    }
    setLoading(true)
    void getSession(sessionId)
      .then((session) => {
        setTitle(session.title)
        setRole(session.role)
        setMessages(hydrate(session))
        setError('')
      })
      .catch((cause) => {
        localStorage.removeItem('enx.session')
        setSessionId(undefined)
        setMessages([])
        setError(cause instanceof Error ? cause.message : String(cause))
      })
      .finally(() => setLoading(false))
  }, [sessionId])

  useEffect(() => {
    if (pinnedRef.current) transcriptRef.current?.scrollTo({ top: transcriptRef.current.scrollHeight })
  }, [messages, streaming])

  useEffect(() => {
    if (!streaming) {
      setActivitySeconds(0)
      return
    }
    const started = Date.now()
    const update = () => setActivitySeconds(Math.floor((Date.now() - started) / 1000))
    update()
    const timer = window.setInterval(update, 1000)
    return () => window.clearInterval(timer)
  }, [streaming])

  useEffect(() => () => {
    if (abortRef.current && sessionRef.current) void interruptRun(sessionRef.current).catch(() => {})
    abortRef.current?.()
  }, [])

  const updateAssistant = (id: string, fn: (message: ChatMessage) => ChatMessage) => {
    setMessages((current) => current.map((message) => (message.id === id ? fn(message) : message)))
  }

  const applyEvent = (event: StreamEvent, assistantRef: { current: string }) => {
    switch (event.type) {
      case 'session':
        localSessionRef.current = event.id
        sessionRef.current = event.id
        setSessionId(event.id)
        setTitle(event.title)
        localStorage.setItem('enx.session', event.id)
        break
      case 'message_start': {
        setActivity({ kind: 'waiting', label: 'Waiting for model' })
        assistantRef.current = event.id
        setMessages((current) => {
          const active = current.find((message) => message.id === event.id)
          return active ? current : [...current, emptyAssistant(event.id)]
        })
        break
      }
      case 'text':
        setActivity({ kind: 'writing', label: 'Writing response' })
        updateAssistant(assistantRef.current, (message) => appendSegment(message, 'text', event.delta))
        break
      case 'reasoning':
        setActivity({ kind: 'thinking', label: 'Thinking' })
        updateAssistant(assistantRef.current, (message) => appendSegment(message, 'reasoning', event.delta))
        break
      case 'tool_call':
        setActivity({ kind: 'tool', label: `Running ${event.name}` })
        updateAssistant(assistantRef.current, (message) => ({
          ...message,
          segments: [
            ...(message.segments ?? []),
            { kind: 'tool', call: { id: event.id, name: event.name, args: event.arguments, running: true } },
          ],
        }))
        break
      case 'tool_result':
        updateAssistant(assistantRef.current, (message) =>
          updateTool(message, event.id, { result: event.content, running: false, isError: event.is_error }),
        )
        setActivity({ kind: 'waiting', label: 'Iterating with results' })
        break
      case 'notice':
        setError(event.message)
        break
      case 'usage':
        setUsage({ used: event.context_tokens, window: event.context_window })
        break
      case 'error':
        setActivity(IDLE_ACTIVITY)
        setError(event.message)
        break
      case 'done':
        setActivity(IDLE_ACTIVITY)
        if (event.stop_reason !== 'stop') setError(`Turn stopped: ${event.stop_reason}`)
        break
    }
  }

  const send = () => {
    const prompt = input.trim()
    if (!prompt || streaming) return
    const user: ChatMessage = { id: crypto.randomUUID(), role: 'user', content: prompt }
    const assistantRef = { current: '' }
    setMessages((current) => [...current, user])
    setInput('')
    setError('')
    setStreaming(true)
    setActivity({ kind: 'waiting', label: 'Waiting for model' })
    abortRef.current = streamChat(
      { session_id: sessionId, message: prompt, role },
      (event) => applyEvent(event, assistantRef),
      (cause) => {
        abortRef.current = null
        setStreaming(false)
        setActivity(IDLE_ACTIVITY)
        if (cause) setError(cause.message)
      },
    )
  }

  const stop = async () => {
    setActivity({ kind: 'tool', label: 'Stopping' })
    try {
      if (sessionRef.current) await interruptRun(sessionRef.current)
    } catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)) }
  }

  const fresh = () => {
    if (streaming) return
    abortRef.current?.()
    sessionRef.current = undefined
    localSessionRef.current = undefined
    localStorage.removeItem('enx.session')
    setSessionId(undefined)
    setMessages([])
    setTitle('')
    setError('')
    inputRef.current?.focus()
  }

  const empty = !loading && messages.length === 0

  useEffect(() => {
    if (!loading) inputRef.current?.focus()
  }, [loading, empty])

  return (
    <div className="flex h-[calc(100dvh-3.5rem)] flex-col lg:h-dvh">
      {!empty ? (
        <div className="flex items-center gap-3 border-b border-border px-4 py-3 sm:px-6">
          <div className="min-w-0 flex-1">
            <p className="truncate text-sm font-medium">{title || 'New conversation'}</p>
            <p className="truncate text-[11px] text-muted-foreground">
              {sessionId ? `Session ${sessionId.slice(0, 12)}` : health.workspace}
            </p>
          </div>
          <Button variant="outline" size="sm" onClick={fresh} disabled={streaming} aria-label="New conversation" className="gap-1.5">
            <Plus /> <span className="hidden sm:inline">New</span>
          </Button>
        </div>
      ) : null}

      {loading ? (
        <StatePanel loading title="Loading session" body="Restoring messages and tool results." />
      ) : empty ? (
        <div className="flex flex-1 items-center justify-center px-4 py-10 sm:px-6">
          <div className="w-full max-w-3xl space-y-6">
            <div className="space-y-3 text-center">
              <img src="/enx.svg" alt="" aria-hidden className="mx-auto size-16" />
              <h1 className="text-2xl font-semibold tracking-tight sm:text-3xl">Start a conversation</h1>
              <p className="mx-auto max-w-lg text-sm leading-relaxed text-muted-foreground">
                Enx reads and edits this workspace, runs commands, and researches the web through its Rust core.
              </p>
            </div>
            {!health.configured ? (
              <StatePanel
                title="Provider setup required"
                body="Choose an OpenAI-compatible endpoint and model before the first turn."
                action="Configure provider"
                onAction={onOpenProviders}
              />
            ) : null}
            <Composer
              ref={inputRef}
              value={input}
              setValue={setInput}
              roles={roles}
              role={role}
              setRole={(next) => {
                setRole(next)
                localStorage.setItem('enx.role', next)
              }}
              send={send}
              stop={stop}
              streaming={streaming}
              usage={usage}
            />
            {error ? <ErrorBanner message={error} /> : null}
          </div>
        </div>
      ) : (
        <>
          <div
            ref={transcriptRef}
            className="min-h-0 flex-1 overflow-y-auto"
            aria-live="polite"
            onScroll={(event) => {
              const el = event.currentTarget
              pinnedRef.current = el.scrollHeight - el.scrollTop - el.clientHeight < 80
            }}
          >
            <div className="mx-auto w-full max-w-3xl space-y-5 px-4 py-6 sm:px-6">
              {messages.map((message) => (
                <MessageBubble key={message.id} message={message} />
              ))}
              {streaming ? <ActivityIndicator activity={activity} seconds={activitySeconds} /> : null}
            </div>
          </div>
          <div className="bg-gradient-to-t from-background via-background to-transparent px-4 pb-[max(1.5rem,env(safe-area-inset-bottom))] pt-3 sm:px-6 sm:pb-8">
            <div className="mx-auto w-full max-w-3xl space-y-2">
              {error ? <ErrorBanner message={error} /> : null}
              <Composer
                ref={inputRef}
                value={input}
                setValue={setInput}
                roles={roles}
                role={role}
                setRole={(next) => {
                  setRole(next)
                  localStorage.setItem('enx.role', next)
                }}
                send={send}
                stop={stop}
                streaming={streaming}
                usage={usage}
              />
            </div>
          </div>
        </>
      )}
    </div>
  )
}
