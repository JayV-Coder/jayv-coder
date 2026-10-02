/** Os valores fechados do perfil, iguais aos `check` de `public.profiles`. São
 * identificadores em inglês: a tela traduz (`profile.sex.female` etc.). */
export const SEXES = ["female", "male", "intersex", "undisclosed"] as const;
export const GENDERS = ["woman", "man", "non_binary", "other", "undisclosed"] as const;
export const PRONOUNS = ["she", "he", "they", "custom", "undisclosed"] as const;
export const ROLES = ["developer", "tech_lead", "qa", "devops", "designer", "manager", "data", "other"] as const;

export type Sex = (typeof SEXES)[number];
export type Gender = (typeof GENDERS)[number];
export type Pronouns = (typeof PRONOUNS)[number];
export type Role = (typeof ROLES)[number];

export const DISPLAY_NAME_MAX = 60;
export const LONG_TEXT_MAX = 120;
export const CUSTOM_TEXT_MAX = 40;

export interface AccountProfile {
  displayName: string;
  fullName: string | null;
  sex: Sex | null;
  gender: Gender | null;
  genderCustom: string | null;
  pronouns: Pronouns | null;
  pronounsCustom: string | null;
  /** `YYYY-MM-DD`. */
  birthDate: string | null;
  /** ISO 3166-1 alfa-2. */
  country: string | null;
  /** Nome IANA. */
  timezone: string | null;
  role: Role | null;
  company: string | null;
  completedAt: string | null;
}

export interface ProfileRow {
  display_name: string;
  full_name: string | null;
  sex: Sex | null;
  gender: Gender | null;
  gender_custom: string | null;
  pronouns: Pronouns | null;
  pronouns_custom: string | null;
  birth_date: string | null;
  country: string | null;
  timezone: string | null;
  role: Role | null;
  company: string | null;
  completed_at?: string | null;
}

const clean = (value: string | null) => (value?.trim() ? value.trim() : null);

/** O rascunho como o banco o aceita: sem espaços sobrando, vazio vira nulo e
 * o texto livre só fica quando a escolha é a livre. */
export function normalizeProfile(draft: AccountProfile): AccountProfile {
  return {
    ...draft,
    displayName: draft.displayName.trim(),
    fullName: clean(draft.fullName),
    genderCustom: draft.gender === "other" ? clean(draft.genderCustom) : null,
    pronounsCustom: draft.pronouns === "custom" ? clean(draft.pronounsCustom) : null,
    birthDate: clean(draft.birthDate),
    country: clean(draft.country)?.toUpperCase() ?? null,
    timezone: clean(draft.timezone),
    company: clean(draft.company),
  };
}

/** A linha para gravar; `completed_at` fica de fora, quem grava decide. */
export function toRow(profile: AccountProfile): ProfileRow {
  return {
    display_name: profile.displayName,
    full_name: profile.fullName,
    sex: profile.sex,
    gender: profile.gender,
    gender_custom: profile.genderCustom,
    pronouns: profile.pronouns,
    pronouns_custom: profile.pronounsCustom,
    birth_date: profile.birthDate,
    country: profile.country,
    timezone: profile.timezone,
    role: profile.role,
    company: profile.company,
  };
}

export function fromRow(row: ProfileRow): AccountProfile {
  return {
    displayName: row.display_name,
    fullName: row.full_name,
    sex: row.sex,
    gender: row.gender,
    genderCustom: row.gender_custom,
    pronouns: row.pronouns,
    pronounsCustom: row.pronouns_custom,
    birthDate: row.birth_date,
    country: row.country,
    timezone: row.timezone,
    role: row.role,
    company: row.company,
    completedAt: row.completed_at ?? null,
  };
}
