import { Fragment, useState } from "react";
import { CheckIcon, CopyIcon } from "lucide-react";
import { useT } from "@/modules/i18n";

/** Um bloco de código da resposta, como num terminal: a linguagem e o botão
 * de copiar em cima, e o código com o número de cada linha na margem. */
export function CodeBlock({ language, code }: { language: string; code: string }) {
  const t = useT();
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    await navigator.clipboard.writeText(code);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  };
  const lines = code.replace(/\n$/, "").split("\n");
  // A margem cabe o maior número, para o código não dançar.
  const gutter = `${String(lines.length).length + 1}ch`;
  return (
    <section className="my-3 overflow-hidden rounded-md border border-border bg-card">
      <div className="flex items-center justify-between border-b border-border bg-muted px-3 py-1 text-caption text-muted-foreground">
        <span className="font-mono">{language || t("code.language")}</span>
        <button type="button" onClick={copy} className="inline-flex items-center gap-1.5 rounded-xs px-1.5 py-0.5 text-caption text-muted-foreground hover:bg-secondary hover:text-foreground focus-visible:ring-1 focus-visible:ring-ring focus-visible:outline-none [&_svg]:size-3">
          {copied ? <CheckIcon aria-hidden="true" /> : <CopyIcon aria-hidden="true" />}
          {copied ? t("code.copied") : t("code.copy")}
        </button>
      </div>
      {/* As quebras ficam no texto (e não no layout) para que copiar um
          trecho selecionado traga as linhas certas, sem os números. */}
      <pre className="overflow-x-auto px-3 py-2.5 font-mono text-xs leading-relaxed text-foreground"><code>{lines.map((line, index) => (
        <Fragment key={index}>
          {index > 0 && "\n"}
          <span aria-hidden="true" style={{ width: gutter }} className="inline-block text-faint select-none">{index + 1}</span>
          {line}
        </Fragment>
      ))}</code></pre>
    </section>
  );
}
