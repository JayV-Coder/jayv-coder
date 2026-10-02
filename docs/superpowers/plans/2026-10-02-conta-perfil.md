# Conta e perfil — plano de implementação

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** registro com senha forte e perfil, login e vinculação por GitHub, GitLab e Bitbucket, e definição ou troca de senha em qualquer conta.

**Architecture:** uma tabela `public.profiles` com RLS de dono, criada por trigger em `auth.users`, e uma função `account_has_password()`. O React lê e grava o perfil direto pelo supabase-js (módulo novo `profile`); o módulo `auth` ganha os fluxos de provedor, vinculação, senha e recuperação. O núcleo Rust não muda.

**Tech Stack:** Supabase (Postgres, Auth, pgTAP), React 19, zustand, supabase-js 2.117, Vitest (novo), Tailwind/shadcn.

**Spec:** `docs/superpowers/specs/2026-10-02-conta-perfil-design.md`

## Global Constraints

- Identificadores em inglês; comentários podem ser em português (`AGENTS.md`).
- Todo texto de tela pela chave do i18n; chave nova nasce em `src/modules/i18n/messages/en.ts` e ganha tradução nos 10 idiomas (pt-BR, en, es, zh-CN, hi, ar, fr, ru, ja, de) na migração, com `on conflict (locale, key) do update`.
- Senha: ≥ 8 caracteres, minúscula, maiúscula, dígito e símbolo (não letra nem dígito).
- Provedores: `github`, `gitlab`, `bitbucket`. Retorno sempre por `jayv://auth/callback` (PKCE).
- Versão 0.14.2 → 0.15.0 em `package.json`, `package-lock.json` (2 entradas), `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock` (entrada `jayv`).
- Sem Docker na máquina: os testes pgTAP são escritos e rodam no `supabase test db` de quem tiver o stack local; o `db push` é do dono do projeto.

## Review Focus

1. Conta OAuth sem senha tentando desvincular o único provedor → botão desativado e `canUnlink` falso (Task 2).
2. Senha com espaço ou acento como "símbolo" (`Abcdefg1 `, `Abcdefg1ç`) → espaço conta como símbolo, letra acentuada conta como letra, não como símbolo (Task 2).
3. Gênero `other` sem texto, ou texto com gênero ≠ `other` → `normalizeProfile` limpa o texto quando não é `other`; o banco recusa texto órfão (Tasks 1 e 4).
4. Erro do Supabase sem `code` conhecido → cai na mensagem original, sem quebrar (Task 2).
5. Link de recuperação aberto com o app fechado → `getCurrent()` entrega o link e a tela de nova senha abre (Task 3, teste manual).

---

### Task 1: Migração `profiles` e testes pgTAP

**Files:**
- Create: `../supabase/migrations/20261002120600_profiles.sql`
- Modify: `../supabase/tests/rls.test.sql`

**Interfaces:**
- Produces: tabela `public.profiles` (colunas do spec), função `public.account_has_password() returns boolean`.

- [ ] **Step 1: Testes** — em `rls.test.sql`, subir o `plan(14)` para `plan(23)` e acrescentar antes do `finish()`:

```sql
-- Perfil: nasce com a conta, só o dono lê e muda, valores fechados.
reset role;
insert into auth.users (id, email, raw_user_meta_data, encrypted_password) values
  ('00000000-0000-0000-0000-00000000000c', 'carla@teste.local', '{"display_name":"Carla"}', 'x'),
  ('00000000-0000-0000-0000-00000000000d', 'dev.d@teste.local', '{"user_name":"devd"}', '');
select is((select display_name from public.profiles where user_id = '00000000-0000-0000-0000-00000000000c'), 'Carla', 'o registro dá o nome');
select is((select display_name from public.profiles where user_id = '00000000-0000-0000-0000-00000000000d'), 'devd', 'o OAuth dá o nome');
select pg_temp.as_user('00000000-0000-0000-0000-00000000000c');
select is_empty($$select 1 from public.profiles where user_id <> auth.uid()$$, 'ninguém lê o perfil de outro');
select lives_ok($$update public.profiles set gender = 'other', gender_custom = 'agênero' where user_id = auth.uid()$$, 'o dono muda o próprio perfil');
select throws_ok($$update public.profiles set sex = 'x' where user_id = auth.uid()$$, '23514', null, 'valor fora da lista é recusado');
select throws_ok($$update public.profiles set gender = 'woman', gender_custom = 'x' where user_id = auth.uid()$$, '23514', null, 'texto livre só com other');
select throws_ok($$insert into public.profiles (user_id, display_name) values (auth.uid(), 'x')$$, '42501', null, 'o cliente não cria perfil');
select throws_ok($$delete from public.profiles$$, '42501', null, 'o cliente não apaga perfil');
select results_eq($$select public.account_has_password()$$, $$values (true)$$, 'a conta com senha diz que tem');
```

