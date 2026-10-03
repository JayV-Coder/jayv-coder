import * as React from "react"
import { cva, type VariantProps } from "class-variance-authority"
import { cn } from "@/lib/utils"
import { Slot } from "radix-ui"

const badgeVariants = cva(
  "inline-flex w-fit shrink-0 items-center justify-center gap-1 overflow-hidden rounded-xs border px-1.5 py-0 text-xs font-normal whitespace-nowrap transition-[color,background-color,border-color,box-shadow] focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 focus-visible:ring-offset-background [&>svg]:pointer-events-none [&>svg]:size-3",
  {
    variants: {
      variant: {
        default: "border-border bg-secondary text-secondary-foreground [a&]:hover:bg-border/70",
        secondary: "border-transparent bg-muted text-muted-foreground [a&]:hover:bg-border/70",
        success: "border-success/30 bg-success/10 text-success",
        warning: "border-warning/30 bg-warning/10 text-warning",
        destructive: "border-destructive/30 bg-destructive/10 text-destructive",
        outline: "border-border bg-transparent text-foreground [a&]:hover:bg-secondary",
        accent: "border-accent bg-accent text-accent-foreground",
        ghost: "border-transparent [a&]:hover:bg-secondary",
        link: "border-transparent text-foreground underline-offset-4 [a&]:hover:underline",
      },
      /** Versão, identificador, métrica: monoespaçada e em caixa alta. */
      mono: {
        true: "font-mono text-caption tracking-wider uppercase",
        false: "",
      },
    },
    defaultVariants: {
      variant: "default",
      mono: false,
    },
  }
)

function Badge({
  className,
  variant = "default",
  mono = false,
  asChild = false,
  ...props
}: React.ComponentProps<"span"> &
  VariantProps<typeof badgeVariants> & { asChild?: boolean }) {
  const Comp = asChild ? Slot.Root : "span"

  return (
    <Comp
      data-slot="badge"
      data-variant={variant}
      className={cn(badgeVariants({ variant, mono }), className)}
      {...props}
    />
  )
}

export { Badge, badgeVariants }
