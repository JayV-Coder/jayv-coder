import type { Chat, Question } from "@/modules/core";
import { useT } from "@/modules/i18n";
import { answerQuestion, answeringFor, dismissQuestion, pick, setDraft, setWriting, useConversation } from "@/modules/conversation";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { Button } from "@/components/ui/button";

/** A pergunta aberta, vestida na caixa de escrita. Com pergunta de sim ou não,
 * as três opções aparecem e o campo de texto fica travado; com pergunta de
 * escolha, as alternativas aparecem. `IGNORAR` está em todos os trajes —
 * pergunta que não trava a caixa para sempre é pergunta que se pode recusar. */
export function AskingPanel({ chat, question }: { chat: Chat; question: Question }) {
  const t = useT();
  const answering = answeringFor(question, useConversation((state) => state.answering));
  const multiple = question.kind === "multiple";
  const act = "h-8 rounded-md px-3 text-[11px] font-bold tracking-[0.12em] uppercase";

  return (
    <div className="border-b border-[#252d27] px-[18px] pt-3.5 pb-3">
      <p className="mb-2 leading-relaxed whitespace-pre-wrap text-[#e6eee7]">{question.prompt}</p>
      {question.kind !== "noul" && (
        <div className="mb-2 grid gap-1.5">
          {multiple ? question.options.map((option, index) => (
            <Label key={option} htmlFor={`pick-${index}`} className="flex cursor-pointer items-center gap-2.5 rounded-md border border-border px-3 py-2 text-sm font-normal hover:bg-accent">
              <Checkbox id={`pick-${index}`} checked={answering.picked.includes(option)} onCheckedChange={(checked) => pick(question, option, checked === true)} />
              {option}
            </Label>
          )) : (
            <RadioGroup value={answering.picked[0] ?? ""} onValueChange={(value) => pick(question, value, true)} className="gap-1.5">
              {question.options.map((option, index) => (
                <Label key={option} htmlFor={`pick-${index}`} className="flex cursor-pointer items-center gap-2.5 rounded-md border border-border px-3 py-2 text-sm font-normal hover:bg-accent">
                  <RadioGroupItem id={`pick-${index}`} value={option} />
                  {option}
                </Label>
              ))}
            </RadioGroup>
          )}
          {multiple && <small className="text-[11px] text-muted-foreground">{t("ask.multiple")}</small>}
        </div>
      )}
      <small className="block font-mono text-[11px] text-[#6e7870]">{t("ask.from", { source: question.source })}</small>
      <div className="mt-3 flex flex-wrap gap-2">
        {answering.writing ? (
          <Button type="button" variant="outline" className={act} onClick={() => { setWriting(question, false); setDraft(chat.id, ""); }}>{t("ask.back")}</Button>
        ) : question.kind === "noul" ? (
          <>
            <Button type="button" className={act} onClick={() => void answerQuestion(question, chat.id, ["yes"])}>{t("ask.yes")}</Button>
            <Button type="button" variant="outline" className={act} onClick={() => void answerQuestion(question, chat.id, ["no"])}>{t("ask.no")}</Button>
            <Button type="button" variant="outline" className={act} onClick={() => setWriting(question, true)}>{t("ask.reply")}</Button>
          </>
        ) : (
          <>
            <Button type="button" className={act} disabled={answering.picked.length === 0} onClick={() => void answerQuestion(question, chat.id, answering.picked)}>{t("ask.sendChoice")}</Button>
            <Button type="button" variant="outline" className={act} onClick={() => setWriting(question, true)}>{t("ask.reply")}</Button>
          </>
        )}
        <Button type="button" variant="ghost" className={`${act} ms-auto text-muted-foreground`} onClick={() => void dismissQuestion(question, chat.id)}>{t("ask.ignore")}</Button>
      </div>
    </div>
  );
}
