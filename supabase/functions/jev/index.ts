// O Jev do JayV. Recebe `{ set, state, include? }` com o JWT do usuário,
// conta a chamada no limite diário, monta as perguntas do conjunto a partir de
// `jev_questions` e repassa à TypeSafe com a chave que só existe aqui.
//
// O app nunca manda perguntas: se mandasse, a chave viraria API de uso livre
// para qualquer conta do projeto.
import { createClient } from "npm:@supabase/supabase-js@2";

const SETS = ["entry", "routing", "verification", "asking"];
const TYPESAFE_URL = `${Deno.env.get("TYPESAFE_BASE_URL") ?? "https://api.typesafe.ai"}/v1/systemone`;
const MODEL = Deno.env.get("TYPESAFE_DEFAULT_MODEL") ?? "jev-latest";
const DAILY_LIMIT = Number(Deno.env.get("JEV_DAILY_LIMIT") ?? 500);

function reply(status: number, body: unknown, headers: HeadersInit = {}): Response {
  return new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json", ...headers } });
}

function refuse(status: number, code: string, message: string): Response {
  return reply(status, { error: message, code });
}

Deno.serve(async (request) => {
  if (request.method !== "POST") return refuse(405, "method", "só POST");

  const authorization = request.headers.get("Authorization") ?? "";
  const token = authorization.replace(/^Bearer\s+/i, "");
  if (!token) return refuse(401, "session", "sem sessão");

  const supabase = createClient(Deno.env.get("SUPABASE_URL")!, Deno.env.get("SUPABASE_ANON_KEY")!, {
    global: { headers: { Authorization: `Bearer ${token}` } },
    auth: { persistSession: false, autoRefreshToken: false },
  });
  const { data: claims, error: invalid } = await supabase.auth.getClaims(token);
  if (invalid || !claims?.claims?.sub) return refuse(401, "session", "sessão inválida ou expirada");

  let input: { set?: unknown; state?: unknown; include?: unknown };
  try {
    input = await request.json();
  } catch {
    return refuse(400, "body", "o corpo não é JSON");
  }
  const set = String(input.set ?? "");
  if (!SETS.includes(set)) return refuse(400, "set", `conjunto desconhecido: ${set}`);
  if (typeof input.state !== "object" || input.state === null) return refuse(400, "state", "falta o estado");
  const include = Array.isArray(input.include) ? input.include.map(String) : null;

  const { data: allowed, error: counting } = await supabase.rpc("jev_take_call", { daily_limit: DAILY_LIMIT });
  if (counting) return refuse(500, "usage", `não foi possível contar o uso: ${counting.message}`);
  if (!allowed) return refuse(429, "daily_limit", "limite diário do Jev atingido");

  const { data: rows, error: reading } = await supabase
    .from("jev_questions")
    .select("id, body")
    .eq("question_set", set)
    .order("position");
  if (reading) return refuse(500, "questions", `não foi possível ler as perguntas: ${reading.message}`);
  const questions = Object.fromEntries((rows ?? []).filter((row) => !include || include.includes(row.id)).map((row) => [row.id, row.body]));
  if (Object.keys(questions).length === 0) return refuse(422, "questions", `o conjunto ${set} não tem perguntas`);

  const key = Deno.env.get("TYPESAFE_API_KEY");
  if (!key) return refuse(500, "config", "a função está sem a chave da TypeSafe");

  let upstream: Response;
  try {
    upstream = await fetch(TYPESAFE_URL, {
      method: "POST",
      headers: { "Content-Type": "application/json", Authorization: `Bearer ${key}` },
      body: JSON.stringify({ model: MODEL, state: input.state, questions }),
    });
  } catch (error) {
    return refuse(502, "upstream", `a TypeSafe não respondeu: ${error}`);
  }
  // A avaliação volta como veio, e o status também: 429, 529 e 5xx são o
  // sinal para o Rust tentar de novo.
  const headers: Record<string, string> = {};
  const retryAfter = upstream.headers.get("Retry-After");
  if (retryAfter) headers["Retry-After"] = retryAfter;
  return new Response(await upstream.text(), { status: upstream.status, headers: { "Content-Type": "application/json", ...headers } });
});
