//! Quem está usando o app. O React faz o login e entrega o token; aqui ele só
//! é aceito com assinatura ES256 de uma chave do JWKS do projeto, emissor e
//! público certos e dentro do prazo.

use super::{AUDIENCE, ISSUER, PROJECT_URL};
use anyhow::Context;
use jsonwebtoken::{decode, decode_header, jwk::JwkSet, Algorithm, DecodingKey, Validation};
use serde::Deserialize;

#[derive(Debug,Clone,PartialEq,Eq)]
pub struct Identity {
    pub user_id:String,
    pub email:Option<String>,
    pub expires_at:i64,
}

#[derive(Debug,thiserror::Error)]
pub enum SessionError {
    #[error("session expired")]
    Expired,
    /// O token foi assinado por uma chave que o JWKS guardado não tem: quem
    /// chama baixa o JWKS de novo antes de recusar.
    #[error("unknown signing key")]
    UnknownKey,
    #[error("invalid session: {0}")]
    Invalid(String),
}

/// O token da sessão validada agora, para quem fala com o Supabase em nome do
/// usuário — o Jev, por exemplo. O desktop o troca a cada `set_session`.
static CURRENT:std::sync::RwLock<Option<String>>=std::sync::RwLock::new(None);

pub fn set_current(token:Option<String>) { *CURRENT.write().unwrap_or_else(|poisoned|poisoned.into_inner())=token; }

pub fn current()->Option<String> { CURRENT.read().unwrap_or_else(|poisoned|poisoned.into_inner()).clone() }

#[derive(Deserialize)]
struct Claims { sub:String, exp:i64, #[serde(default)] email:Option<String> }

/// Confere o token contra o JWKS e o relógio `now` (segundos Unix).
pub fn validate(token:&str,keys:&JwkSet,now:i64)->Result<Identity,SessionError> {
    let identity=verify(token,keys)?;
    if identity.expires_at<=now {return Err(SessionError::Expired);}
    Ok(identity)
}

/// Sem rede, o token vencido do último usuário ainda abre o cache e a fila
/// dele: a assinatura continua conferida, só o prazo é perdoado. A
/// sincronização espera um token novo.
pub fn validate_offline(token:&str,keys:&JwkSet,last_user:Option<&str>,now:i64)->Result<Identity,SessionError> {
    match validate(token,keys,now) {
        Err(SessionError::Expired)=>{
            let identity=verify(token,keys)?;
            if last_user==Some(identity.user_id.as_str()) {Ok(identity)} else {Err(SessionError::Expired)}
        }
        other=>other,
    }
}

fn verify(token:&str,keys:&JwkSet)->Result<Identity,SessionError> {
    let header=decode_header(token).map_err(|error|SessionError::Invalid(error.to_string()))?;
    if header.alg!=Algorithm::ES256 {return Err(SessionError::Invalid(format!("algorithm {:?} is not accepted",header.alg)));}
    let kid=header.kid.ok_or_else(||SessionError::Invalid("token without kid".into()))?;
    let jwk=keys.find(&kid).ok_or(SessionError::UnknownKey)?;
    let key=DecodingKey::from_jwk(jwk).map_err(|error|SessionError::Invalid(error.to_string()))?;
    let mut validation=Validation::new(Algorithm::ES256);
    validation.set_issuer(&[ISSUER]);
    validation.set_audience(&[AUDIENCE]);
    // O prazo é conferido com o relógio de quem chama, para o modo sem rede
    // poder perdoá-lo.
    validation.validate_exp=false;
    validation.set_required_spec_claims(&["exp","sub","iss","aud"]);
    let claims=decode::<Claims>(token,&key,&validation).map_err(|error|SessionError::Invalid(error.to_string()))?.claims;
    Ok(Identity{user_id:claims.sub,email:claims.email,expires_at:claims.exp})
}

pub async fn fetch_jwks(http:&reqwest::Client)->anyhow::Result<JwkSet> {
    let url=format!("{PROJECT_URL}/auth/v1/.well-known/jwks.json");
    let response=http.get(&url).send().await.with_context(||format!("could not download {url}"))?.error_for_status()?;
    response.json().await.context("the project JWKS has an unexpected format")
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{encode, EncodingKey, Header};
    use serde_json::json;

    const NOW:i64=1_900_000_000;
    const USER:&str="6f1c1f9e-5b1a-4a59-9a39-1c0b6b8f9f10";

    fn keys()->JwkSet {serde_json::from_str(include_str!("testdata/jwks.json")).expect("jwks")}

    fn token(pem:&str,kid:&str,claims:serde_json::Value)->String {
        let mut header=Header::new(Algorithm::ES256);
        header.kid=Some(kid.into());
        encode(&header,&claims,&EncodingKey::from_ec_pem(pem.as_bytes()).expect("chave")).expect("token")
    }

    fn claims(exp:i64)->serde_json::Value {json!({"sub":USER,"email":"dev@teste.local","exp":exp,"iss":ISSUER,"aud":AUDIENCE,"role":"authenticated"})}

    const KEY_A:&str=include_str!("testdata/es256_a.pem");
    const KEY_B:&str=include_str!("testdata/es256_b.pem");

    /// O JWKS como o projeto o publica, com `ext` e `key_ops`: ler e achar a
    /// chave pelo `kid` é o que a primeira abertura do app faz.
    #[test] fn the_project_jwks_is_read() {
        let keys:JwkSet=serde_json::from_str(include_str!("testdata/project_jwks.json")).expect("o JWKS publicado tem de ser legível");
        let jwk=keys.find("0a44491d-b641-4447-973b-32c13254f83c").expect("a chave do projeto");
        DecodingKey::from_jwk(jwk).expect("a chave vira chave de verificação");
    }

