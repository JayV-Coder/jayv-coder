# Conta e perfil — design

Etapa A de 4. As outras são: B, organizações (membros, papéis e repositórios
do GitHub, GitLab e Bitbucket); C, política de LLM por organização e por
projeto; D, painel da organização com estatísticas e portaria filtradas por
projeto e por usuário. Cada etapa tem spec, plano e entrega próprios, e
termina com a versão nova, o commit e o push no `main`.

## Objetivo

- Registro por e-mail com senha forte e um perfil com dados de quem usa o
  app, sem documentos (CPF, RG), endereço nem telefone.
- Quem entrou pelo GitHub, GitLab ou Bitbucket pode definir uma senha.
- Quem se registrou por e-mail pode vincular o GitHub, o GitLab ou o
  Bitbucket.
- Login também pelo GitLab e pelo Bitbucket, além do GitHub.

## Regra da senha

Pelo menos 8 caracteres, com letra minúscula, letra maiúscula, dígito e
caractere especial (qualquer caractere que não seja letra nem dígito). A regra
vale em dois lugares: na tela (`src/modules/auth/password.ts`, para a lista que
marca em tempo real) e no servidor (a política de senha do Supabase, que é
quem decide de verdade). Senhas antigas fora da regra continuam funcionando, e
a regra é exigida na próxima troca.

## Dados — repositório `supabase`

Migração `20261002120600_profiles.sql`.

### `public.profiles`

| Coluna | Tipo | Regra |
|---|---|---|
| `user_id` | `uuid` pk | `references auth.users on delete cascade` |
| `display_name` | `text not null` | 1 a 60 caracteres depois do `trim` |
| `full_name` | `text` | até 120 caracteres |
| `sex` | `text` | `female`, `male`, `intersex`, `undisclosed` |
| `gender` | `text` | `woman`, `man`, `non_binary`, `other`, `undisclosed` |
| `gender_custom` | `text` | até 40; só com `gender = 'other'` |
| `pronouns` | `text` | `she`, `he`, `they`, `custom`, `undisclosed` |
| `pronouns_custom` | `text` | até 40; só com `pronouns = 'custom'` |
| `birth_date` | `date` | entre `1900-01-01` e `current_date` |
| `country` | `text` | `^[A-Z]{2}$` (ISO 3166-1 alfa-2) |
| `timezone` | `text` | até 64 (nome IANA, preenchido pelo sistema) |
| `role` | `text` | `developer`, `tech_lead`, `qa`, `devops`, `designer`, `manager`, `data`, `other` |
| `company` | `text` | até 120 caracteres |
| `completed_at` | `timestamptz` | nulo até salvar ou pular o passo de perfil |
| `created_at` | `timestamptz not null default now()` | |
| `updated_at` | `timestamptz not null default now()` | trigger atualiza a cada `update` |

Todos os campos menos `display_name` são opcionais. O banco guarda só
identificadores em inglês; a tela traduz pelo i18n (`profile.sex.female`,
`profile.role.tech_lead` etc.).

- **RLS**: o dono lê (`select`) e altera (`update`) a própria linha.
  `insert`, `delete` e `truncate` são revogados de `anon` e `authenticated`:
  a linha nasce com a conta e morre com ela. `user_id` e `created_at` não
  mudam (trigger recusa).
- **Trigger em `auth.users` (`after insert`)**, função `security definer`:
  cria o perfil com `display_name` vindo de `raw_user_meta_data`, na ordem
  `display_name` (registro por e-mail), `full_name`, `name`, `user_name`,
  `preferred_username` (OAuth); sem nenhum, o começo do e-mail; cortado em 60.
- **Contas existentes**: a migração cria o perfil de cada uma pela mesma
  regra, com `completed_at` nulo. Elas verão o passo de perfil uma vez, e
  podem pulá-lo.

### `public.account_has_password()`

