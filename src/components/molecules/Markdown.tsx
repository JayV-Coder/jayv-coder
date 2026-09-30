import { Fragment, useMemo } from "react";
import { parseMarkdown, type Inline } from "@/modules/conversation";
import { CodeBlock } from "./CodeBlock";

function Inlines({ items }: { items: Inline[] }) {
  return items.map((item, index) => {
    switch (item.type) {
      case "strong": return <strong key={index}>{item.text}</strong>;
      case "code": return <code key={index}>{item.text}</code>;
      case "link": return <a key={index} href={item.href} target="_blank" rel="noreferrer">{item.text}</a>;
      default: return <Fragment key={index}>{item.text}</Fragment>;
    }
  });
}

/** A resposta do modelo, montada bloco a bloco a partir do Markdown. */
export function Markdown({ content }: { content: string }) {
  const blocks = useMemo(() => parseMarkdown(content), [content]);
  return (
    <div className="prose-answer">
      {blocks.map((block, index) => {
        switch (block.type) {
          case "heading": {
            const Tag = `h${Math.min(block.level + 2, 6)}` as "h3";
            return <Tag key={index}><Inlines items={block.inline} /></Tag>;
          }
          case "paragraph": return <p key={index}><Inlines items={block.inline} /></p>;
          case "list": {
            const Tag = block.ordered ? "ol" : "ul";
            return <Tag key={index}>{block.items.map((item, at) => <li key={at}><Inlines items={item} /></li>)}</Tag>;
          }
          case "quote": return <blockquote key={index}><Inlines items={block.inline} /></blockquote>;
          case "rule": return <hr key={index} />;
          case "code": return <CodeBlock key={index} language={block.language} code={block.code} />;
          case "table": return (
            <div key={index} className="my-3.5 overflow-x-auto">
              <table className="w-full border-collapse text-[13px] [&_td]:border [&_td]:border-[#2d352f] [&_td]:px-2.5 [&_td]:py-1.5 [&_th]:border [&_th]:border-[#2d352f] [&_th]:bg-[#101411] [&_th]:px-2.5 [&_th]:py-1.5 [&_th]:text-start">
                <thead><tr>{block.head.map((cell, at) => <th key={at}><Inlines items={cell} /></th>)}</tr></thead>
                <tbody>{block.rows.map((row, at) => <tr key={at}>{row.map((cell, col) => <td key={col}><Inlines items={cell} /></td>)}</tr>)}</tbody>
              </table>
            </div>
          );
        }
      })}
    </div>
  );
}
