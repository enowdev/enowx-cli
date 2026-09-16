import { type ToolCallView } from '@/components/chat/ToolCallCard'
import { type SessionDetail } from '@/lib/api'

interface SegmentText {
  kind: 'text' | 'reasoning'
  text: string
}
interface SegmentTool {
  kind: 'tool'
  call: ToolCallView
}
type Segment = SegmentText | SegmentTool
export type ActivityState =
  | { kind: 'idle'; label: 'Idle' }
  | { kind: 'waiting'; label: 'Waiting for model' | 'Iterating with results' }
  | { kind: 'thinking'; label: 'Thinking' }
  | { kind: 'writing'; label: 'Writing response' }
  | { kind: 'tool'; label: string }

export const IDLE_ACTIVITY: ActivityState = { kind: 'idle', label: 'Idle' }
export interface ChatMessage {
  id: string
  role: 'user' | 'assistant'
  content: string
  segments?: Segment[]
}
export function emptyAssistant(id: string = crypto.randomUUID()): ChatMessage {
  return { id, role: 'assistant', content: '', segments: [] }
}

export function appendSegment(message: ChatMessage, kind: 'text' | 'reasoning', delta: string): ChatMessage {
  const segments = [...(message.segments ?? [])]
  const last = segments.at(-1)
  if (last?.kind === kind) {
    segments[segments.length - 1] = { ...last, text: last.text + delta }
  } else {
    segments.push({ kind, text: delta })
  }
  return { ...message, content: kind === 'text' ? message.content + delta : message.content, segments }
}

export function updateTool(message: ChatMessage, id: string, patch: Partial<ToolCallView>): ChatMessage {
  return {
    ...message,
    segments: (message.segments ?? []).map((segment) =>
      segment.kind === 'tool' && segment.call.id === id
        ? { kind: 'tool', call: { ...segment.call, ...patch } }
        : segment,
    ),
  }
}

export function hydrate(session: SessionDetail): ChatMessage[] {
  const messages: ChatMessage[] = []
  const assistantByCall: Record<string, number> = {}
  for (const turn of session.turns) {
    const stored = turn.message
    if (stored.role === 'user') {
      messages.push({ id: turn.id, role: 'user', content: stored.content ?? '' })
    } else if (stored.role === 'assistant') {
      const message = emptyAssistant(turn.id)
      if (stored.reasoning) message.segments!.push({ kind: 'reasoning', text: stored.reasoning })
      if (stored.content) {
        message.content = stored.content
        message.segments!.push({ kind: 'text', text: stored.content })
      }
      for (const call of stored.tool_calls ?? []) {
        assistantByCall[call.id] = messages.length
        message.segments!.push({
          kind: 'tool',
          call: { id: call.id, name: call.name, args: call.arguments, running: true },
        })
      }
      if (stored.error) {
        message.segments!.push({ kind: 'text', text: `\n\n> ${stored.error}` })
      }
      messages.push(message)
    } else if (stored.role === 'tool' && stored.tool_call_id) {
      const index = assistantByCall[stored.tool_call_id]
      if (index !== undefined) {
        messages[index] = updateTool(messages[index], stored.tool_call_id, {
          result: stored.content ?? '',
          running: false,
          isError: Boolean(stored.error),
        })
      }
    }
  }
  return messages
}
