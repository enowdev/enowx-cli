import { CircleNotch, Robot } from '@phosphor-icons/react'
import { Button } from '@/components/ui/button'

export function Page({ title, description, children }: { title?: string; description?: string; children: React.ReactNode }) {
  return (
    <div className="mx-auto w-full max-w-5xl px-4 py-6 sm:px-6 lg:px-8 lg:py-8">
      {title ? (
        <header className="mb-6 space-y-1.5">
          <h1 className="text-xl font-semibold tracking-tight">{title}</h1>
          {description ? <p className="max-w-2xl text-sm text-muted-foreground">{description}</p> : null}
        </header>
      ) : null}
      {children}
    </div>
  )
}

export function StatePanel({ title, body, loading, action, onAction }: { title: string; body: string; loading?: boolean; action?: string; onAction?: () => void | Promise<void> }) {
  return (
    <div className="m-auto flex min-h-[45dvh] max-w-lg flex-col items-center justify-center px-6 text-center">
      {loading ? <CircleNotch className="mb-4 size-6 animate-spin text-primary" /> : <Robot className="mb-4 size-7 text-primary" />}
      <h2 className="text-base font-semibold">{title}</h2>
      <p className="mt-1.5 text-sm leading-relaxed text-muted-foreground">{body}</p>
      {action && onAction ? <Button className="mt-4" onClick={() => void onAction()}>{action}</Button> : null}
    </div>
  )
}

export function ErrorBanner({ message }: { message: string }) {
  return (
    <div role="alert" className="rounded-[var(--radius-sm)] border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
      {message}
    </div>
  )
}