- [ ] **Step 2: Migração** — escrever `20261002120600_profiles.sql`:

```sql
-- O perfil de quem usa o JayV. Uma linha por conta, criada pelo trigger em
-- `auth.users`; o cliente só lê e muda a própria. Os valores fechados são
-- identificadores em inglês: a tela traduz.

create table public.profiles (
  user_id uuid primary key references auth.users on delete cascade,
  display_name text not null check (char_length(btrim(display_name)) between 1 and 60),
  full_name text check (char_length(full_name) <= 120),
  sex text check (sex in ('female', 'male', 'intersex', 'undisclosed')),
  gender text check (gender in ('woman', 'man', 'non_binary', 'other', 'undisclosed')),
  gender_custom text check (char_length(gender_custom) <= 40),
  pronouns text check (pronouns in ('she', 'he', 'they', 'custom', 'undisclosed')),
  pronouns_custom text check (char_length(pronouns_custom) <= 40),
  birth_date date check (birth_date between date '1900-01-01' and current_date),
  country text check (country ~ '^[A-Z]{2}$'),
  timezone text check (char_length(timezone) <= 64),
  role text check (role in ('developer', 'tech_lead', 'qa', 'devops', 'designer', 'manager', 'data', 'other')),
  company text check (char_length(company) <= 120),
  completed_at timestamptz,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  check (gender_custom is null or gender = 'other'),
  check (pronouns_custom is null or pronouns = 'custom')
);

alter table public.profiles enable row level security;
create policy "dono lê" on public.profiles for select to authenticated using (user_id = (select auth.uid()));
create policy "dono muda" on public.profiles for update to authenticated using (user_id = (select auth.uid())) with check (user_id = (select auth.uid()));
revoke insert, delete, truncate on public.profiles from anon, authenticated;
revoke all on public.profiles from anon;

-- `user_id` e `created_at` não mudam; `updated_at` anda a cada escrita.
create function public.profiles_touch() returns trigger language plpgsql set search_path = '' as $$
begin
  new.user_id := old.user_id;
  new.created_at := old.created_at;
  new.updated_at := now();
  return new;
end;
$$;
create trigger touch before update on public.profiles for each row execute function public.profiles_touch();

-- O nome inicial: o do registro, o do provedor ou o começo do e-mail.
create function public.profile_name(meta jsonb, email text) returns text language sql immutable set search_path = '' as $$
  select left(coalesce(
    nullif(btrim(meta->>'display_name'), ''),
    nullif(btrim(meta->>'full_name'), ''),
    nullif(btrim(meta->>'name'), ''),
    nullif(btrim(meta->>'user_name'), ''),
    nullif(btrim(meta->>'preferred_username'), ''),
    nullif(split_part(coalesce(email, ''), '@', 1), ''),
    'user'), 60);
$$;

create function public.profiles_create() returns trigger language plpgsql security definer set search_path = '' as $$
begin
  insert into public.profiles (user_id, display_name)
  values (new.id, public.profile_name(new.raw_user_meta_data, new.email))
  on conflict (user_id) do nothing;
  return new;
end;
$$;
create trigger profiles_create after insert on auth.users for each row execute function public.profiles_create();

insert into public.profiles (user_id, display_name)
select id, public.profile_name(raw_user_meta_data, email) from auth.users
on conflict (user_id) do nothing;

-- A conta OAuth que define senha não ganha identidade `email`: só o banco sabe.
create function public.account_has_password() returns boolean language sql stable security definer set search_path = '' as $$
  select coalesce((select encrypted_password <> '' from auth.users where id = auth.uid()), false);
$$;
revoke execute on function public.account_has_password() from public, anon;
grant execute on function public.account_has_password() to authenticated;
revoke execute on function public.profiles_create(), public.profiles_touch(), public.profile_name(jsonb, text) from public, anon, authenticated;
```

