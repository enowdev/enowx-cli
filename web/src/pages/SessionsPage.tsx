import { useEffect, useState } from 'react'
import { Trash } from '@phosphor-icons/react'
import { Button } from '@/components/ui/button'
import { deleteSession, listSessions, type SessionMeta } from '@/lib/api'
import { timeAgo } from '@/lib/utils'
import { Page, StatePanel } from '@/components/PageState'

export function SessionsPage({ onOpen }: { onOpen: (id: string) => void }) {
  const [sessions, setSessions] = useState<SessionMeta[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  const reload = async () => {
    setLoading(true)
    try {
      setSessions(await listSessions())
      setError('')
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause))
    } finally {
      setLoading(false)
    }
  }
  useEffect(() => { void reload() }, [])

  if (loading) return <Page><StatePanel loading title="Loading sessions" body="Reading saved conversations." /></Page>
  if (error) return <Page><StatePanel title="Sessions unavailable" body={error} action="Retry" onAction={reload} /></Page>
  if (sessions.length === 0) return <Page><StatePanel title="No saved sessions" body="Start a chat and it will appear here." /></Page>

  return (
    <Page title="Sessions" description="Saved conversations, newest first.">
      <div className="divide-y divide-border overflow-hidden rounded-[var(--radius-lg)] border border-border bg-card">
        {sessions.map((session) => (
          <div key={session.id} className="flex items-center gap-3 px-4 py-3">
            <button onClick={() => onOpen(session.id)} className="min-w-0 flex-1 text-left">
              <p className="truncate text-sm font-medium">{session.title || 'New conversation'}</p>
              <p className="mt-1 text-xs text-muted-foreground">
                {session.role} · {session.message_count} messages · {timeAgo(session.updated_at)}
              </p>
            </button>
            <Button
              variant="ghost"
              size="icon"
              aria-label={`Delete ${session.title || 'conversation'}`}
              onClick={async () => {
                try {
                  await deleteSession(session.id)
                  if (localStorage.getItem('enx.session') === session.id) localStorage.removeItem('enx.session')
                  await reload()
                } catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)) }
              }}
            >
              <Trash />
            </Button>
          </div>
        ))}
      </div>
    </Page>
  )
}
