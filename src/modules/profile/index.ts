import { create } from "zustand";
import { supabase } from "@/modules/auth/client";
import { fromRow, normalizeProfile, toRow, type AccountProfile, type ProfileRow } from "./fields";

export * from "./fields";

interface ProfileState {
  profile: AccountProfile | null;
  /** A foto da conta (`profiles.avatar_url`): a que a pessoa escolheu no site
   * ou, sem ela, a do provedor. Só se mostra aqui; trocar é no site. */
  photo: string | null;
  loading: boolean;
}

/** O perfil da conta, lido e gravado direto no Supabase: fica fora da fila de
 * sync porque os membros de uma organização vão ler o perfil uns dos outros. */
export const useProfile = create<ProfileState>(() => ({ profile: null, photo: null, loading: true }));

const COLUMNS = "display_name,username,sex,gender,gender_custom,pronouns,pronouns_custom,birth_date,country,timezone,role,company,completed_at,username_set_at,avatar_url";

type Row = ProfileRow & { avatar_url?: string | null };

/** Só endereço `https` vira foto: o resto cai na inicial do nome. */
export function photoOf(row: { avatar_url?: string | null } | null | undefined) {
  const url = row?.avatar_url?.trim();
  return url && url.startsWith("https://") ? url : null;
}

async function userId() {
  const { data, error } = await supabase.auth.getUser();
  if (error || !data.user) throw error ?? new Error("no session");
  return data.user.id;
}

/** O banco recusou um valor que passou pela tela: nome de usuário tomado
 * (índice único) ou outro `check`. */
function failure(error: { code?: string; message?: string }) {
  if (error.code === "23505") return { key: "profile.usernameTaken" };
  if (error.message === "profile.username.locked") return { key: "profile.username.locked" };
  return error.code === "23514" || error.code === "22007" || error.code === "22008" ? { key: "profile.invalid" } : error;
}

async function write(row: Partial<ProfileRow>) {
  const { data, error } = await supabase.from("profiles").update(row).eq("user_id", await userId()).select(COLUMNS).single();
  if (error) throw failure(error);
  useProfile.setState({ profile: fromRow(data as Row), photo: photoOf(data as Row) });
}

export async function loadProfile() {
  useProfile.setState({ loading: true });
  const { data, error } = await supabase.from("profiles").select(COLUMNS).maybeSingle();
  useProfile.setState({ loading: false, profile: data ? fromRow(data as Row) : null, photo: photoOf(data as Row | null) });
  if (error) throw error;
}

/** Salvar ou pular marcam `completed_at`: o passo de perfil não volta. Salvar
 * também fixa o nome de usuário (o banco guarda só a primeira vez). */
export async function saveProfile(draft: AccountProfile) {
  const now = new Date().toISOString();
  await write({ ...toRow(normalizeProfile(draft)), completed_at: now, username_set_at: now });
}

export async function skipSetup() {
  await write({ completed_at: new Date().toISOString() });
}

/** Pergunta ao banco sem ler perfil nenhum: o próprio nome conta como livre. */
export async function usernameAvailable(name: string) {
  const { data, error } = await supabase.rpc("username_available", { name });
  if (error) throw error;
  return data === true;
}

export function clearProfile() {
  useProfile.setState({ profile: null, photo: null, loading: true });
}