(As traduções entram no fim deste arquivo na Task 8.)

- [ ] **Step 3:** sem Docker, conferir a sintaxe lendo; rodar `supabase test db` onde houver stack local.

### Task 2: Vitest e regras puras (senha, desvincular, erros)

**Files:**
- Modify: `package.json` (devDependency `vitest`, script `"test:web": "vitest run"`)
- Create: `src/modules/auth/password.ts`, `src/modules/auth/identities.ts`, `src/modules/auth/errors.ts`
- Test: `src/modules/auth/password.test.ts`, `src/modules/auth/identities.test.ts`, `src/modules/auth/errors.test.ts`

**Interfaces:**
- Produces:
  - `type PasswordRule = "length" | "lower" | "upper" | "digit" | "symbol"`; `PASSWORD_RULES: PasswordRule[]`; `passwordRules(password: string): Record<PasswordRule, boolean>`; `passwordOk(password: string): boolean`.
  - `type Provider = "github" | "gitlab" | "bitbucket"`; `PROVIDERS: Provider[]`; `canUnlink(providers: string[], hasPassword: boolean, provider: Provider): boolean`.
  - `authFailure(error: unknown): unknown` — devolve `Text` (`{ key }`) para códigos conhecidos e o próprio erro (ou `error.message`) para o resto.

- [ ] **Step 1: Testes que falham**

```ts
// password.test.ts
import { describe, expect, it } from "vitest";
import { passwordOk, passwordRules } from "./password";

describe("passwordRules", () => {
  it("marca cada regra", () => {
    expect(passwordRules("abc")).toEqual({ length: false, lower: true, upper: false, digit: false, symbol: false });
    expect(passwordRules("Abcdefg1!")).toEqual({ length: true, lower: true, upper: true, digit: true, symbol: true });
  });
  it("letra acentuada é letra, espaço é símbolo", () => {
    expect(passwordRules("Ébcdefg1ç").symbol).toBe(false);
    expect(passwordRules("Ébcdefg1ç").upper).toBe(true);
    expect(passwordRules("Abcdefg1 ").symbol).toBe(true);
  });
  it("passwordOk exige todas", () => {
    expect(passwordOk("Abcdefg1!")).toBe(true);
    expect(passwordOk("Abcdefgh!")).toBe(false);
    expect(passwordOk("Ab1!")).toBe(false);
  });
});
```

```ts
// identities.test.ts
import { describe, expect, it } from "vitest";
import { canUnlink } from "./identities";

describe("canUnlink", () => {
  it("recusa a última forma de entrar", () => expect(canUnlink(["github"], false, "github")).toBe(false));
  it("aceita com senha", () => expect(canUnlink(["github"], true, "github")).toBe(true));
  it("aceita com outro provedor", () => expect(canUnlink(["github", "gitlab"], false, "github")).toBe(true));
  it("a identidade email conta como forma de entrar", () => expect(canUnlink(["email", "gitlab"], false, "gitlab")).toBe(true));
  it("não desvincula o que não está vinculado", () => expect(canUnlink(["email"], true, "github")).toBe(false));
});
```

