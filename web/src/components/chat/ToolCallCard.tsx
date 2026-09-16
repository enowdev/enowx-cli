import { memo, useMemo, useState } from 'react'
import {
  CaretDown,
  CaretRight,
  CheckCircle,
  CircleNotch,
  FileText,
  Globe,
  ListChecks,
  MagnifyingGlass,
  PencilSimple,
  FilePlus,
  Terminal,
  Wrench,
} from '@phosphor-icons/react'
import { cn } from '@/lib/utils'

export interface ToolCallView {
  id: string
  name: string
  args: string
  result?: string
  isError?: boolean
  running?: boolean
}

type IconType = React.ComponentType<{ className?: string; weight?: 'regular' | 'fill' }>

// Verb plus icon per tool, so a call reads as an action rather than a raw
// function name. Names match the Rust tool registry.
const TOOL_META: Record<string, { label: string; Icon: IconType }> = {
  read: { label: 'read', Icon: FileText },
  write: { label: 'create', Icon: FilePlus },
  edit: { label: 'edit', Icon: PencilSimple },
  glob: { label: 'find', Icon: MagnifyingGlass },
  grep: { label: 'search', Icon: MagnifyingGlass },
  bash: { label: 'run', Icon: Terminal },
  fetch: { label: 'fetch', Icon: Globe },
  todo: { label: 'tasks', Icon: ListChecks },
}
const FILE_TOOLS: Record<string, true> = { read: true, write: true, edit: true }

/** One-line summary of a call, from its arguments. */
function summarize(name: string, args: Record<string, unknown>): string {
  const value = (key: string) => (typeof args[key] === 'string' ? (args[key] as string) : '')
  switch (name) {
    case 'bash':
      return value('command')
    case 'grep':
    case 'glob':
      return value('pattern')
    case 'fetch':
      return value('url')
    case 'todo':
      return value('op')
    default:
      return Object.entries(args)
        .slice(0, 2)
        .map(([key, raw]) => `${key}=${String(raw).slice(0, 40)}`)
        .join(' ')
  }
}

// Memoised on the call object: a streaming turn keeps every call on one
// assistant message, so without this each new call re-renders all finished cards.
export const ToolCallCard = memo(function ToolCallCard({ call }: { call: ToolCallView }) {
  const [open, setOpen] = useState(false)

  const args = useMemo<Record<string, unknown>>(() => {
    try {
      const parsed = JSON.parse(call.args || '{}')
      return parsed && typeof parsed === 'object' ? (parsed as Record<string, unknown>) : {}
    } catch {
      return {}
    }
  }, [call.args])
  const path = FILE_TOOLS[call.name] && typeof args.path === 'string' ? args.path : ''
  const meta = TOOL_META[call.name] ?? { label: call.name, Icon: Wrench }
  const fileName = path ? path.split('/').pop() || path : ''
  const parentPath = path.includes('/') ? path.slice(0, path.lastIndexOf('/')) : ''
  const summary = path ? '' : summarize(call.name, args)
  const output = call.result ?? ''
  const canExpand = (call.args && call.args !== '{}') || Boolean(output)

  return (
    <div
      className={cn(
        'overflow-hidden rounded-[var(--radius-sm)] border bg-card',
        call.isError ? 'border-destructive/50' : 'border-border',
      )}
    >
      <button
        type="button"
        aria-expanded={canExpand ? open : undefined}
        onClick={() => canExpand && setOpen((value) => !value)}
        className={cn(
          'flex w-full items-start gap-2 px-2.5 py-2 text-left',
          canExpand ? 'cursor-pointer hover:bg-muted/40' : 'cursor-default',
        )}
      >
        <span className="mt-0.5 shrink-0 text-muted-foreground">
          {canExpand ? (
            open ? (
              <CaretDown className="size-3.5" />
            ) : (
              <CaretRight className="size-3.5" />
            )
          ) : (
            <meta.Icon className="size-3.5" />
          )}
        </span>

        {path ? (
          <span className="flex min-w-0 flex-1 flex-col leading-tight" title={path}>
            <span className="truncate font-mono text-[11px] font-medium text-foreground">
              {fileName}
            </span>
            {parentPath ? (
              <span className="truncate font-mono text-[10px] text-muted-foreground">
                {parentPath}/
              </span>
            ) : null}
          </span>
        ) : (
          <span className="flex min-w-0 flex-1 flex-col gap-0.5 leading-tight">
            <span className="truncate font-mono text-[11px] font-medium text-foreground">
              {meta.label}
            </span>
            {summary ? (
              <span className="truncate font-mono text-[10px] text-muted-foreground">{summary}</span>
            ) : null}
          </span>
        )}

        <span className="mt-0.5 shrink-0">
          {call.isError ? (
            <span className="text-[10px] font-semibold text-destructive">failed</span>
          ) : call.running ? (
            <span className="flex items-center gap-1 text-[10px] text-muted-foreground">
              <CircleNotch className="size-3.5 animate-spin" />
              running
            </span>
          ) : call.result !== undefined ? (
            <CheckCircle className="size-3.5 text-[var(--success)]" weight="fill" />
          ) : null}
        </span>
      </button>

      {open && canExpand ? (
        <div className="space-y-2 border-t border-border px-3 py-2">
          {call.args && call.args !== '{}' ? (
            <div>
              <p className="mb-1 text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
                Arguments
              </p>
              <pre className="max-h-48 overflow-auto whitespace-pre-wrap break-words rounded bg-muted/50 p-2 font-mono text-[11px]">
                {prettyJSON(call.args)}
              </pre>
            </div>
          ) : null}
          {output ? (
            <div>
              <p className="mb-1 text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
                Result
              </p>
              <pre
                className={cn(
                  'max-h-80 overflow-auto whitespace-pre-wrap break-words rounded bg-muted/50 p-2 font-mono text-[11px]',
                  call.isError && 'text-destructive',
                )}
              >
                {output}
              </pre>
            </div>
          ) : null}
        </div>
      ) : null}
    </div>
  )
})

function prettyJSON(raw: string): string {
  try {
    return JSON.stringify(JSON.parse(raw), null, 2)
  } catch {
    return raw
  }
}
