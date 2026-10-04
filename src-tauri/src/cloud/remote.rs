//! O PostgREST do projeto, do tamanho do que o JayV usa: subir linhas por
//! upsert, marcar exclusão, baixar o que mudou depois de um cursor, e ler o
//! conteúdo global. Cada requisição leva o JWT do usuário, e é o RLS que
//! decide o que ele vê.

use super::{PROJECT_URL, PUBLISHABLE_KEY};
use crate::local::{global::LocaleRow, outbox::SyncTable};
use crate::policy::RemotePolicy;
use async_trait::async_trait;
use reqwest::{Method, Request};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Debug,Clone,PartialEq,thiserror::Error)]
pub enum RemoteError {
    /// Sem rede, DNS, tempo esgotado: a fila espera e tenta de novo.
    #[error("no connection to Supabase: {0}")]
    Offline(String),
    #[error("session expired")]
    Unauthorized,
    /// Chave repetida (23505) ou referência a uma linha que não existe (23503).
    #[error("conflict in Supabase ({code}): {detail}")]
    Conflict{code:String,detail:String},
    /// O servidor recusou e reenviar não resolve.
    #[error("Supabase refused ({status}): {detail}")]
    Rejected{status:u16,detail:String},
    #[error("Supabase answered {0}")]
    Server(u16),
}

#[async_trait]
pub trait Backend:Send+Sync {
    async fn push(&self,table:&SyncTable,rows:Vec<Value>)->Result<(),RemoteError>;
    async fn soft_delete(&self,table:&SyncTable,keys:&[Value],at:&str)->Result<(),RemoteError>;
    /// As linhas com `synced_at >= since`, em ordem de `synced_at`. `since`
    /// vazio é a tabela inteira.
    async fn pull(&self,table:&SyncTable,since:&str,limit:usize)->Result<Vec<Value>,RemoteError>;
    /// A política de LLM dos projetos de quem está logado
    /// (`rpc/my_project_policies`). `None`: este backend não fala dela.
    async fn project_policies(&self)->Result<Option<Vec<RemotePolicy>>,RemoteError> { Ok(None) }
    /// Os recursos do plano de quem está logado (`rpc/my_features`). `None`:
    /// este backend não fala deles.
    async fn features(&self)->Result<Option<crate::features::RemoteFeatures>,RemoteError> { Ok(None) }
}

pub struct Remote {
    http:reqwest::Client,
    base:String,
    key:String,
    token:Option<String>,
}

/// A chave de conflito no Supabase. As tabelas de LLM e as da conta são por
/// usuário — dois usuários têm cada um o seu agente `claude` e o seu nível —,
/// então o `user_id` entra na chave; nas outras o id já é único.
pub fn conflict_target(table:&SyncTable)->String {
    match table.name {
        "llm_agents"|"llm_models"|"account_settings"=>format!("user_id,{}",table.key.join(",")),
        _=>table.key.join(","),
    }
}

impl Remote {
    pub fn new(http:reqwest::Client,token:Option<String>)->Self { Self::with_base(http,PROJECT_URL,PUBLISHABLE_KEY,token) }

    pub fn with_base(http:reqwest::Client,base:&str,key:&str,token:Option<String>)->Self {
        Self{http,base:base.trim_end_matches('/').to_string(),key:key.to_string(),token}
    }

    fn request(&self,method:Method,path:&str)->reqwest::RequestBuilder {
        let bearer=self.token.as_deref().unwrap_or(&self.key);
        self.http.request(method,format!("{}/rest/v1/{path}",self.base)).header("apikey",&self.key).bearer_auth(bearer)
    }

    pub fn push_request(&self,table:&SyncTable,rows:&[Value])->reqwest::Result<Request> {
        self.request(Method::POST,table.name)
            .query(&[("on_conflict",conflict_target(table))])
            // `missing=default`: o `user_id` não vem no corpo e tem de nascer
            // de `auth.uid()`, não como NULL.
            .header("Prefer","resolution=merge-duplicates,missing=default,return=minimal")
            .json(rows).build()
    }