`returns boolean`, `security definer`, `stable`, `set search_path = ''`:
`encrypted_password` não vazio em `auth.users` para `auth.uid()`. Só fala do
próprio usuário; `execute` só para `authenticated`. A tela precisa dela porque
a conta OAuth que define uma senha não ganha identidade `email`, então as
identidades não dizem se há senha.

### Traduções

Toda chave nova da tela entra na mesma migração, nos 10 idiomas de
`public.locales`, com `on conflict (locale, key) do update`.

### Configuração no painel do Supabase (manual, pelo dono do projeto)

1. Authentication → Providers: ativar **GitLab** e **Bitbucket** (criar o
   OAuth app em cada provedor com o callback
   `https://exvsozyemolrjbjetqww.supabase.co/auth/v1/callback`).
2. Authentication → Sign In / Providers: ativar **Allow manual linking**.
3. Authentication → Policies (Password): mínimo **8**; exigir **minúsculas,
   maiúsculas, dígitos e símbolos**.
4. **Secure password change** ligado; **Leaked password protection** se o
   plano permitir.
5. Redirect URLs: `jayv://auth/callback` (já existe).

## Telas e fluxos — repositório `jayv-coder`

Só React. O núcleo Rust não muda: a sessão continua passando pelo
`commands.setSession`.

### `src/modules/auth`

- `password.ts`: `passwordRules(password)` devolve o estado de cada regra
  (`length`, `lower`, `upper`, `digit`, `symbol`) e `passwordOk(password)`.
- `identities.ts`: `canUnlink(identities, hasPassword, provider)` — a regra
  da última forma de entrar, pura.
- `signInWithProvider(provider)` substitui `signInWithGithub`;
  `provider` é `"github" | "gitlab" | "bitbucket"`.
- `signUp(email, password, displayName)`: manda `display_name` em
  `options.data`.
- `requestPasswordReset(email)`: `resetPasswordForEmail` com
  `redirectTo: CALLBACK_URL`.
- `linkProvider(provider)`: `linkIdentity` com `skipBrowserRedirect` e o
  navegador; o retorno pelo deep link troca o código como no login.
- `unlinkProvider(provider)`: recusa antes de chamar se `canUnlink` falhar.
- `loadAccess()`: `getUserIdentities` + `account_has_password()`; guarda
  `identities` e `hasPassword` no `useAuth`.
- `changePassword(current, next)`: confere `current` com
  `signInWithPassword` no e-mail da conta e só então `updateUser`.
- `sendSetPasswordCode()`: `reauthenticate()` (código de 6 dígitos no
  e-mail). `setFirstPassword(code, password)`: `updateUser({ password,
  nonce })`.
- O `status` ganha `recovering`: o evento `PASSWORD_RECOVERY` deixa de ser
  ignorado e abre a tela de nova senha; salvar a senha leva a `signedIn`.

### `src/modules/profile` (novo)

Store `useProfile` com `profile`, `loadProfile()`, `saveProfile(changes)`
(grava e marca `completed_at`) e `skipSetup()` (só marca `completed_at`).
Lê e grava `public.profiles` pelo supabase-js. Fica fora da fila de sync: a
etapa B precisa que membros da mesma organização leiam o perfil uns dos
outros, e login, registro e vinculação já exigem rede.

### Telas

- **LoginPage**: abas Entrar / Criar conta. O registro pede e-mail, nome de
  exibição, senha com `PasswordRules` e confirmação; o envio só ativa com
  todas as regras e as duas senhas iguais. Botões GitHub, GitLab e Bitbucket
  (`ProviderButton`). Link "Esqueci minha senha", que pede o e-mail e envia a
  recuperação.
- **NewPasswordPage**: aberta no `status = recovering`; nova senha com
  `PasswordRules` e confirmação.
- **ProfileSetupPage**: depois do login, enquanto `completed_at` for nulo,
  antes do `AppShell`. `ProfileForm` com Salvar e Pular; vem preenchido com o
  que o OAuth trouxe (nome) e o fuso do sistema.
