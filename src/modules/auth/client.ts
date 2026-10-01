import { createClient } from "@supabase/supabase-js";

/** O projeto e a chave publicável vão no build: só identificam o projeto. A
 * secret key nunca entra no app. */
export const SUPABASE_URL = "https://exvsozyemolrjbjetqww.supabase.co";
export const SUPABASE_KEY = "sb_publishable_7PeIEX0yh_svIEeNw3Gwew_lEbMVtMm";
/** Para onde o navegador volta depois do GitHub ou do e-mail de confirmação. */
export const CALLBACK_URL = "jayv://auth/callback";

/** PKCE porque o retorno chega por deep link, fora da janela: o código só vale
 * com o verificador que ficou guardado aqui. */
export const supabase = createClient(SUPABASE_URL, SUPABASE_KEY, {
  auth: { flowType: "pkce", persistSession: true, autoRefreshToken: true, detectSessionInUrl: false },
});
