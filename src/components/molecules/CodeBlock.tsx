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
    <section className="my-3.5 overflow-hidden rounded-[10px] border border-[#2d352f] bg-[#0b0e0c]">
      <div className="flex items-center justify-between border-b border-[#232a25] px-3.5 py-1.5 text-[11px] text-[#7f8981]">
        <span>{language || t("code.language")}</span>
        <button type="button" onClick={copy} className="rounded-md px-2 py-0.5 text-[11px] text-[#9ca69e] hover:bg-accent hover:text-accent-foreground">
          {copied ? t("code.copied") : t("code.copy")}
        </button>
      </div>
      <pre className="overflow-x-auto px-4 py-3.5 font-mono text-[12.5px] leading-relaxed text-[#d7e2d9]"><code>{code}</code></pre>
    </section>
  );
}