    /// Uma requisição por chave: o filtro de chave composta em lote é um `or`
    /// que o PostgREST aceita, mas que se escreve mal e se lê pior.
    pub fn soft_delete_request(&self,table:&SyncTable,key:&Value,at:&str)->reqwest::Result<Request> {
        let parts=key.as_array().cloned().unwrap_or_default();
        let filters:Vec<(String,String)>=table.key.iter().zip(parts.iter()).map(|(column,value)|((*column).to_string(),format!("eq.{}",plain(value)))).collect();
        self.request(Method::PATCH,table.name).query(&filters)
            .header("Prefer","return=minimal")
            .json(&json!({"row_deleted_at":at,"row_updated_at":at})).build()
    }

    pub fn pull_request(&self,table:&SyncTable,since:&str,limit:usize)->reqwest::Result<Request> {
        let mut query=vec![("select".to_string(),"*".to_string()),("order".to_string(),"synced_at.asc".to_string()),("limit".to_string(),limit.to_string())];
        if !since.is_empty() {query.push(("synced_at".into(),format!("gte.{since}")));}
        self.request(Method::GET,table.name).query(&query).build()
    }

    pub fn project_policies_request(&self)->reqwest::Result<Request> {
        self.request(Method::POST,"rpc/my_project_policies").json(&json!({})).build()
    }

    pub fn features_request(&self)->reqwest::Result<Request> {
        self.request(Method::POST,"rpc/my_features").json(&json!({})).build()
    }

    async fn send(&self,request:reqwest::Result<Request>)->Result<String,RemoteError> {
        let request=request.map_err(|error|RemoteError::Rejected{status:0,detail:error.to_string()})?;
        let response=self.http.execute(request).await.map_err(|error|RemoteError::Offline(error.to_string()))?;
        let status=response.status().as_u16();
        let body=response.text().await.map_err(|error|RemoteError::Offline(error.to_string()))?;
        if (200..300).contains(&status) {Ok(body)} else {Err(classify(status,&body))}
    }

    async fn get<T:for<'de> Deserialize<'de>>(&self,request:reqwest::Result<Request>)->Result<T,RemoteError> {
        let body=self.send(request).await?;
        serde_json::from_str(&body).map_err(|error|RemoteError::Rejected{status:200,detail:format!("resposta em formato inesperado: {error}")})
    }

    pub async fn locales(&self)->Result<Vec<LocaleRow>,RemoteError> {
        self.get(self.request(Method::GET,"locales").query(&[("select","id,name,rtl,position"),("order","position.asc")]).build()).await
    }

    /// Todas as traduções do idioma, em páginas. O PostgREST do Supabase
    /// devolve no máximo 1000 linhas por consulta (`max_rows`), e um idioma já
    /// passa disso: sem paginar, as chaves mais novas ficavam de fora e a tela
    /// caía no inglês embutido.
    pub async fn translations(&self,locale:&str)->Result<BTreeMap<String,Value>,RemoteError> {
        #[derive(Deserialize)] struct Row{key:String,value:Value}
        let rows:Vec<Row>=all_pages(|offset|async move {
            let offset=offset.to_string();
            let limit=PAGE.to_string();
            self.get(self.request(Method::GET,"translations")
                .query(&[("select","key,value"),("locale",&format!("eq.{locale}")),("order","key.asc"),("limit",&limit),("offset",&offset)]).build()).await
        }).await?;
        Ok(rows.into_iter().map(|row|(row.key,row.value)).collect())
    }

    pub async fn jev_parameters(&self)->Result<BTreeMap<String,Value>,RemoteError> {
        #[derive(Deserialize)] struct Row{key:String,value:Value}
        let rows:Vec<Row>=self.get(self.request(Method::GET,"jev_parameters").query(&[("select","key,value")]).build()).await?;
        Ok(rows.into_iter().map(|row|(row.key,row.value)).collect())
    }
}

/// O tamanho de cada página pedida ao PostgREST.
const PAGE:usize=1000;
/// Um teto para nunca girar para sempre se o servidor repetir páginas.
const MAX_PAGES:usize=100;

/// Pede página atrás de página até vir uma vazia. Não para numa página menor
/// que `PAGE`: o servidor pode ter um `max_rows` menor que o pedido, e a
/// página cheia dele seria confundida com a última.
pub(crate) async fn all_pages<T,F,Fut>(mut fetch:F)->Result<Vec<T>,RemoteError>
where F:FnMut(usize)->Fut, Fut:std::future::Future<Output=Result<Vec<T>,RemoteError>> {
    let mut rows=Vec::new();
    for _ in 0..MAX_PAGES {
        let page=fetch(rows.len()).await?;
        if page.is_empty() {break;}
        rows.extend(page);
    }
    Ok(rows)
}

