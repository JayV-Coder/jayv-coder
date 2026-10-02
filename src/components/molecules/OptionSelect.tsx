import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";

export interface Option<V extends string> { value: V; label: string; hint?: string; disabled?: boolean }

/** Uma escolha dentro de uma lista fechada: nada que o núcleo não aceite chega
 * a ser digitado. Na lista, a explicação fica embaixo do nome; no campo fechado,
 * ao lado dele, numa linha só. */
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
              {current.hint && <span className="truncate font-mono text-[11px] text-muted-foreground">{current.hint}</span>}
            </span>
          )}
        </SelectValue>
      </SelectTrigger>
      <SelectContent>
        {options.map((option) => (
          <SelectItem key={option.value} value={option.value} disabled={option.disabled} description={option.hint}>{option.label}</SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
