import { useEffect, useState } from 'react'
import { Check, ChatCircleDots, Gear, List, Moon, SidebarSimple, Sun, TerminalWindow, X } from '@phosphor-icons/react'
import { Button } from '@/components/ui/button'
import { getHealth, type Health } from '@/lib/api'
import { cn } from '@/lib/utils'
import { StatePanel } from '@/components/PageState'
import { ChatPage } from '@/pages/ChatPage'
import { SessionsPage } from '@/pages/SessionsPage'
import { ProvidersPage } from '@/pages/ProvidersPage'
import { ToolsPage } from '@/pages/ToolsPage'

type Page = 'chat' | 'sessions' | 'providers' | 'tools'

const NAV: { id: Page; label: string; Icon: typeof ChatCircleDots }[] = [
  { id: 'chat', label: 'Chat', Icon: ChatCircleDots },
  { id: 'sessions', label: 'Sessions', Icon: SidebarSimple },
  { id: 'providers', label: 'Providers', Icon: Gear },
  { id: 'tools', label: 'Tools', Icon: TerminalWindow },
]
export default function App() {
  const [page, setPage] = useState<Page>('chat')
  const [drawerOpen, setDrawerOpen] = useState(false)
  const [theme, setTheme] = useState<'dark' | 'light'>(() =>
    localStorage.getItem('enx.theme') === 'light' ? 'light' : 'dark',
  )
  const [health, setHealth] = useState<Health>()
  const [healthError, setHealthError] = useState('')

  useEffect(() => {
    document.documentElement.classList.toggle('dark', theme === 'dark')
    document.documentElement.style.colorScheme = theme
    localStorage.setItem('enx.theme', theme)
  }, [theme])

  const refreshHealth = async () => {
    try {
      setHealth(await getHealth())
      setHealthError('')
    } catch (error) {
      setHealthError(error instanceof Error ? error.message : String(error))
    }
  }

  useEffect(() => {
    void refreshHealth()
  }, [])

  useEffect(() => {
    if (!drawerOpen) return
    const close = (event: KeyboardEvent) => { if (event.key === 'Escape') setDrawerOpen(false) }
    document.addEventListener('keydown', close)
    return () => document.removeEventListener('keydown', close)
  }, [drawerOpen])

  return (
    <div className="flex min-h-dvh bg-background text-foreground">
      <aside className="sticky top-0 hidden h-dvh w-60 shrink-0 flex-col border-r border-border bg-sidebar lg:flex">
        <Brand />
        <Navigation page={page} setPage={setPage} />
        <SidebarFooter health={health} theme={theme} setTheme={setTheme} />
      </aside>

      {drawerOpen ? (
        <div className="fixed inset-0 z-50 lg:hidden">
          <button
            aria-label="Close navigation"
            className="absolute inset-0 bg-black/60 backdrop-blur-[2px]"
            onClick={() => setDrawerOpen(false)}
          />
          <aside className="absolute inset-y-0 left-0 flex w-[17rem] max-w-[85vw] flex-col bg-sidebar shadow-2xl fade-up">
            <div className="flex items-center justify-between">
              <Brand />
              <Button
                variant="ghost"
                size="icon"
                onClick={() => setDrawerOpen(false)}
                aria-label="Close navigation"
                className="mr-2"
              >
                <X />
              </Button>
            </div>
            <Navigation
              page={page}
              setPage={(next) => {
                setPage(next)
                setDrawerOpen(false)
              }}
            />
            <SidebarFooter health={health} theme={theme} setTheme={setTheme} />
          </aside>
        </div>
      ) : null}

      <div className="flex min-w-0 flex-1 flex-col">
        <header className="safe-top sticky top-0 z-30 flex h-14 items-center gap-2 border-b border-border bg-background/95 px-3 lg:hidden">
          <Button variant="ghost" size="icon" onClick={() => setDrawerOpen(true)} aria-label="Open navigation">
            <List />
          </Button>
          <img src="/enx.svg" alt="" aria-hidden className="size-7" />
          <span className="truncate text-sm font-semibold">Enx</span>
          <Button
            variant="ghost"
            size="icon"
            aria-label="Toggle colour theme"
            onClick={() => setTheme(theme === 'dark' ? 'light' : 'dark')}
            className="ml-auto"
          >
            {theme === 'dark' ? <Sun /> : <Moon />}
          </Button>
        </header>

        <main className="min-w-0 flex-1">
          {healthError ? (
            <StatePanel title="Server unavailable" body={healthError} action="Retry connection" onAction={refreshHealth} />
          ) : !health ? (
            <StatePanel loading title="Connecting" body="Loading the local Enx runtime." />
          ) : page === 'chat' ? (
            <ChatPage health={health} onOpenProviders={() => setPage('providers')} />
          ) : page === 'sessions' ? (
            <SessionsPage onOpen={(id) => { localStorage.setItem('enx.session', id); setPage('chat') }} />
          ) : page === 'providers' ? (
            <ProvidersPage health={health} onSaved={refreshHealth} />
          ) : (
            <ToolsPage />
          )}
        </main>
      </div>
    </div>
  )
}

