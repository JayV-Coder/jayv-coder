import { useState } from "react";
import type { Chat, Question } from "@/modules/core";
import { useT } from "@/modules/i18n";
import { answerForm, answerQuestion, answeringFor, dismissQuestion, formItems, pick, setDraft, setFolded, setStep, setWriting, useConversation, sourceLabel } from "@/modules/conversation";
import { ChevronIcon } from "@/components/atoms";
import { cn } from "@/lib/utils";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { Button } from "@/components/ui/button";

/** A pergunta aberta, vestida na caixa de escrita. Com pergunta de sim ou não,
 * as três opções aparecem e o campo de texto fica travado; com pergunta de
 * escolha, as alternativas aparecem. `IGNORAR` está em todos os trajes —
 * pergunta que não trava a caixa para sempre é pergunta que se pode recusar.
 *
 * O formulário vem em etapas: uma pergunta por vez, com voltar e avançar, e
 * tudo vai junto na última. E o painel inteiro se recolhe numa linha, para o
 * chat de cima ter espaço enquanto se lê o que o agente disse. */
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
  const step = Math.min(answering.step, Math.max(0, items.length - 1));
  const last = step >= items.length - 1;
  const answers = items.map((_, index) => answering.form[index] ?? "");
  const answered = answers.filter((value) => value.trim()).length;
  const folded = answering.folded;
  const act = "h-8 rounded-md px-3 text-caption font-semibold tracking-wider uppercase";
  const toggle = t(folded ? "ask.expand" : "ask.collapse");
  // A linha que fica à vista com o painel recolhido: a pergunta da vez.
  const headline = form && !answering.writing && items.length > 0 ? items[step]?.prompt ?? question.prompt : question.prompt;

  return (
    <div className={cn("border-b border-border px-[18px]", folded ? "py-2" : "pt-2.5 pb-3")}>
      <div className={cn("flex items-center gap-2", !folded && "mb-2")}>
        <button
          type="button"
          aria-expanded={!folded}
          title={toggle}
          aria-label={toggle}
          onClick={() => setFolded(question, !folded)}
          className="-ms-1 flex min-w-0 flex-1 items-center gap-2 rounded-sm px-1 py-0.5 text-start text-caption text-muted-foreground hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
        >
          <ChevronIcon className={cn("w-[15px] flex-none transition-transform duration-200 motion-reduce:transition-none", folded && "-rotate-90")} />
          <span className="flex-none font-semibold tracking-wider text-ask uppercase">{t("ask.title")}</span>
          {form && items.length > 1 && <span className="flex-none font-mono tabular-nums">{t("ask.form.step", { current: step + 1, total: items.length })}</span>}
          {folded && <span className="min-w-0 truncate text-foreground">{headline}</span>}
        </button>
      </div>
      {!folded && (<>
      {form ? (
        // Uma pergunta por etapa, como o formulário do Claude: as alternativas
        // dela, e um campo para responder com outras palavras.
        <div className="mb-2 grid max-h-[46vh] gap-2.5 overflow-y-auto pe-1">
          {answering.writing && <p className="leading-relaxed whitespace-pre-wrap text-foreground">{question.prompt}</p>}
          {!answering.writing && items.length > 1 && (
            // Uma marca por pergunta: a da vez acesa, as já respondidas cheias.
            <ol aria-label={t("ask.form.step", { current: step + 1, total: items.length })} className="flex gap-1">
              {items.map((item, index) => (
                <li key={index} className="flex-1">
                  <button
                    type="button"
                    title={item.prompt}
                    aria-label={`${index + 1}. ${item.prompt}`}
                    aria-current={index === step ? "step" : undefined}
                    onClick={() => setStep(question, index)}
                    className={cn("block h-1.5 w-full rounded-full bg-border transition-colors", answers[index].trim() && "bg-muted-foreground", index === step && "bg-accent")}
                  />
                </li>
              ))}
            </ol>
          )}
          {!answering.writing && items.map((item, index) => {
            if (index !== step) return null;
            const value = answering.form[index] ?? "";
            const chosen = item.options.includes(value);
            return (
              <fieldset key={index} className="grid gap-1.5 rounded-lg border border-border/70 bg-muted px-3.5 py-3">
                <legend className="sr-only">{item.prompt}</legend>
                <p className="leading-relaxed text-foreground">{item.prompt}</p>
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
                  onKeyDown={(event) => {
                    // Enter avança, como num formulário em etapas; na última
                    // não envia sozinho — o botão está ali do lado.
                    if (event.key === "Enter" && !event.nativeEvent.isComposing && !last) {
                      event.preventDefault();
                      setStep(question, step + 1);
                    }
                  }}
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
            {step > 0 && <Button type="button" variant="outline" className={act} onClick={() => setStep(question, step - 1)}>{t("ask.form.previous")}</Button>}
            {last
              ? <Button type="button" className={act} disabled={busy || answered === 0} onClick={() => run(() => answerQuestion(question, chat.id, answers))}>{t("ask.form.send")}</Button>
              : <Button type="button" className={act} onClick={() => setStep(question, step + 1)}>{t("ask.form.next")}</Button>}
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
      </>)}
    </div>
  );
}
