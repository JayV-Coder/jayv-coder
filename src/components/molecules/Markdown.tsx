import { Fragment, useMemo } from "react";
import { parseMarkdown, type Inline } from "@/modules/conversation";
import { useT } from "@/modules/i18n";
import { CodeBlock } from "./CodeBlock";

type OpenFile = (path: string) => void;

function Inlines({ items, onOpenFile }: { items: Inline[]; onOpenFile?: OpenFile }) {
  const t = useT();
  return items.map((item, index) => {
    switch (item.type) {
      case "strong": return <strong key={index}>{item.text}</strong>;
      case "code": return <code key={index}>{item.text}</code>;
      case "file": return onOpenFile
        ? <button key={index} type="button" className="file-ref" title={t("chat.openFile", { path: item.path })} onClick={() => onOpenFile(item.path)}><code>{item.text}</code></button>
        : <code key={index}>{item.text}</code>;
      case "link": return <a key={index} href={item.href} target="_blank" rel="noreferrer">{item.text}</a>;
      default: return <Fragment key={index}>{item.text}</Fragment>;
    }
  });
}

/** A resposta do modelo, montada bloco a bloco a partir do Markdown. Com
 * `onOpenFile`, os caminhos citados em `código` viram botões que abrem o
 * arquivo. */
export function Markdown({ content, onOpenFile }: { content: string; onOpenFile?: OpenFile }) {
  const blocks = useMemo(() => parseMarkdown(content), [content]);
  return (
    <div className="prose-answer">
      {blocks.map((block, index) => {
        switch (block.type) {
          case "heading": {
            const Tag = `h${Math.min(block.level + 2, 6)}` as "h3";
            return <Tag key={index}><Inlines items={block.inline} onOpenFile={onOpenFile} /></Tag>;
          }
          case "paragraph": return <p key={index}><Inlines items={block.inline} onOpenFile={onOpenFile} /></p>;
          case "list": {
            const Tag = block.ordered ? "ol" : "ul";
            return <Tag key={index}>{block.items.map((item, at) => <li key={at}><Inlines items={item} onOpenFile={onOpenFile} /></li>)}</Tag>;
          }
          case "quote": return <blockquote key={index}><Inlines items={block.inline} onOpenFile={onOpenFile} /></blockquote>;
          case "rule": return <hr key={index} />;
          case "code": return <CodeBlock key={index} language={block.language} code={block.code} />;
          case "table": return (
            <div key={index} className="my-3.5 overflow-x-auto">
              <table className="w-full border-collapse text-sm [&_td]:border [&_td]:border-border [&_td]:px-2.5 [&_td]:py-1.5 [&_th]:border [&_th]:border-border [&_th]:bg-card [&_th]:px-2.5 [&_th]:py-1.5 [&_th]:text-start">
                <thead><tr>{block.head.map((cell, at) => <th key={at}><Inlines items={cell} onOpenFile={onOpenFile} /></th>)}</tr></thead>
                <tbody>{block.rows.map((row, at) => <tr key={at}>{row.map((cell, col) => <td key={col}><Inlines items={cell} onOpenFile={onOpenFile} /></td>)}</tr>)}</tbody>
              </table>
            </div>
          );
        }
      })}
    </div>
  );
}