    #[test] fn a_valid_token_returns_the_user() {
        let identity=validate(&token(KEY_A,"test-key",claims(NOW+60)),&keys(),NOW).expect("válido");
        assert_eq!(identity,Identity{user_id:USER.into(),email:Some("dev@teste.local".into()),expires_at:NOW+60});
    }

    #[test] fn an_expired_token_is_refused() {
        let error=validate(&token(KEY_A,"test-key",claims(NOW-1)),&keys(),NOW).unwrap_err();
        assert!(matches!(error,SessionError::Expired));
        assert_eq!(error.to_string(),"session expired");
    }

    #[test] fn another_issuer_is_refused() {
        let mut other=claims(NOW+60);
        other["iss"]=json!("https://outro.supabase.co/auth/v1");
        assert!(matches!(validate(&token(KEY_A,"test-key",other),&keys(),NOW),Err(SessionError::Invalid(_))));
    }

    #[test] fn another_audience_is_refused() {
        let mut other=claims(NOW+60);
        other["aud"]=json!("anon");
        assert!(matches!(validate(&token(KEY_A,"test-key",other),&keys(),NOW),Err(SessionError::Invalid(_))));
    }

    #[test] fn a_signature_from_another_key_is_refused() {
        assert!(matches!(validate(&token(KEY_B,"test-key",claims(NOW+60)),&keys(),NOW),Err(SessionError::Invalid(_))));
    }

    #[test] fn an_unknown_kid_fetches_the_jwks_again() {
        assert!(matches!(validate(&token(KEY_A,"rotacionada",claims(NOW+60)),&keys(),NOW),Err(SessionError::UnknownKey)));
    }

    #[test] fn offline_the_last_user_keeps_going_with_an_expired_token() {
        let expired=token(KEY_A,"test-key",claims(NOW-3_600));
        assert_eq!(validate_offline(&expired,&keys(),Some(USER),NOW).expect("último usuário").user_id,USER);
        assert!(matches!(validate_offline(&expired,&keys(),Some("outro"),NOW),Err(SessionError::Expired)));
        assert!(matches!(validate_offline(&expired,&keys(),None,NOW),Err(SessionError::Expired)));
        assert!(matches!(validate_offline(&token(KEY_B,"test-key",claims(NOW-3_600)),&keys(),Some(USER),NOW),Err(SessionError::Invalid(_))),"a assinatura não é perdoada");
    }
}
