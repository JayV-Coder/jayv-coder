import { Building2Icon } from "lucide-react";
import { useT, type Key } from "@/modules/i18n";
import type { Organization } from "@/modules/organizations";

/** A placa da organização no alto do menu lateral, no ambiente dela: nome,
 * @slug e o papel de quem usa o app. Enquanto a lista das organizações não
 * chegou, só o ícone e um traço no lugar do nome. */
export function OrganizationPlate({ organization }: { organization: Organization | null }) {
  const t = useT();
  return (
    <div className="mb-3 rounded-lg border border-sidebar-border bg-card px-3 py-2.5">
      <span className="mb-1 flex min-w-0 items-center gap-1.5 text-caption font-medium text-success">
        <Building2Icon aria-hidden="true" className="size-3.5 flex-none" />
        <span className="truncate">{organization ? t(`org.role.${organization.role}` as Key) : "—"}</span>
      </span>
      <strong title={organization?.name} className="block text-sm leading-tight font-semibold break-words text-foreground">{organization?.name ?? "—"}</strong>
      {organization && <span className="mt-1 block truncate font-mono text-caption text-muted-foreground">@{organization.slug}</span>}
    </div>
  );
}
