import { clsx, type ClassValue } from 'clsx'
import { twMerge } from 'tailwind-merge'

/** Merge conditional class names, resolving Tailwind conflicts. */
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}

/** Compact number formatting: 1.2k, 3.4M. */
export function formatCount(n: number): string {
  if (n < 1000) return String(n)
  if (n < 1_000_000) return `${(n / 1000).toFixed(1).replace(/\.0$/, '')}k`
  return `${(n / 1_000_000).toFixed(1).replace(/\.0$/, '')}M`
}

/** Relative time, falling back to a date for old items. */
export function timeAgo(iso: string | Date | null | undefined): string {
  if (!iso) return 'Unknown date'
  const then = typeof iso === 'string' ? new Date(iso) : iso
  const secs = Math.floor((Date.now() - then.getTime()) / 1000)
  if (Number.isNaN(secs)) return 'Unknown date'
  if (secs < 45) return 'just now'
  if (secs < 3600) return `${Math.floor(secs / 60)} min ago`
  if (secs < 86400) return `${Math.floor(secs / 3600)} h ago`
  if (secs < 604800) return `${Math.floor(secs / 86400)} d ago`
  return then.toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: 'numeric' })
}
