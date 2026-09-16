import { useEffect, useRef, useState } from 'react'
import { CaretDown, Check, UsersThree } from '@phosphor-icons/react'
import type { RoleInfo } from '@/lib/api'
import { cn } from '@/lib/utils'

export function RolePicker({
  roles,
  value,
  onChange,
}: {
  roles: RoleInfo[]
  value: string
  onChange: (role: string) => void
}) {
  const [open, setOpen] = useState(false)
  const ref = useRef<HTMLDivElement>(null)
  const current = roles.find((role) => role.name === value) ?? roles[0]

  useEffect(() => {
    if (!open) return
    const close = (event: MouseEvent) => {
      if (ref.current && !ref.current.contains(event.target as Node)) setOpen(false)
    }
    const escape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setOpen(false)
    }
    document.addEventListener('mousedown', close)
    document.addEventListener('keydown', escape)
    return () => {
      document.removeEventListener('mousedown', close)
      document.removeEventListener('keydown', escape)
    }
  }, [open])

  return (
    <div ref={ref} className="relative">
      <button
        type="button"
        aria-label={`Agent role: ${current?.title ?? 'Orchestrator'}`}
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
        className="flex h-8 items-center gap-1.5 rounded-[var(--radius-md)] border border-border bg-card px-2.5 text-xs transition-colors hover:border-primary/40 focus-visible:border-ring"
      >
        <UsersThree className="size-3.5 shrink-0 text-muted-foreground" />
        <span className="hidden max-w-28 truncate sm:inline">{current?.title ?? 'Orchestrator'}</span>
        <CaretDown className="size-3 shrink-0 text-muted-foreground" />
      </button>

      {open ? (
        <div
          role="listbox"
          aria-label="Agent role"
          className="absolute bottom-full left-0 z-30 mb-2 w-72 max-w-[calc(100vw-3rem)] rounded-[var(--radius-lg)] border border-border bg-card p-1 shadow-lg"
        >
          <p className="px-2.5 pb-1 pt-2 text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
            Agent role
          </p>
          {roles.map((role) => (
            <button
              key={role.name}
              role="option"
              aria-selected={role.name === value}
              onClick={() => {
                onChange(role.name)
                setOpen(false)
              }}
              className={cn(
                'flex w-full items-start gap-2 rounded-[var(--radius-sm)] px-2.5 py-2 text-left transition-colors hover:bg-muted',
                role.name === value && 'bg-primary/5',
              )}
            >
              {role.name === value ? (
                <Check className="mt-0.5 size-3.5 shrink-0 text-primary" />
              ) : (
                <span className="w-3.5 shrink-0" />
              )}
              <span className="min-w-0">
                <span className="block text-xs font-medium">{role.title}</span>
                <span className="block text-[11px] leading-relaxed text-muted-foreground">
                  {role.summary}
                </span>
              </span>
            </button>
          ))}
        </div>
      ) : null}
    </div>
  )
}
