import { create } from "zustand";
import { supabase } from "@/modules/auth/client";
import { fromRow, normalizeProfile, toRow, type AccountProfile, type ProfileRow } from "./fields";

export * from "./fields";

interface ProfileState {
  profile: AccountProfile | null;
  loading: boolean;
}

/** O perfil da conta, lido e gravado direto no Supabase: fica fora da fila de
 * sync porque os membros de uma organização vão ler o perfil uns dos outros. */
export const useProfile = create<ProfileState>(() => ({ profile: null, loading: true }));

const COLUMNS = "display_name,full_name,sex,gender,gender_custom,pronouns,pronouns_custom,birth_date,country,timezone,role,company,completed_at";

async function userId() {
  const { data, error } = await supabase.auth.getUser();
  if (error || !data.user) throw error ?? new Error("no session");
  return data.user.id;
}

/** O `check` do banco recusou um valor que passou pela tela. */
function failure(error: { code?: string }) {
  return error.code === "23514" || error.code === "22007" || error.code === "22008" ? { key: "profile.invalid" } : error;
}

async function write(row: Partial<ProfileRow>) {
  const { data, error } = await supabase.from("profiles").update(row).eq("user_id", await userId()).select(COLUMNS).single();
  if (error) throw failure(error);
  useProfile.setState({ profile: fromRow(data as ProfileRow) });
}

export async function loadProfile() {
  useProfile.setState({ loading: true });
  const { data, error } = await supabase.from("profiles").select(COLUMNS).maybeSingle();
  useProfile.setState({ loading: false, profile: data ? fromRow(data as ProfileRow) : null });
  if (error) throw error;
}

/** Salvar ou pular marcam `completed_at`: o passo de perfil não volta. */
export async function saveProfile(draft: AccountProfile) {
  await write({ ...toRow(normalizeProfile(draft)), completed_at: new Date().toISOString() });
}

export async function skipSetup() {
  await write({ completed_at: new Date().toISOString() });
}

export function clearProfile() {
  useProfile.setState({ profile: null, loading: true });
}
