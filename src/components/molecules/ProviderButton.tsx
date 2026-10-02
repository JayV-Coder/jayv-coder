import type { Provider } from "@/modules/auth";
import { ProviderIcon } from "@/components/atoms";
import { Button } from "@/components/ui/button";

/** Entrar ou vincular por um provedor: a marca e o que o botão faz. */
export function ProviderButton({ provider, label, onClick, disabled }: { provider: Provider; label: string; onClick: () => void; disabled?: boolean }) {
  return (
    <Button type="button" variant="outline" disabled={disabled} onClick={onClick}>
      <ProviderIcon provider={provider} />
      {label}
    </Button>
  );
}