```ts
// errors.test.ts
import { describe, expect, it } from "vitest";
import { authFailure } from "./errors";

describe("authFailure", () => {
  it("traduz códigos conhecidos", () => {
    expect(authFailure({ code: "identity_already_exists", message: "x" })).toEqual({ key: "auth.identityTaken" });
    expect(authFailure({ code: "weak_password", message: "x" })).toEqual({ key: "auth.weakPassword" });
    expect(authFailure({ code: "invalid_credentials", message: "x" })).toEqual({ key: "auth.invalidCredentials" });
  });
  it("mantém a mensagem do resto", () => {
    expect(authFailure(new Error("rede caiu"))).toBe("rede caiu");
    expect(authFailure({ code: "nada", message: "outra" })).toBe("outra");
  });
});
```

- [ ] **Step 2:** `npm i -D vitest` e `npm run test:web` → falha (módulos inexistentes).

- [ ] **Step 3: Implementação**

```ts
// password.ts
/** As regras da senha nova. O servidor confere as mesmas; esta cópia é só
 * para a lista que marca enquanto a pessoa digita. */
export type PasswordRule = "length" | "lower" | "upper" | "digit" | "symbol";
export const PASSWORD_RULES: PasswordRule[] = ["length", "lower", "upper", "digit", "symbol"];
export const PASSWORD_MIN = 8;

export function passwordRules(password: string): Record<PasswordRule, boolean> {
  return {
    length: [...password].length >= PASSWORD_MIN,
    lower: /\p{Ll}/u.test(password),
    upper: /\p{Lu}/u.test(password),
    digit: /\p{Nd}/u.test(password),
    symbol: /[^\p{L}\p{Nd}]/u.test(password),
  };
}

export const passwordOk = (password: string) => Object.values(passwordRules(password)).every(Boolean);
```

```ts
// identities.ts
export type Provider = "github" | "gitlab" | "bitbucket";
export const PROVIDERS: Provider[] = ["github", "gitlab", "bitbucket"];

/** Desvincular só quando sobra outra forma de entrar: a senha ou outra
 * identidade (inclusive a `email`). */
export function canUnlink(providers: string[], hasPassword: boolean, provider: Provider) {
  if (!providers.includes(provider)) return false;
  return hasPassword || providers.some((other) => other !== provider);
}
```

```ts
// errors.ts
import type { Text } from "@/modules/i18n";

/** Os códigos do Supabase Auth que a tela sabe explicar. */
const KNOWN: Record<string, string> = {
  identity_already_exists: "auth.identityTaken",
  weak_password: "auth.weakPassword",
  invalid_credentials: "auth.invalidCredentials",
  same_password: "auth.samePassword",
  reauthentication_not_valid: "auth.badCode",
  otp_expired: "auth.badCode",
  single_identity_not_deletable: "auth.lastIdentity",
  manual_linking_disabled: "auth.linkingDisabled",
  email_not_confirmed: "auth.emailNotConfirmed",
};

export function authFailure(error: unknown): unknown {
  const code = typeof error === "object" && error !== null ? (error as { code?: unknown }).code : undefined;
  if (typeof code === "string" && KNOWN[code]) return { key: KNOWN[code] } satisfies Text;
  if (error instanceof Error) return error.message;
  if (typeof error === "object" && error !== null && typeof (error as { message?: unknown }).message === "string") return (error as { message: string }).message;
  return error;
}
```

O `vitest` precisa do alias `@`: ele lê o `vite.config.ts`, que já o declara.

- [ ] **Step 4:** `npm run test:web` → PASS.

### Task 3: Módulo `auth` — provedores, vinculação, senha e recuperação

**Files:**
- Modify: `src/modules/auth/index.ts`

**Interfaces:**
- Consumes: Task 2 (`Provider`, `canUnlink`, `authFailure`, `passwordOk`).
- Produces (exportados de `@/modules/auth`):
  - estado `useAuth`: `{ status, email, profile, waitingBrowser, recovering: boolean, providers: string[], hasPassword: boolean }`.
  - `signUp(email, password, displayName): Promise<boolean>`; `signInWithProvider(provider: Provider)`; `requestPasswordReset(email)`; `linkProvider(provider)`; `unlinkProvider(provider)`; `loadAccess()`; `changePassword(current, next)`; `sendSetPasswordCode()`; `setFirstPassword(code, password)`; `finishRecovery(password)`; re-exporta `passwordRules`, `passwordOk`, `PASSWORD_RULES`, `PasswordRule`, `PROVIDERS`, `Provider`, `canUnlink`, `authFailure`.
  - Todas as funções lançam `authFailure(error)` (já pronto para `reportError`).

