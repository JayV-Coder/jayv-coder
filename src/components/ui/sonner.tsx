import {
  CircleCheckIcon,
  InfoIcon,
  Loader2Icon,
  OctagonXIcon,
  TriangleAlertIcon,
} from "lucide-react"
import { Toaster as Sonner, type ToasterProps } from "sonner"
import { useTheme } from "@/modules/theme"

/** Avisos discretos: superfície do tema, borda fina e o ícone na cor do estado. */
const Toaster = ({ ...props }: ToasterProps) => {
  const theme = useTheme((state) => state.theme)
  return (
    <Sonner
      theme={theme}
      className="toaster group"
      icons={{
        success: <CircleCheckIcon aria-hidden="true" className="size-4 text-success" />,
        info: <InfoIcon aria-hidden="true" className="size-4 text-info" />,
        warning: <TriangleAlertIcon aria-hidden="true" className="size-4 text-warning" />,
        error: <OctagonXIcon aria-hidden="true" className="size-4 text-destructive" />,
        loading: <Loader2Icon aria-hidden="true" className="size-4 animate-spin text-muted-foreground" />,
      }}
      toastOptions={{ classNames: { toast: "!shadow-md !text-sm", description: "!text-muted-foreground" } }}
      style={
        {
          "--normal-bg": "var(--popover)",
          "--normal-text": "var(--popover-foreground)",
          "--normal-border": "var(--border)",
          "--border-radius": "calc(var(--radius) + 2px)",
        } as React.CSSProperties
      }
      {...props}
    />
  )
}

export { Toaster }