#[async_trait]
impl Backend for Remote {
    async fn push(&self,table:&SyncTable,rows:Vec<Value>)->Result<(),RemoteError> {
        if rows.is_empty() {return Ok(());}
        self.send(self.push_request(table,&rows)).await.map(drop)
    }

    async fn soft_delete(&self,table:&SyncTable,keys:&[Value],at:&str)->Result<(),RemoteError> {
        for key in keys {self.send(self.soft_delete_request(table,key,at)).await?;}
        Ok(())
    }

    async fn pull(&self,table:&SyncTable,since:&str,limit:usize)->Result<Vec<Value>,RemoteError> {
        self.get(self.pull_request(table,since,limit)).await
    }

    async fn project_policies(&self)->Result<Option<Vec<RemotePolicy>>,RemoteError> {
        self.get(self.project_policies_request()).await.map(Some)
    }

    async fn features(&self)->Result<Option<crate::features::RemoteFeatures>,RemoteError> {
        self.get(self.features_request()).await.map(Some)
    }
}

/// O valor como vai no filtro `eq.`: texto sem aspas, número como número.
fn plain(value:&Value)->String {
    match value {Value::String(text)=>text.clone(),other=>other.to_string()}
}

/// O que o status e o corpo do PostgREST querem dizer para a fila.
pub fn classify(status:u16,body:&str)->RemoteError {
    #[derive(Deserialize,Default)] struct Problem{#[serde(default)] code:String,#[serde(default)] message:String,#[serde(default)] details:Option<String>}
    let problem:Problem=serde_json::from_str(body).unwrap_or_default();
    let detail=match (&problem.message,&problem.details) {
        (message,Some(details)) if !message.is_empty()=>format!("{message} — {details}"),
        (message,_) if !message.is_empty()=>message.clone(),
        _=>body.chars().take(300).collect(),
    };
    match status {
        401=>RemoteError::Unauthorized,
        409=>RemoteError::Conflict{code:problem.code,detail},
        500..=599=>RemoteError::Server(status),
        _=>RemoteError::Rejected{status,detail},
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local::outbox::table;

    fn remote()->Remote { Remote::with_base(reqwest::Client::new(),"https://exemplo.supabase.co/",PUBLISHABLE_KEY,Some("jwt".into())) }

    #[tokio::test] async fn pages_continue_past_a_server_cap_until_one_comes_back_empty() {
        // 2500 linhas e um servidor que corta em 1000: três páginas e a vazia.
        let all:Vec<usize>=(0..2500).collect();
        let mut asked=vec![];
        let rows=all_pages(|offset|{
            asked.push(offset);
            let page:Vec<usize>=all.iter().copied().skip(offset).take(1000).collect();
            async move {Ok::<_,RemoteError>(page)}
        }).await.unwrap();
        assert_eq!(rows.len(),2500);
        assert_eq!(asked,[0,1000,2000,2500]);
        let small=all_pages(|offset|{let page:Vec<usize>=all.iter().copied().skip(offset).take(300).collect(); async move {Ok::<_,RemoteError>(page)}}).await.unwrap();
        assert_eq!(small.len(),2500,"um max_rows menor que a página não para no meio");
    }

    fn query(request:&Request)->Vec<(String,String)> { request.url().query_pairs().map(|(key,value)|(key.into_owned(),value.into_owned())).collect() }

    #[test] fn the_upsert_uses_each_tables_key() {
        let request=remote().push_request(table("projects").unwrap(),&[json!({"id":"p1"})]).expect("request");
        assert_eq!(request.method(),Method::POST);
        assert_eq!(request.url().path(),"/rest/v1/projects");
        assert_eq!(query(&request),[("on_conflict".to_string(),"id".to_string())]);
        // Sem `missing=default` o PostgREST grava NULL no `user_id`, que não
        // vem no corpo: toda subida seria recusada pelo NOT NULL.
        assert_eq!(request.headers()["Prefer"],"resolution=merge-duplicates,missing=default,return=minimal");
        assert_eq!(request.headers()["Authorization"],"Bearer jwt");
        assert_eq!(request.headers()["apikey"],PUBLISHABLE_KEY);
        assert_eq!(query(&remote().push_request(table("messages").unwrap(),&[]).unwrap())[0].1,"uid");
        assert_eq!(query(&remote().push_request(table("llm_agents").unwrap(),&[]).unwrap())[0].1,"user_id,id");
        assert_eq!(query(&remote().push_request(table("llm_models").unwrap(),&[]).unwrap())[0].1,"user_id,agent,model");
    }

    #[test] fn without_session_the_token_is_the_publishable_key() {
        let anonymous=Remote::with_base(reqwest::Client::new(),"https://exemplo.supabase.co",PUBLISHABLE_KEY,None);
        let request=anonymous.pull_request(table("projects").unwrap(),"",10).unwrap();
        assert_eq!(request.headers()["Authorization"],format!("Bearer {PUBLISHABLE_KEY}"));
    }

    #[test] fn the_download_asks_for_what_changed_since_the_cursor() {
        let request=remote().pull_request(table("chats").unwrap(),"2026-09-30T12:00:00Z",1000).unwrap();
        assert_eq!(request.method(),Method::GET);
        let pairs=query(&request);
        for expected in [("select","*"),("order","synced_at.asc"),("limit","1000"),("synced_at","gte.2026-09-30T12:00:00Z")] {
            assert!(pairs.contains(&(expected.0.to_string(),expected.1.to_string())),"{pairs:?}");
        }
        assert!(!query(&remote().pull_request(table("chats").unwrap(),"",1000).unwrap()).iter().any(|(key,_)|key=="synced_at"),"sem cursor baixa tudo");
    }

    #[test] fn delete_marks_the_row_by_its_key() {
        let request=remote().soft_delete_request(table("llm_models").unwrap(),&json!(["claude","opus, \"novo\""]),"2026-09-30T12:00:00Z").unwrap();
        assert_eq!(request.method(),Method::PATCH);
        assert_eq!(query(&request),[("agent".to_string(),"eq.claude".to_string()),("model".to_string(),"eq.opus, \"novo\"".to_string())]);
        let body:Value=serde_json::from_slice(request.body().unwrap().as_bytes().unwrap()).unwrap();
        assert_eq!(body,json!({"row_deleted_at":"2026-09-30T12:00:00Z","row_updated_at":"2026-09-30T12:00:00Z"}));
    }

    #[test] fn each_status_becomes_the_error_the_queue_understands() {
        assert_eq!(classify(401,r#"{"code":"PGRST301","message":"JWT expired"}"#),RemoteError::Unauthorized);
        assert_eq!(classify(409,r#"{"code":"23505","message":"duplicate key","details":"Key (chat_id, ordinal)=(c, 2) already exists."}"#),
            RemoteError::Conflict{code:"23505".into(),detail:"duplicate key — Key (chat_id, ordinal)=(c, 2) already exists.".into()});
        assert!(matches!(classify(409,r#"{"code":"23503","message":"fk"}"#),RemoteError::Conflict{code,..} if code=="23503"));
        assert_eq!(classify(503,"down"),RemoteError::Server(503));
        assert_eq!(classify(400,r#"{"code":"PGRST204","message":"coluna desconhecida"}"#),RemoteError::Rejected{status:400,detail:"coluna desconhecida".into()});
    }

    #[test] fn the_policies_come_from_the_rpc_with_the_users_token() {
        let request=remote().project_policies_request().unwrap();
        assert_eq!((request.method(),request.url().path()),(&Method::POST,"/rest/v1/rpc/my_project_policies"));
        assert_eq!(request.headers()["Authorization"],"Bearer jwt");
    }

    #[test] fn the_plan_features_come_from_the_rpc_with_the_users_token() {
        let request=remote().features_request().unwrap();
        assert_eq!((request.method(),request.url().path()),(&Method::POST,"/rest/v1/rpc/my_features"));
        assert_eq!(request.headers()["Authorization"],"Bearer jwt");
    }

    #[tokio::test] async fn without_network_the_error_is_offline() {
        let unreachable=Remote::with_base(reqwest::Client::new(),"http://127.0.0.1:9",PUBLISHABLE_KEY,Some("jwt".into()));
        assert!(matches!(unreachable.pull(table("projects").unwrap(),"",1).await,Err(RemoteError::Offline(_))));
    }
}
