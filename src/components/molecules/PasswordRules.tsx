import { CheckIcon, CircleIcon } from "lucide-react";
import { PASSWORD_MIN, PASSWORD_RULES, passwordRules } from "@/modules/auth";
import { useT, type Key } from "@/modules/i18n";
import { cn } from "@/lib/utils";

/** As regras da senha nova, marcadas enquanto a pessoa digita. */
export function PasswordRules({ password }: { password: string }) {
  const t = useT();
  const state = passwordRules(password);
  return (
    <ul aria-live="polite" className="grid gap-1 text-[11.5px]">
      {PASSWORD_RULES.map((rule) => (
        <li key={rule} className={cn("flex items-center gap-1.5", state[rule] ? "text-[#a4f4a9]" : "text-muted-foreground")}>
          {state[rule] ? <CheckIcon className="size-3.5" /> : <CircleIcon className="size-3" />}
          {t(`auth.rule.${rule}` as Key, { min: PASSWORD_MIN })}
        </li>
      ))}
    </ul>
  );
}
