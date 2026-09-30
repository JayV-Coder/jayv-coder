import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";

export interface Option<V extends string> { value: V; label: string; hint?: string; disabled?: boolean }

/** Uma escolha dentro de uma lista fechada: nada que o núcleo não aceite chega
 * a ser digitado. A explicação da opção aparece embaixo do nome dela. */
export function OptionSelect<V extends string>({ id, value, options, onChange, disabled, invalid }: {
  id?: string; value: V; options: Option<V>[]; onChange: (value: V) => void; disabled?: boolean; invalid?: boolean;
}) {
  return (
    <Select value={value} onValueChange={(next) => onChange(next as V)} disabled={disabled}>
      <SelectTrigger id={id} className="w-full" aria-invalid={invalid || undefined}><SelectValue /></SelectTrigger>
      <SelectContent>
        {options.map((option) => (
          <SelectItem key={option.value} value={option.value} disabled={option.disabled}>
            <span className="flex flex-col items-start">
              <span>{option.label}</span>
              {option.hint && <span className="text-[11px] text-muted-foreground">{option.hint}</span>}
            </span>
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
