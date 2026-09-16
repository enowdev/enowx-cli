import { ArrowUp, Stop } from '@phosphor-icons/react'
import { RolePicker } from '@/components/chat/RolePicker'
import { Button } from '@/components/ui/button'
import { type RoleInfo } from '@/lib/api'
import { formatCount } from '@/lib/utils'

export function Composer({
  ref,
  value,
  setValue,
  roles,
  role,
  setRole,
  send,
  stop,
  streaming,
  usage,
}: {
  ref: React.RefObject<HTMLTextAreaElement | null>
  value: string
  setValue: (value: string) => void
  roles: RoleInfo[]
  role: string
  setRole: (role: string) => void
  send: () => void
  stop: () => void
  streaming: boolean
  usage: { used: number; window: number }
}) {
  const percentage = usage.window ? Math.min(100, Math.round((usage.used / usage.window) * 100)) : 0
  return (
    <div className="rounded-[var(--radius-xl)] border border-border bg-card p-2 shadow-sm transition-colors focus-within:border-ring">
      <textarea
        ref={ref}
        rows={1}
        value={value}
        onChange={(event) => setValue(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === 'Enter' && !event.shiftKey) {
            event.preventDefault()
            send()
          }
        }}
        placeholder="Ask anything…"
        aria-label="Message"
        className="max-h-48 min-h-12 w-full resize-none bg-transparent px-1.5 py-2 text-sm outline-none placeholder:text-muted-foreground"
      />
      <div className="mt-1 flex items-center gap-1.5">
        <div className="min-w-0 flex-1">
          <RolePicker roles={roles} value={role} onChange={setRole} />
        </div>
        <span className="hidden text-[10px] tabular-nums text-muted-foreground sm:block" title="Context window used">
          {formatCount(usage.used)} / {formatCount(usage.window)} ({percentage}%)
        </span>
        {streaming ? (
          <Button size="icon" variant="destructive" onClick={stop} aria-label="Stop generation" className="rounded-full">
            <Stop weight="fill" />
          </Button>
        ) : (
          <Button size="icon" onClick={send} disabled={!value.trim()} aria-label="Send message" className="rounded-full">
            <ArrowUp weight="bold" />
          </Button>
        )}
      </div>
    </div>
  )
}
