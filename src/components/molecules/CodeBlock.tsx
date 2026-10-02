import { useState } from "react";
import { useT } from "@/modules/i18n";

/** Um bloco de código da resposta, com o botão de copiar. */
export function CodeBlock({ language, code }: { language: string; code: string }) {
  const t = useT();
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    await navigator.clipboard.writeText(code);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  };
  return (
    <section className="my-3.5 overflow-hidden rounded-lg border border-border bg-card">
      <div className="flex items-center justify-between border-b border-border px-3.5 py-1.5 text-caption text-muted-foreground">
        <span>{language || t("code.language")}</span>
        <button type="button" onClick={copy} className="rounded-md px-2 py-0.5 text-caption text-muted-foreground hover:bg-secondary hover:text-foreground">
          {copied ? t("code.copied") : t("code.copy")}
        </button>
      </div>
      <pre className="overflow-x-auto px-4 py-3.5 font-mono text-xs leading-relaxed text-foreground"><code>{code}</code></pre>
    </section>
  );
}
