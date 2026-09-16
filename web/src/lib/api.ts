/** REST + SSE client for the enx API. */

const BASE = '/api'

export interface RoleInfo {
  name: string
  title: string
  summary: string
}

export interface Health {
  ok: boolean
  configured: boolean
  model: string
  provider: string
  workspace: string
  context_window: number
}

export interface SessionMeta {
  id: string
  title: string
  role: string
  created_at: string
  updated_at: string
  message_count: number
}

export interface StoredMessage {
  role: 'system' | 'user' | 'assistant' | 'tool'
  content?: string
  reasoning?: string
  tool_calls?: { id: string; name: string; arguments: string }[]
  tool_call_id?: string
  interrupted?: boolean
  error?: string
  model?: string
  message_id?: string
}

export interface SessionDetail {
  id: string
  title: string
  role: string
  turns: { id: string; created_at: string; message: StoredMessage }[]
}

/** One streamed turn event. Mirrors `enx_core::Event`. */
export type StreamEvent =
  | { type: 'session'; id: string; title: string }
  | { type: 'message_start'; id: string }
  | { type: 'text'; delta: string }
  | { type: 'reasoning'; delta: string }
  | { type: 'tool_call'; id: string; name: string; arguments: string }
  | { type: 'tool_result'; id: string; name: string; content: string; is_error: boolean }
  | { type: 'notice'; message: string }
  | {
      type: 'usage'
      input_tokens: number
      output_tokens: number
      context_tokens: number
      context_window: number
    }
  | { type: 'error'; message: string }
  | { type: 'done'; stop_reason: string }

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`${BASE}${path}`, {
    ...init,
    headers: { 'Content-Type': 'application/json', ...(init?.headers ?? {}) },
  })
  if (!response.ok) {
    const body = await response.text()
    let message = body || `${response.status} ${response.statusText}`
    try {
      const parsed = JSON.parse(body) as { error?: string }
      if (parsed.error) message = parsed.error
    } catch {
      // Not JSON; the raw body is the best message available.
    }
    throw new Error(message)
  }
  if (response.status === 204) return undefined as T
  return (await response.json()) as T
}

export const getHealth = () => request<Health>('/health')
export const getRoles = () => request<{ roles: RoleInfo[] }>('/roles').then((r) => r.roles)
export const listSessions = () =>
  request<{ sessions: SessionMeta[] }>('/sessions').then((r) => r.sessions)
export const getSession = (id: string) =>
  request<{ session: SessionDetail }>(`/sessions/${id}`).then((r) => r.session)
export const deleteSession = (id: string) => request<void>(`/sessions/${id}`, { method: 'DELETE' })
export const interruptRun = (id: string) =>
  request<{ interrupted: boolean }>(`/chat/${id}/interrupt`, { method: 'POST' })

/**
 * Stream one turn. Returns an abort handle; `onEvent` receives every parsed
 * event in order, and `onEnd` fires once the connection closes for any reason.
 */
export function streamChat(
  body: { session_id?: string; message: string; role: string },
  onEvent: (event: StreamEvent) => void,
  onEnd: (error?: Error) => void,
): () => void {
  const controller = new AbortController()

  void (async () => {
    let failure: Error | undefined
    try {
      const response = await fetch(`${BASE}/chat`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
        signal: controller.signal,
      })
      if (!response.ok || !response.body) {
        const body = await response.json().catch(() => ({})) as { error?: string }
        throw new Error(body.error || `stream failed: ${response.status} ${response.statusText}`)
      }
      const reader = response.body.getReader()
      const decoder = new TextDecoder()
      let buffer = ''
      let terminal = false
      for (;;) {
        const { done, value } = await reader.read()
        if (done) break
        // Decode with stream:true so a multi-byte character split across
        // network chunks is not mangled.
        buffer += decoder.decode(value, { stream: true })
        let boundary = buffer.indexOf('\n\n')
        while (boundary !== -1) {
          const frame = buffer.slice(0, boundary)
          buffer = buffer.slice(boundary + 2)
          const data = frame
            .split('\n')
            .filter((line) => line.startsWith('data:'))
            .map((line) => line.slice(5).replace(/^ /, ''))
            .join('\n')
          if (data) {
            const event = JSON.parse(data) as StreamEvent
            if (event.type === 'done' || event.type === 'error') terminal = true
            onEvent(event)
          }
          boundary = buffer.indexOf('\n\n')
        }
      }
      if (!terminal) throw new Error('Connection closed before the turn finished. Reload the session to see saved progress.')
    } catch (error) {
      // An abort is the user pressing stop, not a failure to report.
      if (!(error instanceof DOMException && error.name === 'AbortError')) {
        failure = error instanceof Error ? error : new Error(String(error))
      }
    } finally {
      onEnd(failure)
    }
  })()

  return () => controller.abort()
}
