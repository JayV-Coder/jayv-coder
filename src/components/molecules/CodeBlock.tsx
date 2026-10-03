import { useState } from "react";
import { CheckIcon, ChevronDownIcon, CopyIcon } from "lucide-react";
import { highlight, type TokenKind } from "@/modules/conversation";
import { useT } from "@/modules/i18n";
import { cn } from "@/lib/utils";

/** Acima disto o bloco abre recolhido, mostrando só o começo. */
const LONG = 24;
const PREVIEW = 14;

const TONE: Record<TokenKind, string | undefined> = {
  plain: undefined,
  comment: "text-code-comment italic",
  string: "text-code-string",
  number: "text-code-number",
  keyword: "text-code-keyword",
  type: "text-code-type",
  fn: "text-code-fn",
  add: "text-success",
  del: "text-destructive",
  hunk: "text-info",
};

/** O fundo da linha inteira no diff: o que entrou e o que saiu se veem de longe. */
const ROW: Partial<Record<TokenKind, string>> = {
  add: "bg-[color-mix(in_srgb,var(--success)_12%,transparent)]",
  del: "bg-[color-mix(in_srgb,var(--destructive)_12%,transparent)]",
  hunk: "bg-[color-mix(in_srgb,var(--info)_10%,transparent)]",
};

/** Um bloco de código da resposta, como num terminal: a linguagem, o tamanho e
 * o botão de copiar em cima; embaixo, o código colorido sobre um fundo mais
 * fundo que o balão, com o número de cada linha numa margem separada. Linha
 * única (um comando) vai sem número. Bloco comprido abre recolhido. */
export function CodeBlock({ language, code }: { language: string; code: string }) {
  const t = useT();
  const [copied, setCopied] = useState(false);
  const [open, setOpen] = useState(false);
  const copy = async () => {
    await navigator.clipboard.writeText(code);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  };
  const lines = highlight(code.replace(/\n$/, ""), language);
  const long = lines.length > LONG;
  const shown = long && !open ? lines.slice(0, PREVIEW) : lines;
  const numbered = lines.length > 1;
  // A margem cabe o maior número, para o código não dançar.
  const gutter = `${String(lines.length).length + 1}ch`;
  return (
    <section className="my-3 overflow-hidden rounded-md border border-border bg-background">
      <div className="flex items-center gap-3 border-b border-border bg-secondary/60 px-3 py-1 text-caption text-muted-foreground">
        <span className="font-mono font-semibold text-foreground/80">{language || t("code.language")}</span>
        {numbered && <span className="tabular-nums">{t("code.lines", { count: lines.length })}</span>}
        <button type="button" onClick={copy} className="ms-auto inline-flex items-center gap-1.5 rounded-xs px-1.5 py-0.5 text-caption text-muted-foreground hover:bg-secondary hover:text-foreground focus-visible:ring-1 focus-visible:ring-ring focus-visible:outline-none [&_svg]:size-3">
          {copied ? <CheckIcon aria-hidden="true" /> : <CopyIcon aria-hidden="true" />}
          {copied ? t("code.copied") : t("code.copy")}
        </button>
      </div>
      {/* Cada linha é um bloco: copiar um trecho selecionado traz as quebras
          certas, e os números ficam de fora (não são selecionáveis). */}
      <pre className="overflow-x-auto py-2.5 font-mono text-xs leading-[1.65] text-foreground"><code className="grid min-w-max">{shown.map((line, index) => {
        const row = line.length === 1 ? ROW[line[0].kind] : undefined;
        return (
            <span key={index} className={cn("block min-h-[1.65em] pe-4", !numbered && "ps-3", row)}>
              {numbered && <span aria-hidden="true" style={{ width: `calc(${gutter} + 1.5rem)` }} className="me-3 inline-block border-e border-border pe-2.5 text-end text-faint select-none">{index + 1}</span>}
              {line.map((token, at) => <span key={at} className={TONE[token.kind]}>{token.text}</span>)}
            </span>
        );
      })}</code></pre>
      {long && (
        <button
          type="button"
          aria-expanded={open}
          onClick={() => setOpen(!open)}
          className="flex w-full items-center justify-center gap-1.5 border-t border-border bg-secondary/40 py-1 text-caption text-muted-foreground hover:bg-secondary hover:text-foreground focus-visible:ring-1 focus-visible:ring-ring focus-visible:outline-none [&_svg]:size-3"
        >
          <ChevronDownIcon aria-hidden="true" className={cn("transition-transform", open && "rotate-180")} />
          {open ? t("code.collapse") : t("code.expand", { count: lines.length - PREVIEW })}
        </button>
      )}
    </section>
  );
}
