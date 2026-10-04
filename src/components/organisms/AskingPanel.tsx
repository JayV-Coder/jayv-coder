import { useState } from "react";
import type { Chat, Question } from "@/modules/core";
import { useT } from "@/modules/i18n";
import { answerForm, answerQuestion, answeringFor, dismissQuestion, formItems, pick, setDraft, setWriting, useConversation, sourceLabel } from "@/modules/conversation";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { Button } from "@/components/ui/button";

/** A pergunta aberta, vestida na caixa de escrita. Com pergunta de sim ou não,
 * as três opções aparecem e o campo de texto fica travado; com pergunta de
 * escolha, as alternativas aparecem. `IGNORAR` está em todos os trajes —
 * pergunta que não trava a caixa para sempre é pergunta que se pode recusar. */
export function AskingPanel({ chat, question }: { chat: Chat; question: Question }) {
  const t = useT();
  // Um clique de cada vez: o segundo clique mandaria outra resposta a uma
  // pergunta que o primeiro já fechou.
  const [busy, setBusy] = useState(false);
  const run = (action: () => Promise<void>) => {
    if (busy) return;
    setBusy(true);
    void action().finally(() => setBusy(false));
  };
  const answering = answeringFor(question, useConversation((state) => state.answering));
  const multiple = question.kind === "multiple";
  const form = question.kind === "form";
  const items = form ? formItems(question) : [];
  const act = "h-8 rounded-md px-3 text-caption font-semibold tracking-wider uppercase";

  return (
    <div className="border-b border-border px-[18px] pt-3.5 pb-3">
      {form ? (
        // Uma pergunta por cartão, como o formulário do Claude: as
        // alternativas dela, e um campo para responder com outras palavras.
        <div className="mb-2 grid max-h-[46vh] gap-2.5 overflow-y-auto pe-1">
          {answering.writing && <p className="leading-relaxed whitespace-pre-wrap text-foreground">{question.prompt}</p>}
          {!answering.writing && items.map((item, index) => {
            const value = answering.form[index] ?? "";
            const chosen = item.options.includes(value);
            return (
              <fieldset key={index} className="grid gap-1.5 rounded-lg border border-border/70 bg-muted px-3.5 py-3">
                <legend className="sr-only">{item.prompt}</legend>
                <p className="flex gap-2 leading-relaxed text-foreground">
                  <span className="font-mono text-caption text-muted-foreground tabular-nums">{index + 1}/{items.length}</span>
                  <span>{item.prompt}</span>
                </p>
                {item.options.length > 0 && (
                  <RadioGroup value={chosen ? value : ""} onValueChange={(next) => answerForm(question, index, next)} className="gap-1.5">
                    {item.options.map((option, at) => (
                      <Label key={option} htmlFor={`form-${index}-${at}`} className="flex cursor-pointer items-center gap-2.5 rounded-md border border-border px-3 py-2 text-sm font-normal hover:bg-secondary">
                        <RadioGroupItem id={`form-${index}-${at}`} value={option} />
                        {option}
                      </Label>
                    ))}
                  </RadioGroup>
                )}
                <Input
                  value={chosen ? "" : value}
                  placeholder={t(item.options.length > 0 ? "ask.form.other" : "ask.form.answer")}
                  aria-label={item.prompt}
                  onChange={(event) => answerForm(question, index, event.target.value)}
                />
              </fieldset>
            );
          })}
        </div>
      ) : (
        <p className="mb-2 leading-relaxed whitespace-pre-wrap text-foreground">{question.prompt}</p>
      )}
      {!form && question.kind !== "noul" && (
        <div className="mb-2 grid gap-1.5">
          {multiple ? question.options.map((option, index) => (
            <Label key={option} htmlFor={`pick-${index}`} className="flex cursor-pointer items-center gap-2.5 rounded-md border border-border px-3 py-2 text-sm font-normal hover:bg-secondary">
              <Checkbox id={`pick-${index}`} checked={answering.picked.includes(option)} onCheckedChange={(checked) => pick(question, option, checked === true)} />
              {option}
            </Label>
          )) : (
            <RadioGroup value={answering.picked[0] ?? ""} onValueChange={(value) => pick(question, value, true)} className="gap-1.5">
              {question.options.map((option, index) => (
                <Label key={option} htmlFor={`pick-${index}`} className="flex cursor-pointer items-center gap-2.5 rounded-md border border-border px-3 py-2 text-sm font-normal hover:bg-secondary">
                  <RadioGroupItem id={`pick-${index}`} value={option} />
                  {option}
                </Label>
              ))}
            </RadioGroup>
          )}
          {multiple && <small className="text-caption text-muted-foreground">{t("ask.multiple")}</small>}
        </div>
      )}
      <small className="block font-mono text-caption text-muted-foreground">{t("ask.from", { source: sourceLabel(question.source) })}</small>
      <div className="mt-3 flex flex-wrap gap-2">
        {answering.writing ? (
          <Button type="button" variant="outline" className={act} onClick={() => { setWriting(question, false); setDraft(chat.id, ""); }}>{t("ask.back")}</Button>
        ) : form ? (
          <>
            <Button type="button" className={act} disabled={busy || !answering.form.some((value) => value?.trim())} onClick={() => run(() => answerQuestion(question, chat.id, items.map((_, index) => answering.form[index] ?? "")))}>{t("ask.form.send")}</Button>
            <Button type="button" variant="outline" className={act} onClick={() => setWriting(question, true)}>{t("ask.reply")}</Button>
          </>
        ) : question.kind === "noul" ? (
          <>
            <Button type="button" disabled={busy} className={act} onClick={() => run(() => answerQuestion(question, chat.id, ["yes"]))}>{t("ask.yes")}</Button>
            <Button type="button" disabled={busy} variant="outline" className={act} onClick={() => run(() => answerQuestion(question, chat.id, ["no"]))}>{t("ask.no")}</Button>
            <Button type="button" variant="outline" className={act} onClick={() => setWriting(question, true)}>{t("ask.reply")}</Button>
          </>
        ) : (
          <>
            <Button type="button" className={act} disabled={busy || answering.picked.length === 0} onClick={() => run(() => answerQuestion(question, chat.id, answering.picked))}>{t("ask.sendChoice")}</Button>
            <Button type="button" variant="outline" className={act} onClick={() => setWriting(question, true)}>{t("ask.reply")}</Button>
          </>
        )}
        <Button type="button" disabled={busy} variant="ghost" className={`${act} ms-auto text-muted-foreground`} onClick={() => run(() => dismissQuestion(question, chat.id))}>{t("ask.ignore")}</Button>
      </div>
    </div>
  );
}