- [ ] **Step 1:** `Profile.provider` passa a ser o provedor do último login; `receive()` continua só trocando o código (serve para login, vinculação e recuperação: o supabase-js sabe qual pelo verificador guardado).
- [ ] **Step 2:** em `connectAuth`, tratar `PASSWORD_RECOVERY`: `hand(session)` e `useAuth.setState({ recovering: true })`. `hand()` chama `loadAccess()` depois de `signedIn`; `signedOut` zera `recovering`, `providers` e `hasPassword`.
- [ ] **Step 3:** implementar:

```ts
const fail = (error: unknown): never => { throw authFailure(error); };

export async function loadAccess() {
  const [identities, password] = await Promise.all([supabase.auth.getUserIdentities(), supabase.rpc("account_has_password")]);
  if (identities.error) fail(identities.error);
  useAuth.setState({
    providers: (identities.data?.identities ?? []).map((identity) => identity.provider),
    hasPassword: password.data === true,
  });
}

export async function signInWithProvider(provider: Provider) {
  const { data, error } = await supabase.auth.signInWithOAuth({ provider, options: { redirectTo: CALLBACK_URL, skipBrowserRedirect: true } });
  if (error) fail(error);
  useAuth.setState({ waitingBrowser: true });
  await openUrl(data.url!);
}

export async function linkProvider(provider: Provider) {
  const { data, error } = await supabase.auth.linkIdentity({ provider, options: { redirectTo: CALLBACK_URL, skipBrowserRedirect: true } });
  if (error) fail(error);
  useAuth.setState({ waitingBrowser: true });
  await openUrl(data.url!);
}

export async function unlinkProvider(provider: Provider) {
  const { providers, hasPassword } = useAuth.getState();
  if (!canUnlink(providers, hasPassword, provider)) fail({ code: "single_identity_not_deletable" });
  const { data } = await supabase.auth.getUserIdentities();
  const identity = data?.identities.find((known) => known.provider === provider);
  if (!identity) return;
  const { error } = await supabase.auth.unlinkIdentity(identity);
  if (error) fail(error);
  await loadAccess();
}

export async function requestPasswordReset(email: string) {
  const { error } = await supabase.auth.resetPasswordForEmail(email, { redirectTo: CALLBACK_URL });
  if (error) fail(error);
}

/** Conferir a senha atual entrando de novo também renova a sessão, que é o
 * que o "secure password change" do Supabase pede. */
export async function changePassword(current: string, next: string) {
  const email = useAuth.getState().email;
  if (!email) fail(new Error("no email"));
  const check = await supabase.auth.signInWithPassword({ email: email!, password: current });
  if (check.error) fail(check.error.code === "invalid_credentials" ? { code: "wrong_password" } : check.error);
  const { error } = await supabase.auth.updateUser({ password: next });
  if (error) fail(error);
}

export async function sendSetPasswordCode() {
  const { error } = await supabase.auth.reauthenticate();
  if (error) fail(error);
}

export async function setFirstPassword(code: string, password: string) {
  const { error } = await supabase.auth.updateUser({ password, nonce: code.trim() });
  if (error) fail(error);
  await loadAccess();
}

export async function finishRecovery(password: string) {
  const { error } = await supabase.auth.updateUser({ password });
  if (error) fail(error);
  useAuth.setState({ recovering: false });
  await loadAccess();
}
```

`errors.ts` ganha `wrong_password: "auth.wrongPassword"` (código interno da troca de senha), com caso no teste.

- [ ] **Step 4:** `signUp` manda `options: { emailRedirectTo: CALLBACK_URL, data: { display_name: displayName.trim() } }`; `signIn` e `signUp` lançam `authFailure`. Remover `signInWithGithub`.
- [ ] **Step 5:** `npm run typecheck` e `npm run test:web` → PASS.

