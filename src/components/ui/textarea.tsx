import * as React from "react"
import { cn } from "@/lib/utils"
import { fieldClasses } from "@/components/ui/input"

function Textarea({ className, ...props }: React.ComponentProps<"textarea">) {
  return (
    <textarea
      data-slot="textarea"
      className={cn(fieldClasses, "flex field-sizing-content min-h-16 px-3 py-2 leading-relaxed", className)}
      {...props}
    />
  )
}

export { Textarea }