- **ProfilePage**: o `ProfileCard` mostra o `display_name` do perfil e os
  provedores vinculados; seções novas **Dados pessoais** (`ProfileForm`),
  **Segurança** (`SecurityPanel`: troca de senha com a atual, ou "Definir
  senha" com o código do e-mail) e **Contas vinculadas** (`LinkedAccounts`:
  Vincular ou Desvincular cada provedor; Desvincular desativado, com dica,
  quando é a última forma de entrar).

Componentes novos: molecules `PasswordRules`, `ProviderButton`; atom
`ProviderIcon`; organisms `ProfileForm`, `SecurityPanel`, `LinkedAccounts`;
pages `ProfileSetupPage`, `NewPasswordPage`.

## Regras de segurança

1. Trocar a senha exige a senha atual.
2. Definir a primeira senha exige o código de 6 dígitos enviado ao e-mail.
3. Desvincular um provedor só se sobrar outra forma de entrar (senha ou outro
   provedor).
4. Vincular um provedor já ligado a outra conta é recusado com erro claro;
   contas nunca se fundem.
5. A recuperação de senha volta pelo deep link e abre a tela de nova senha.

## Erros

Todo erro passa pelo `reportError` com chave de i18n:

| Situação | Chave |
|---|---|
| Provedor já vinculado a outra conta | `auth.identityTaken` |
| Senha atual errada | `auth.wrongPassword` |
| Código inválido ou expirado | `auth.badCode` |
| Senha recusada pela política do servidor | `auth.weakPassword` |
| Última forma de entrar | `auth.lastIdentity` |
| Valor recusado pelos `check` do perfil | `profile.invalid` |

Erro desconhecido cai em `error.unexpected` com o motivo técnico.

## Testes

- **SQL** (`supabase/tests/rls.test.sql`): ninguém lê nem altera o perfil de
  outro; `insert` e `delete` pelo cliente recusados; o trigger cria o perfil
  no cadastro com o nome certo; os `check` recusam valores fora da lista;
  `account_has_password()` só fala do próprio usuário.
- **Vitest** (devDependency nova, script `test:web`): `passwordRules`,
  `passwordOk` e `canUnlink`.
- `npm run typecheck`, `npm run check` e os fluxos à mão no app: registro,
  passo de perfil, troca e definição de senha, recuperação, vincular e
  desvincular.

## Entrega

- Versão **0.14.2 → 0.15.0** (MINOR), nos cinco lugares do `AGENTS.md`.
- Commit e push no `main` do `supabase` (migração e testes) e do
  `jayv-coder` (tela, spec e plano).
- O `supabase db push` para o remoto é feito pelo dono do projeto, antes de
  usar o app novo.

## Fora do escopo

Organizações (etapa B), política de LLM (C), painel da organização (D) e a
configuração automática do painel do Supabase.

## Revisão 0.16.0

- `full_name` saiu; entrou `username` (obrigatório, único, 3 a 30 caracteres
  `^[a-z0-9][a-z0-9_-]{1,28}[a-z0-9]$`, guardado em minúsculas). Gerado pelo
  banco a partir do apelido do provedor ou do começo do e-mail, com sufixo na
  colisão (`ana`, `ana-2`); a tela confere a disponibilidade pela RPC
  `username_available(name)`, que só devolve sim ou não. Migração
  `20261002120700_username.sql`. A busca de outras pessoas pelo nome de
  usuário chega com as organizações (etapa B).
- Data de nascimento em três escolhas (dia, mês por extenso, ano decrescente
  desde 1900), com o dia ajustado ao mês e ao ano; só grava completa.
- Página de Perfil em abas: Visão geral (nível, atividade e limites dos
  planos), Dados pessoais, Segurança e Contas vinculadas; o card fica acima.