### Task 4: Módulo `profile`

**Files:**
- Create: `src/modules/profile/fields.ts`, `src/modules/profile/index.ts`
- Test: `src/modules/profile/fields.test.ts`

**Interfaces:**
- Produces:
  - `SEXES = ["female","male","intersex","undisclosed"] as const`, `GENDERS = ["woman","man","non_binary","other","undisclosed"] as const`, `PRONOUNS = ["she","he","they","custom","undisclosed"] as const`, `ROLES = ["developer","tech_lead","qa","devops","designer","manager","data","other"] as const`, e os tipos `Sex`, `Gender`, `Pronouns`, `Role`.
  - `interface AccountProfile { displayName: string; fullName: string|null; sex: Sex|null; gender: Gender|null; genderCustom: string|null; pronouns: Pronouns|null; pronounsCustom: string|null; birthDate: string|null; country: string|null; timezone: string|null; role: Role|null; company: string|null; completedAt: string|null }`
  - `normalizeProfile(draft: AccountProfile): AccountProfile` (trim, vazio → null, limpa o texto livre que não vale).
  - `toRow(profile)` / `fromRow(row)` (snake_case ↔ camelCase).
  - `useProfile` (`{ profile: AccountProfile | null; loading: boolean }`), `loadProfile()`, `saveProfile(draft)`, `skipSetup()`, `clearProfile()`.

- [ ] **Step 1: Teste que falha**

```ts
import { describe, expect, it } from "vitest";
import { fromRow, normalizeProfile, toRow, type AccountProfile } from "./fields";

const base: AccountProfile = { displayName: "Ana", fullName: null, sex: null, gender: null, genderCustom: null, pronouns: null, pronounsCustom: null, birthDate: null, country: null, timezone: null, role: null, company: null, completedAt: null };

describe("normalizeProfile", () => {
  it("apara e troca vazio por null", () => {
    expect(normalizeProfile({ ...base, displayName: "  Ana ", company: "  " })).toMatchObject({ displayName: "Ana", company: null });
  });
  it("limpa o texto livre quando a escolha não é a livre", () => {
    expect(normalizeProfile({ ...base, gender: "woman", genderCustom: "x" }).genderCustom).toBeNull();
    expect(normalizeProfile({ ...base, gender: "other", genderCustom: " agênero " }).genderCustom).toBe("agênero");
    expect(normalizeProfile({ ...base, pronouns: "she", pronounsCustom: "x" }).pronounsCustom).toBeNull();
  });
  it("país sempre em maiúsculas", () => expect(normalizeProfile({ ...base, country: "br" }).country).toBe("BR"));
});

describe("linhas", () => {
  it("ida e volta", () => expect(fromRow(toRow({ ...base, role: "qa" }) as never)).toMatchObject({ displayName: "Ana", role: "qa" }));
});
```

- [ ] **Step 2:** `npm run test:web` → FAIL.
- [ ] **Step 3:** implementar `fields.ts` (constantes, tipos, `normalizeProfile`, `toRow` sem `completed_at`, `fromRow`) e `index.ts`:

```ts
export async function loadProfile() {
  useProfile.setState({ loading: true });
  const { data, error } = await supabase.from("profiles").select("*").maybeSingle();
  useProfile.setState({ loading: false, profile: data ? fromRow(data) : null });
  if (error) throw error;
}

/** Salvar ou pular marcam `completed_at`: o passo de perfil não volta. */
export async function saveProfile(draft: AccountProfile) {
  const row = { ...toRow(normalizeProfile(draft)), completed_at: new Date().toISOString() };
  const { data, error } = await supabase.from("profiles").update(row).eq("user_id", await userId()).select("*").single();
  if (error) throw error.code === "23514" || error.code === "22007" ? { key: "profile.invalid" } : error;
  useProfile.setState({ profile: fromRow(data) });
}

export async function skipSetup() { /* update só de completed_at, mesmo tratamento */ }
```

