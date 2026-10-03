import * as React from "react"
import { cva, type VariantProps } from "class-variance-authority"
import { cn } from "@/lib/utils"
import { Loader2Icon } from "lucide-react"
import { Slot } from "radix-ui"

const buttonVariants = cva(
  "inline-flex shrink-0 items-center justify-center gap-2 rounded-md border text-sm font-medium whitespace-nowrap transition-[color,background-color,border-color,box-shadow,opacity] outline-none select-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background disabled:pointer-events-none disabled:opacity-50 aria-busy:cursor-progress aria-invalid:border-destructive [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
  {
    variants: {
      variant: {
        default:
          "border-primary bg-primary text-primary-foreground hover:bg-primary/88 active:bg-primary/80",
        secondary:
          "border-border bg-secondary text-secondary-foreground hover:bg-border/70 active:bg-border",
        outline:
          "border-border bg-transparent text-foreground hover:bg-secondary active:bg-border/70",
        ghost:
          "border-transparent bg-transparent text-foreground hover:bg-secondary active:bg-border/70",
        accent:
          "border-accent bg-accent text-accent-foreground hover:brightness-95 active:brightness-90",
        destructive:
          "border-destructive bg-destructive text-destructive-foreground hover:bg-destructive/90 active:bg-destructive/80",
        link: "border-transparent px-0 text-foreground underline-offset-4 hover:underline",
      },
      size: {
        default: "h-9 px-3.5 py-2 has-[>svg]:px-3",
        xs: "h-6 gap-1 px-2 text-xs has-[>svg]:px-1.5 [&_svg:not([class*='size-'])]:size-3",
        sm: "h-8 gap-1.5 px-3 has-[>svg]:px-2.5",
        lg: "h-10 px-5 has-[>svg]:px-4",
        icon: "size-9",
        "icon-xs": "size-6 [&_svg:not([class*='size-'])]:size-3",
        "icon-sm": "size-8",
        "icon-lg": "size-10",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  }
)

/** `loading` trava o botão e troca o ícone por um giro; o rótulo continua. */
function Button({
  className,
  variant = "default",
  size = "default",
  asChild = false,
  loading = false,
  disabled,
  children,
  ...props
}: React.ComponentProps<"button"> &
  VariantProps<typeof buttonVariants> & {
    asChild?: boolean
    loading?: boolean
  }) {
  const Comp = asChild ? Slot.Root : "button"

  return (
    <Comp
      data-slot="button"
      data-variant={variant}
      data-size={size}
      aria-busy={loading || undefined}
      disabled={disabled || loading}
      className={cn(buttonVariants({ variant, size, className }))}
      {...props}
    >
      {loading && !asChild ? (
        <>
          <Loader2Icon aria-hidden="true" className="animate-spin" />
          {children}
        </>
      ) : (
        children
      )}
    </Comp>
  )
}

export { Button, buttonVariants }
