import { useMemo } from 'react'
import { Page } from '@/components/PageState'

export function ToolsPage() {
  const tools = useMemo(
    () => [
      ['read', 'Read files or list directories with line-aware output.'],
      ['write', 'Create or replace files inside the workspace.'],
      ['edit', 'Replace one exact text block safely.'],
      ['glob', 'Find workspace paths while respecting .gitignore.'],
      ['grep', 'Search UTF-8 files with regular expressions.'],
      ['bash', 'Run a shell command as Orchestrator.'],
      ['fetch', 'Fetch HTTP documentation and public sources.'],
      ['todo', 'Track a visible checklist during longer work.'],
    ],
    [],
  )
  return (
    <Page title="Tools" description="The focused core tool surface exposed by the Rust harness.">
      <div className="overflow-hidden rounded-[var(--radius-lg)] border border-border bg-card">
        {tools.map(([name, description]) => (
          <div key={name} className="flex gap-4 border-b border-border px-4 py-3 last:border-0">
            <code className="w-16 shrink-0 font-mono text-xs font-medium text-primary">{name}</code>
            <p className="text-sm text-muted-foreground">{description}</p>
          </div>
        ))}
      </div>
    </Page>
  )
}
