import { Fragment, useMemo } from "react";
import { FileCodeIcon } from "lucide-react";
import { parseMarkdown, type Inline } from "@/modules/conversation";
import { useT } from "@/modules/i18n";
import { CodeBlock } from "./CodeBlock";

type OpenFile = (path: string) => void;

/** O caminho em três partes: a pasta, o nome e o `:linha` do fim. */
function pathParts(text: string) {
  const line = text.match(/(?::\d+){1,2}$|#L\d+(?:-L?\d+)?$/)?.[0] ?? "";
  const path = line ? text.slice(0, -line.length) : text;
  const slash = path.lastIndexOf("/");
  return { dir: path.slice(0, slash + 1), name: path.slice(slash + 1), line };
}

/** O arquivo citado na resposta, como selo: pasta apagada, nome em destaque.
 * Com `onOpenFile`, abre o arquivo no app do sistema. */
function FileRef({ text, path, onOpenFile }: { text: string; path: string; onOpenFile?: OpenFile }) {
  const t = useT();
  const { dir, name, line } = pathParts(text);
  const body = (
    <>
      <FileCodeIcon aria-hidden="true" />
      {dir && <span className="file-dir">{dir}</span>}
      <span className="file-name">{name}</span>
      {line && <span className="file-line">{line}</span>}
    </>
  );
  return onOpenFile
    ? <button type="button" className="file-ref" title={t("chat.openFile", { path })} onClick={() => onOpenFile(path)}>{body}</button>
    : <span className="file-ref" title={path}>{body}</span>;
}

/** O item de lista que não diz nada além de um arquivo. */
const onlyFile = (items: Inline[]) => items.some((item) => item.type === "file") && items.every((item) => item.type === "file" || (item.type === "text" && !item.text.trim()));

function Inlines({ items, onOpenFile }: { items: Inline[]; onOpenFile?: OpenFile }) {
  return items.map((item, index) => {
    switch (item.type) {
      case "strong": return <strong key={index}>{item.text}</strong>;
      case "code": return <code key={index}>{item.text}</code>;
      case "file": return <FileRef key={index} text={item.text} path={item.path} onOpenFile={onOpenFile} />;
      case "link": return <a key={index} href={item.href} target="_blank" rel="noreferrer">{item.text}</a>;
      default: return <Fragment key={index}>{item.text}</Fragment>;
    }
  });
}

/** A resposta do modelo, montada bloco a bloco a partir do Markdown. Os
 * caminhos citados, em `código` ou soltos no texto, viram selos de arquivo; com
 * `onOpenFile`, cada selo abre o arquivo. */
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
            const files = !block.ordered && block.items.every(onlyFile);
            return <Tag key={index} className={files ? "file-list" : undefined}>{block.items.map((item, at) => <li key={at}><Inlines items={item} onOpenFile={onOpenFile} /></li>)}</Tag>;
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
