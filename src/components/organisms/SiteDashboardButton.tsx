import type { ComponentProps } from "react";
import { ExternalLinkIcon } from "lucide-react";
import { reportError } from "@/modules/feedback";
import { useT } from "@/modules/i18n";
import { openDashboard, SITE_URL } from "@/modules/site";
import { Button } from "@/components/ui/button";

/** Leva ao painel do site, onde moram criar organização, convidar membros e a
 * política de LLM. Sem o endereço do site no build, não aparece. */
export function SiteDashboardButton({ path, variant = "outline", size }: {
  path?: string;
  variant?: ComponentProps<typeof Button>["variant"];
  size?: ComponentProps<typeof Button>["size"];
}) {
  const t = useT();
  if (!SITE_URL) return null;
  return (
    <Button variant={variant} size={size} className="justify-self-start" onClick={() => void openDashboard(path).catch(reportError)}>
      <ExternalLinkIcon />
      {t("org.site.manage")}
    </Button>
  );
}