function Brand() {
  return (
    <div className="flex items-center gap-2.5 px-4 py-4">
      <img src="/enx.svg" alt="" aria-hidden className="size-8 shrink-0 object-contain" />
      <div className="min-w-0">
        <p className="truncate text-sm font-semibold tracking-tight">Enx</p>
        <p className="truncate text-[11px] text-muted-foreground">Rust coding agent</p>
      </div>
    </div>
  )
}

function Navigation({ page, setPage }: { page: Page; setPage: (page: Page) => void }) {
  return (
    <nav aria-label="Main navigation" className="flex flex-1 flex-col gap-0.5 overflow-y-auto px-2 pb-2">
      {NAV.map(({ id, label, Icon }) => (
        <button
          key={id}
          onClick={() => setPage(id)}
          aria-current={page === id ? 'page' : undefined}
          className={cn(
            'flex min-h-11 items-center gap-2.5 rounded-[var(--radius-sm)] px-3 py-2 text-sm transition-colors',
            page === id
              ? 'bg-primary/12 font-medium text-primary'
              : 'text-muted-foreground hover:bg-accent hover:text-accent-foreground',
          )}
        >
          <Icon className="size-4.5 shrink-0" weight={page === id ? 'fill' : 'regular'} />
          <span className="truncate">{label}</span>
        </button>
      ))}
    </nav>
  )
}

function SidebarFooter({
  health,
  theme,
  setTheme,
}: {
  health?: Health
  theme: 'dark' | 'light'
  setTheme: (theme: 'dark' | 'light') => void
}) {
  return (
    <div className="space-y-1 border-t border-border p-3">
      <div className="flex min-h-11 items-center gap-2 rounded-[var(--radius-sm)] border border-border px-3 py-2">
        {health?.configured ? (
          <Check className="size-4 shrink-0 text-[var(--success)]" weight="bold" />
        ) : (
          <Gear className="size-4 shrink-0 text-[var(--warning)]" />
        )}
        <div className="min-w-0">
          <p className="truncate text-xs font-medium">{health?.configured ? 'Connected' : 'Setup required'}</p>
          <p className="truncate text-[10px] text-muted-foreground">{health?.model ?? 'Loading runtime'}</p>
        </div>
      </div>
      <Button
        variant="ghost"
        size="sm"
        className="min-h-11 w-full justify-start gap-2 px-3"
        onClick={() => setTheme(theme === 'dark' ? 'light' : 'dark')}
      >
        {theme === 'dark' ? <Sun /> : <Moon />}
        {theme === 'dark' ? 'Light mode' : 'Dark mode'}
      </Button>
    </div>
  )
}
