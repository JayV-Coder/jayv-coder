import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";

export interface Option<V extends string> { value: V; label: string; hint?: string; disabled?: boolean }

/** Uma escolha dentro de uma lista fechada: nada que o núcleo não aceite chega
 * a ser digitado. Na lista, a explicação fica embaixo do nome; no campo fechado,
 * ao lado dele, numa linha só. A lista aberta tem largura máxima: um nome
 * comprido (o título de um chat) é cortado com reticências e aparece inteiro
 * ao passar o mouse, em vez de esticar a lista pela tela. */
export function OptionSelect<V extends string>({ id, value, options, onChange, disabled, invalid, label }: {
  id?: string; value: V; options: Option<V>[]; onChange: (value: V) => void; disabled?: boolean; invalid?: boolean;
  /** O nome lido pelo leitor de tela quando não há `<label>` apontando para o campo. */
  label?: string;
}) {
  const current = options.find((option) => option.value === value);
  return (
    <Select value={value} onValueChange={(next) => onChange(next as V)} disabled={disabled}>
      <SelectTrigger id={id} className="w-full min-w-0" aria-invalid={invalid || undefined} aria-label={label}>
        <SelectValue>
          {current && (
            <span className="flex min-w-0 items-baseline gap-2">
              <span className="truncate">{current.label}</span>
              {current.hint && <span className="truncate font-mono text-caption text-muted-foreground">{current.hint}</span>}
            </span>
          )}
        </SelectValue>
      </SelectTrigger>
      <SelectContent className="max-w-[min(24rem,var(--radix-select-content-available-width))]">
        {options.map((option) => (
          <SelectItem key={option.value} value={option.value} disabled={option.disabled} description={option.hint} title={option.label} className="[&>span:last-child]:min-w-0">
            <span className="block truncate">{option.label}</span>
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
