import { Markdown } from '@/components/chat/Markdown'
import { ToolCallCard } from '@/components/chat/ToolCallCard'
import { type ChatMessage, type ActivityState } from '@/components/chat/model'

export function ActivityIndicator({ activity, seconds }: { activity: ActivityState; seconds: number }) {
  return (
    <div role="status" aria-live="polite" className="fade-up flex items-center gap-2.5 text-xs text-muted-foreground">
      <span aria-hidden className="flex items-center gap-1">
        <span className="pulse-dot size-1.5 rounded-full bg-primary" />
        <span className="pulse-dot size-1.5 rounded-full bg-primary [animation-delay:0.15s]" />
        <span className="pulse-dot size-1.5 rounded-full bg-primary [animation-delay:0.3s]" />
      </span>
      <span className="font-medium text-foreground">{activity.label}</span>
      <span className="tabular-nums">{seconds}s</span>
    </div>
  )
}

export function MessageBubble({ message }: { message: ChatMessage }) {
  if (message.role === 'user') {
    return (
      <div className="ml-auto max-w-[88%] rounded-[var(--radius-lg)] bg-primary/12 px-3.5 py-2.5 text-sm leading-relaxed sm:max-w-[78%]">
        {message.content}
      </div>
    )
  }
  return (
    <div className="min-w-0 space-y-2.5 text-[13px] leading-relaxed">
      {(message.segments ?? []).map((segment, index) => {
        if (segment.kind === 'tool') {
          return <ToolCallCard key={segment.call.id} call={segment.call} />
        }
        if (segment.kind === 'reasoning') {
          return (
            <details key={index} className="rounded-[var(--radius-sm)] border border-border bg-muted/30 px-3 py-2">
              <summary className="cursor-pointer text-xs text-muted-foreground">Reasoning</summary>
              <p className="mt-2 whitespace-pre-wrap text-xs italic text-muted-foreground">{segment.text}</p>
            </details>
          )
        }
        return <Markdown key={index} content={segment.text} />
      })}
      {!message.content && !(message.segments?.length) ? <span className="sr-only">Assistant is working</span> : null}
    </div>
  )
}
