import { useEffect, useMemo, useState, type FormEvent, type ReactNode } from "react";
import { useLocale, useT, type Key } from "@/modules/i18n";
import {
  CUSTOM_TEXT_MAX, DISPLAY_NAME_MAX, GENDERS, LONG_TEXT_MAX, PRONOUNS, ROLES, SEXES, USERNAME_MAX, usernameAvailable, usernameOk, type AccountProfile,
} from "@/modules/profile";
import { DateParts, FormField, OptionSelect, type Option } from "@/components/molecules";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

/** O `Select` não aceita valor vazio: "não informado" vira esta marca e volta
 * a ser nulo ao gravar. */
const UNSET = "unset";
type Choice<V extends string> = V | typeof UNSET;

const pick = <V extends string>(value: V | null): Choice<V> => value ?? UNSET;
const drop = <V extends string>(value: Choice<V>): V | null => (value === UNSET ? null : (value as V));

/** Os códigos ISO de duas letras que o `Intl` sabe nomear. Os de grupos e
 * reservados (União Europeia, ONU, pseudolocais) ficam de fora. */
const NOT_COUNTRIES = new Set(["EU", "EZ", "UN", "QO", "XA", "XB", "ZZ"]);
function countryCodes(names: Intl.DisplayNames) {
  const letters = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
  const codes: string[] = [];
  for (const first of letters) for (const second of letters) {
    const code = first + second;
    if (NOT_COUNTRIES.has(code)) continue;
    const name = names.of(code);
    if (name && name !== code) codes.push(code);
  }
  return codes;
}

/** O que a tela sabe do nome de usuário digitado. */
type UsernameState = "same" | "invalid" | "checking" | "available" | "taken" | "unknown";

/** Confere o formato na hora e a disponibilidade no banco depois de uma pausa
 * na digitação; o índice único do banco continua sendo a última palavra. */
function useUsernameState(name: string, original: string): UsernameState {
  const [state, setState] = useState<UsernameState>("same");
  useEffect(() => {
    if (name === original) { setState("same"); return; }
    if (!usernameOk(name)) { setState("invalid"); return; }
    setState("checking");
    let live = true;
    const timer = setTimeout(() => {
      usernameAvailable(name)
        .then((free) => { if (live) setState(free ? "available" : "taken"); })
        // Sem resposta (rede), deixa gravar: o banco decide.
        .catch(() => { if (live) setState("unknown"); });
    }, 400);
    return () => { live = false; clearTimeout(timer); };
  }, [name, original]);
  return state;
}

/** Os dados pessoais da conta, no passo de perfil e na página de Perfil. Só o
 * nome de exibição é obrigatório; documentos, endereço e telefone não existem
 * aqui de propósito. */
