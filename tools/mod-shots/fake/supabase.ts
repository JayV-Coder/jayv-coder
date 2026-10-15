// O Supabase do app, de mentira: a conta já entrou, o plano libera tudo e as
// tabelas que a tela lê devolvem os dados de exemplo de `fixtures.ts`.
import { FEATURES } from "@/modules/plans/catalog";
import { PROFILE, USER } from "./fixtures";

export const SUPABASE_URL = "https://studio.invalid";
export const SUPABASE_KEY = "studio";
export const CALLBACK_URL = "jayv://auth/callback";

const session = { access_token: "studio-token", refresh_token: "studio", expires_in: 3600, expires_at: Math.floor(Date.now() / 1000) + 3600, token_type: "bearer", user: USER };

/** As linhas de cada tabela. */
const TABLES: Record<string, unknown[]> = {
  profiles: [PROFILE],
  organization_members: [],
  organization_repositories: [],
  organization_llm_policies: [],
  organization_git_connections: [],
  notifications: [],
  plans: [{ key: "pro", name: "Pro", description: "", position: 1, active: true, is_default: false, stripe_price_id: null, price_cents: 0, currency: "usd", billing_interval: "month" }],
  plan_features: [],
  features: [],
  subscriptions: [],
};

/** O plano de exemplo libera todo recurso do catálogo dos planos. */
const ALL_FEATURES = [...FEATURES];

const RPCS: Record<string, unknown> = {
  my_features: { plan: "pro", admin: false, features: ALL_FEATURES, locked: [] },
  account_has_password: true,
  username_available: true,
  mark_notifications_read: null,
  clear_notifications: null,
};

/** Uma consulta encadeável (`select().eq().order()…`) que no fim dá as linhas
 * da tabela; `single`/`maybeSingle` dão a primeira. */
function query(table: string) {
  let single = false;
  const rows = () => TABLES[table] ?? [];
  const builder: Record<string, unknown> = {};
  const chain = () => builder;
  for (const name of ["select", "eq", "neq", "in", "is", "order", "limit", "range", "match", "filter", "or", "gte", "lte", "like", "ilike", "update", "insert", "upsert", "delete"]) builder[name] = chain;
  builder.single = () => { single = true; return builder; };
  builder.maybeSingle = () => { single = true; return builder; };
  builder.then = (resolve: (value: unknown) => unknown, reject?: (error: unknown) => unknown) =>
    Promise.resolve({ data: single ? (rows()[0] ?? null) : rows(), error: null, count: rows().length }).then(resolve, reject);
  return builder;
}

function channel() {
  const value = { on: () => value, subscribe: () => value, unsubscribe: async () => "ok" };
  return value;
}

export const supabase = {
  auth: {
    onAuthStateChange(callback: (event: string, value: unknown) => void) {
      setTimeout(() => callback("SIGNED_IN", session), 0);
      return { data: { subscription: { unsubscribe() {} } } };
    },
    getSession: async () => ({ data: { session }, error: null }),
    getUser: async () => ({ data: { user: USER }, error: null }),
    getUserIdentities: async () => ({ data: { identities: [{ provider: "email" }] }, error: null }),
    refreshSession: async () => ({ data: { session }, error: null }),
    signOut: async () => ({ error: null }),
    signInWithPassword: async () => ({ data: { session }, error: null }),
    signInWithOAuth: async () => ({ data: {}, error: null }),
    signUp: async () => ({ data: {}, error: null }),
    resetPasswordForEmail: async () => ({ data: {}, error: null }),
    updateUser: async () => ({ data: { user: USER }, error: null }),
    exchangeCodeForSession: async () => ({ data: { session }, error: null }),
    mfa: {
      getAuthenticatorAssuranceLevel: async () => ({ data: { currentLevel: "aal1", nextLevel: "aal1" }, error: null }),
      listFactors: async () => ({ data: { totp: [], all: [] }, error: null }),
      challengeAndVerify: async () => ({ data: {}, error: null }),
      enroll: async () => ({ data: {}, error: null }),
      unenroll: async () => ({ data: {}, error: null }),
    },
  },
  from: query,
  rpc: async (name: string) => ({ data: RPCS[name] ?? null, error: null }),
  channel,
  removeChannel: async () => "ok",
  functions: { invoke: async () => ({ data: null, error: null }) },
  storage: { from: () => ({ getPublicUrl: () => ({ data: { publicUrl: "" } }) }) },
};
