import * as React from "react"
import { cn } from "@/lib/utils"

/** O mesmo contorno vale para campo, área de texto e seleção. */
const fieldClasses = cn(
  "w-full min-w-0 rounded-md border border-input bg-card text-sm text-foreground transition-[color,background-color,border-color,box-shadow] outline-none placeholder:text-muted-foreground",
  "hover:border-muted-foreground/50",
  "focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/20",
  "disabled:cursor-not-allowed disabled:bg-muted disabled:opacity-60",
  "aria-invalid:border-destructive aria-invalid:ring-destructive/20"
)

function Input({ className, type, ...props }: React.ComponentProps<"input">) {
  return (
    <input
      type={type}
      data-slot="input"
      className={cn(
        fieldClasses,
        "h-9 px-3 py-2 selection:bg-accent selection:text-accent-foreground file:inline-flex file:h-7 file:border-0 file:bg-transparent file:text-sm file:font-medium file:text-foreground",
        className
      )}
      {...props}
    />
  )
}

export { Input, fieldClasses }