export function ProfileForm({ initial, busy, submitLabel, onSubmit, secondary }: {
  initial: AccountProfile;
  busy: boolean;
  submitLabel: string;
  onSubmit: (draft: AccountProfile) => void;
  secondary?: ReactNode;
}) {
  const t = useT();
  const locale = useLocale();
  const [draft, setDraft] = useState(initial);
  const set = <K extends keyof AccountProfile>(key: K, value: AccountProfile[K]) => setDraft((current) => ({ ...current, [key]: value }));
  const username = useUsernameState(draft.username, initial.username);
  const usernameLocked = initial.usernameSetAt !== null;
  const usernameNote = username === "invalid" ? t("profile.username.invalid", { min: 3, max: USERNAME_MAX })
    : username === "taken" ? t("profile.usernameTaken") : null;
  const usernameHint = username === "checking" ? t("profile.username.checking")
    : username === "available" ? t("profile.username.available")
    : t(usernameLocked ? "profile.username.locked" : "profile.username.once");

  const unset: Option<typeof UNSET> = { value: UNSET, label: t("profile.unset") };
  const closed = <V extends string>(group: string, values: readonly V[]): Option<Choice<V>>[] =>
    [unset, ...values.map((value) => ({ value, label: t(`profile.${group}.${value}` as Key) }))];

  const countries = useMemo<Option<string>[]>(() => {
    const names = new Intl.DisplayNames([locale], { type: "region", fallback: "none" });
    const list = countryCodes(names).map((code) => ({ value: code, label: names.of(code) ?? code }));
    return list.sort((a, b) => a.label.localeCompare(b.label, locale));
  }, [locale]);
  const zones = useMemo<Option<string>[]>(() => Intl.supportedValuesOf("timeZone").map((zone) => ({ value: zone, label: zone })), []);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    onSubmit(draft);
  };

  return (
    <form onSubmit={submit} className="grid gap-5">
      <div className="grid gap-4 sm:grid-cols-2">
        <FormField label={t("profile.field.displayName")} htmlFor="profile-display-name" hint={t("profile.field.displayName.hint")}>
          <Input id="profile-display-name" required maxLength={DISPLAY_NAME_MAX} value={draft.displayName} onChange={(event) => set("displayName", event.target.value)} />
        </FormField>
        <FormField label={t("profile.field.username")} htmlFor="profile-username" hint={usernameHint} error={usernameNote}>
          <div className="relative">
            <span aria-hidden="true" className="pointer-events-none absolute inset-y-0 start-3 grid place-items-center text-sm text-muted-foreground">@</span>
            <Input id="profile-username" required disabled={usernameLocked} autoComplete="username" autoCapitalize="none" spellCheck={false} maxLength={USERNAME_MAX} className="ps-7"
              aria-invalid={usernameNote ? true : undefined} value={draft.username} onChange={(event) => set("username", event.target.value.toLowerCase().replace(/\s/g, ""))} />
          </div>
        </FormField>
        <FormField label={t("profile.field.sex")} htmlFor="profile-sex">
          <OptionSelect id="profile-sex" value={pick(draft.sex)} options={closed("sex", SEXES)} onChange={(value) => set("sex", drop(value))} />
        </FormField>
        <FormField label={t("profile.field.gender")} htmlFor="profile-gender">
          <OptionSelect id="profile-gender" value={pick(draft.gender)} options={closed("gender", GENDERS)} onChange={(value) => set("gender", drop(value))} />
        </FormField>
        {draft.gender === "other" && (
          <FormField label={t("profile.field.genderCustom")} htmlFor="profile-gender-custom">
            <Input id="profile-gender-custom" maxLength={CUSTOM_TEXT_MAX} value={draft.genderCustom ?? ""} onChange={(event) => set("genderCustom", event.target.value)} />
          </FormField>
        )}
        <FormField label={t("profile.field.pronouns")} htmlFor="profile-pronouns">
          <OptionSelect id="profile-pronouns" value={pick(draft.pronouns)} options={closed("pronouns", PRONOUNS)} onChange={(value) => set("pronouns", drop(value))} />
        </FormField>
        {draft.pronouns === "custom" && (
          <FormField label={t("profile.field.pronounsCustom")} htmlFor="profile-pronouns-custom">
            <Input id="profile-pronouns-custom" maxLength={CUSTOM_TEXT_MAX} value={draft.pronounsCustom ?? ""} onChange={(event) => set("pronounsCustom", event.target.value)} />
          </FormField>
        )}
        <FormField label={t("profile.field.birthDate")} htmlFor="profile-birth-date" hint={t("profile.birthDate.hint")}>
          <DateParts id="profile-birth-date" value={draft.birthDate} onChange={(value) => set("birthDate", value)} />
        </FormField>
        <FormField label={t("profile.field.country")} htmlFor="profile-country">
          <OptionSelect id="profile-country" value={draft.country ?? UNSET} options={[unset, ...countries]} onChange={(value) => set("country", value === UNSET ? null : value)} />
        </FormField>
        <FormField label={t("profile.field.timezone")} htmlFor="profile-timezone">
          <OptionSelect id="profile-timezone" value={draft.timezone ?? UNSET} options={[unset, ...zones]} onChange={(value) => set("timezone", value === UNSET ? null : value)} />
        </FormField>
        <FormField label={t("profile.field.role")} htmlFor="profile-role">
          <OptionSelect id="profile-role" value={pick(draft.role)} options={closed("role", ROLES)} onChange={(value) => set("role", drop(value))} />
        </FormField>
        <FormField label={t("profile.field.company")} htmlFor="profile-company">
          <Input id="profile-company" autoComplete="organization" maxLength={LONG_TEXT_MAX} value={draft.company ?? ""} onChange={(event) => set("company", event.target.value)} />
        </FormField>
      </div>
      <div className="flex flex-wrap justify-end gap-2">
        {secondary}
        <Button type="submit" disabled={busy || !draft.displayName.trim() || username === "invalid" || username === "taken" || username === "checking"}>{submitLabel}</Button>
      </div>
    </form>
  );
}