`userId()` lê `supabase.auth.getUser()`. O `App` chama `loadProfile()` no login e `clearProfile()` no logout.

- [ ] **Step 4:** `npm run test:web` → PASS.

### Task 5: Peças de tela — `ProviderIcon`, `PasswordRules`, `ProviderButton`, `ProfileForm`

**Files:**
- Create: `src/components/atoms/icons/GitlabIcon.tsx`, `src/components/atoms/icons/BitbucketIcon.tsx` (e `GithubIcon.tsx` se não existir), `src/components/atoms/ProviderIcon.tsx`
- Create: `src/components/molecules/PasswordRules.tsx`, `src/components/molecules/ProviderButton.tsx`
- Create: `src/components/organisms/ProfileForm.tsx`
- Modify: os `index.ts` de atoms, molecules e organisms

**Interfaces:**
- Produces:
  - `ProviderIcon({ provider }: { provider: Provider } & SVGProps<SVGSVGElement>)`.
  - `PasswordRules({ password }: { password: string })` — lista `PASSWORD_RULES` com ✓/○ e `t("auth.rule.<regra>", { min: 8 })`, `aria-live="polite"`.
  - `ProviderButton({ provider, onClick, disabled, label })` — `Button variant="outline"` com ícone.
  - `ProfileForm({ initial, busy, onSubmit, secondary }: { initial: AccountProfile; busy: boolean; onSubmit: (draft: AccountProfile) => void; secondary?: ReactNode })` — campos do spec em grade de 2 colunas (`FormField`, `Input`, `OptionSelect`); país e fuso com `OptionSelect` alimentado por `Intl.DisplayNames(locale, { type: "region", fallback: "none" })` sobre os códigos A–Z e `Intl.supportedValuesOf("timeZone")`; `gender_custom` só aparece com `other`, `pronouns_custom` só com `custom`; data com `<Input type="date" max={hoje} min="1900-01-01">`; botão Salvar desativado com `displayName` vazio.
- [ ] **Step 1:** implementar; `npm run typecheck` → PASS.

### Task 6: Login, nova senha e passo de perfil

**Files:**
- Modify: `src/components/pages/LoginPage.tsx`, `src/app/App.tsx`, `src/components/pages/index.ts`
- Create: `src/components/pages/NewPasswordPage.tsx`, `src/components/pages/ProfileSetupPage.tsx`

**Interfaces:**
- Consumes: Tasks 3–5.

- [ ] **Step 1: LoginPage** — `mode: "signIn" | "signUp" | "reset"`. Criar conta: e-mail, nome de exibição (obrigatório, até 60), senha + `PasswordRules`, confirmação (erro `auth.mismatch` quando diferente); envio só com `passwordOk` e senhas iguais. Entrar: e-mail, senha, link "Esqueci minha senha" → modo `reset` (só e-mail; ao enviar, nota `auth.resetSent` e volta a `signIn`). Três `ProviderButton`; o que foi clicado mostra `auth.waitingBrowser`. Erros por `reportError(error)` (já vêm de `authFailure`).
- [ ] **Step 2: NewPasswordPage** — senha + `PasswordRules` + confirmação; Salvar chama `finishRecovery`; Cancelar chama `signOut`.
- [ ] **Step 3: ProfileSetupPage** — título `profile.setup.title`, descrição `profile.setup.description`; `ProfileForm` com `initial` do perfil carregado, `timezone` preenchido com `Intl.DateTimeFormat().resolvedOptions().timeZone` quando vazio, `secondary` = botão Pular (`skipSetup`).
- [ ] **Step 4: App.tsx** — com `signedIn`: `recovering` → `NewPasswordPage`; perfil ainda carregando → nota `auth.loading`; perfil com `completedAt` nulo → `ProfileSetupPage`; senão `AppShell`. O efeito de login também chama `loadProfile().catch(reportError)`; no logout, `clearProfile()`.
- [ ] **Step 5:** `npm run typecheck` → PASS.

### Task 7: Página de Perfil — dados, segurança e contas vinculadas

