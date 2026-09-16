import * as React from 'react'
import { CircleNotch } from '@phosphor-icons/react'
import { cn } from '@/lib/utils'

type Variant = 'default' | 'secondary' | 'outline' | 'ghost' | 'destructive'
type Size = 'sm' | 'default' | 'icon' | 'icon-sm'

const VARIANTS: Record<Variant, string> = {
  default: 'bg-primary text-primary-foreground hover:bg-primary/90 shadow-sm',
  secondary: 'bg-secondary text-secondary-foreground hover:bg-secondary/80',
  outline: 'border border-border bg-transparent hover:bg-accent hover:text-accent-foreground',
  ghost: 'hover:bg-accent hover:text-accent-foreground',
  destructive: 'bg-destructive text-destructive-foreground hover:bg-destructive/90 shadow-sm',
}

// Every size keeps a 44px touch area on coarse pointers via the shared base
// class, so a thumb target never depends on the visual box alone.
const SIZES: Record<Size, string> = {
  sm: 'h-8 px-3 text-xs [&_svg]:size-4',
  default: 'h-9 px-4 [&_svg]:size-4',
  icon: 'size-9 [&_svg]:size-4',
  'icon-sm': 'size-8 [&_svg]:size-4',
}

export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant
  size?: Size
  loading?: boolean
}

/** Primary interactive control. Set `loading` to show an inline spinner. */
export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant = 'default', size = 'default', loading = false, children, disabled, ...props }, ref) => (
    <button
      ref={ref}
      disabled={disabled || loading}
      className={cn(
        'relative inline-flex select-none items-center justify-center gap-2 whitespace-nowrap rounded-[var(--radius-sm)] text-sm font-medium transition-[background,color,box-shadow,transform] duration-150 active:scale-[0.98] disabled:pointer-events-none disabled:opacity-50 [&_svg]:shrink-0',
        // Coarse pointers get a 44x44 hit box centred on the control, without
        // changing the visual size on a mouse-driven screen.
        'after:absolute after:left-1/2 after:top-1/2 after:hidden after:size-11 after:-translate-x-1/2 after:-translate-y-1/2 after:content-[""] pointer-coarse:after:block',
        VARIANTS[variant],
        SIZES[size],
        className,
      )}
      {...props}
    >
      {loading ? <CircleNotch className="animate-spin" weight="bold" /> : null}
      {children}
    </button>
  ),
)
Button.displayName = 'Button'