**Files:**
- Create: `src/components/organisms/SecurityPanel.tsx`, `src/components/organisms/LinkedAccounts.tsx`
- Modify: `src/components/organisms/ProfileCard.tsx`, `src/components/organisms/Sidebar.tsx`, `src/components/pages/ProfilePage.tsx`, `src/components/organisms/index.ts`

**Interfaces:**
- `displayName(account: AccountProfile | null, profile: Profile | null, email: string | null)` — prefere `account.displayName`.

- [ ] **Step 1: ProfileCard / Sidebar** — usam o `displayName` novo; o card mostra um selo por provedor vinculado (`ProviderIcon` + nome) e, se `hasPassword`, o selo `profile.provider.email`.
- [ ] **Step 2: SecurityPanel** (`SettingsSection` `security.title`): com `hasPassword` → senha atual, nova + `PasswordRules`, confirmação, botão `security.change` → `changePassword` → `notify(t("security.changed"))`. Sem senha → `security.noPassword` + botão `security.sendCode` → `sendSetPasswordCode` → aparece campo do código (6 dígitos, `inputMode="numeric"`), nova senha + regras, confirmação, `security.setPassword` → `setFirstPassword` → `notify(t("security.set"))`.
- [ ] **Step 3: LinkedAccounts** (`SettingsSection` `linked.title`): uma linha por `PROVIDERS` com ícone, nome e estado (`linked.on` / `linked.off`); Vincular (`linkProvider`) ou Desvincular (`ConfirmAction` → `unlinkProvider`), desativado com `title={t("auth.lastIdentity")}` quando `!canUnlink(...)`.
- [ ] **Step 4: ProfilePage** — depois do card: `SettingsSection` `profile.data.title` com `ProfileForm` (`onSubmit` → `saveProfile` → `notify(t("profile.saved"))`), depois `SecurityPanel` e `LinkedAccounts` lado a lado em telas largas; o resto continua como está.
- [ ] **Step 5:** `npm run typecheck` → PASS.

### Task 8: Textos — `en.ts` e traduções nos 10 idiomas

**Files:**
- Modify: `src/modules/i18n/messages/en.ts`
- Modify: `../supabase/migrations/20261002120600_profiles.sql` (bloco de traduções no fim)

- [ ] **Step 1:** acrescentar em `en.ts` todas as chaves usadas nas Tasks 2–7 (`auth.*`, `auth.rule.*`, `auth.provider.*`, `profile.setup.*`, `profile.field.*`, `profile.sex.*`, `profile.gender.*`, `profile.pronouns.*`, `profile.role.*`, `profile.data.*`, `profile.saved`, `profile.invalid`, `profile.skip`, `profile.save`, `security.*`, `linked.*`). Remover `auth.github` e trocar pelo parametrizado `auth.continueWith` (`{provider}`), que é chave nova.
- [ ] **Step 2:** `npm run typecheck` (chave inexistente quebra o tipo `Key`).
- [ ] **Step 3:** gerar o bloco `insert into public.translations ... on conflict (locale, key) do update set value = excluded.value;` com cada chave nos 10 idiomas, e conferir por script que cada chave nova de `en.ts` aparece 10 vezes na migração.

### Task 9: Verificação, versão, commit e push

- [ ] **Step 1:** `npm run test:web`, `npm run typecheck`, `npm run check` → tudo PASS.
- [ ] **Step 2:** `npm run dev` e percorrer à mão: registro (regras, confirmação), passo de perfil (salvar e pular), troca de senha, definir senha por código, recuperação, vincular e desvincular. O que depender do painel do Supabase (GitLab, Bitbucket, manual linking) fica registrado como não verificado se ainda não estiver ativo.
- [ ] **Step 3:** versão 0.15.0 nos cinco lugares.
- [ ] **Step 4:** commit e push do `supabase` (`feat: perfil da conta e senha das contas OAuth`), depois do `jayv-coder` (`v0.15.0: registro com senha forte, perfil e contas vinculadas`, corpo por seção conforme `AGENTS.md`).
