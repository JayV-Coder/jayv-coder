use crate::model::ChatMessage;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use std::{fs, path::{Path, PathBuf}, time::Duration};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all="camelCase")]
pub struct ProjectRecord {
    pub id: String,
    pub name: String,
    pub root_path: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all="camelCase")]
pub struct WorkspaceMessage {
    pub role: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all="camelCase")]
pub struct ChatRecord {
    pub id: String,
    /// O identificador curto que o desenvolvedor lê e cita: é ele que a
    /// Portaria mostra no botão de volta para o chat.
    pub code: String,
    pub project_id: String,
    pub title: String,
    pub messages: Vec<WorkspaceMessage>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all="camelCase")]
pub struct WorkspaceData {
    pub projects: Vec<ProjectRecord>,
    pub chats: Vec<ChatRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TableCount { pub name: String, pub rows: i64 }

pub struct WorkspaceStore {
    connection: Connection,
    path: PathBuf,
}

impl WorkspaceStore {
    pub fn open(path: PathBuf, legacy_json: Option<&Path>) -> Result<Self> {
        if let Some(parent)=path.parent(){fs::create_dir_all(parent).with_context(||format!("could not create {}",parent.display()))?;}
        let connection=Connection::open(&path).with_context(||format!("could not open {}",path.display()))?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS projects (
               id TEXT PRIMARY KEY,
               name TEXT NOT NULL,
               root_path TEXT NOT NULL DEFAULT '',
               created_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS chats (
               id TEXT PRIMARY KEY,
               code TEXT NOT NULL DEFAULT '',
               project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
               title TEXT NOT NULL,
               created_at TEXT NOT NULL,
               updated_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS messages (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               chat_id TEXT NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
               role TEXT NOT NULL,
               content TEXT NOT NULL,
               created_at TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS chats_project_updated ON chats(project_id, updated_at DESC);
             CREATE INDEX IF NOT EXISTS messages_chat_order ON messages(chat_id, id);
             CREATE TABLE IF NOT EXISTS app_metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL);"
        )?;
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path,fs::Permissions::from_mode(0o600))?;
        }
        let mut store=Self{connection,path};
        store.ensure_chat_codes()?;
        if let Some(legacy)=legacy_json {store.migrate_legacy_json(legacy)?;}
        store.ensure_chat_codes()?;
        Ok(store)
    }

    pub fn snapshot(&self) -> Result<WorkspaceData> {
        let project_rows={
            let mut statement=self.connection.prepare("SELECT id,name,root_path,created_at FROM projects ORDER BY created_at,id")?;
            statement.query_map([],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?)))?.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let projects=project_rows.into_iter().map(|(id,name,root_path,created_at)|Ok(ProjectRecord{id,name,root_path,created_at:parse_time(&created_at)?})).collect::<Result<Vec<_>>>()?;
        let chat_rows={
            let mut statement=self.connection.prepare("SELECT id,code,project_id,title,created_at,updated_at FROM chats ORDER BY updated_at DESC,id")?;
            statement.query_map([],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?,row.get::<_,String>(4)?,row.get::<_,String>(5)?)))?.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let mut chats=Vec::with_capacity(chat_rows.len());
        for (id,code,project_id,title,created_at,updated_at) in chat_rows {
            chats.push(ChatRecord{id:id.clone(),code,project_id,title,messages:self.messages(&id)?,created_at:parse_time(&created_at)?,updated_at:parse_time(&updated_at)?});
        }
        Ok(WorkspaceData{projects,chats})
    }

    pub fn create_project(&mut self, name: &str, root_path: Option<String>) -> Result<ProjectRecord> {
        let name=name.trim();
        anyhow::ensure!(!name.is_empty(),"o nome do projeto é obrigatório");
        let project=ProjectRecord{id:Uuid::new_v4().to_string(),name:name.into(),root_path:root_path.unwrap_or_default(),created_at:Utc::now()};
        self.connection.execute("INSERT INTO projects(id,name,root_path,created_at) VALUES(?1,?2,?3,?4)",params![project.id,project.name,project.root_path,project.created_at.to_rfc3339()])?;
        Ok(project)
    }

    pub fn create_chat(&mut self, project_id: &str, title: Option<String>) -> Result<ChatRecord> {
        anyhow::ensure!(self.project_exists(project_id)?,"projeto não encontrado");
        let now=Utc::now();
        let title=title.unwrap_or_default().trim().to_string();
        let chat=ChatRecord{id:Uuid::new_v4().to_string(),code:self.unused_chat_code()?,project_id:project_id.into(),title:if title.is_empty(){"Novo chat".into()}else{title},messages:vec![],created_at:now,updated_at:now};
        self.connection.execute("INSERT INTO chats(id,code,project_id,title,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6)",params![chat.id,chat.code,chat.project_id,chat.title,chat.created_at.to_rfc3339(),chat.updated_at.to_rfc3339()])?;
        Ok(chat)
    }

    /// Grava o pedido antes de qualquer ida ao modelo: se a rota falhar, o
    /// desenvolvedor sair do chat ou o aplicativo fechar no meio, o que ele
    /// escreveu continua no banco, sem resposta, e a interface oferece o
    /// reenvio. Devolve `false` quando o pedido pendente já era exatamente
    /// este — é a retentativa do mesmo texto, que não pode virar duas linhas
    /// iguais no histórico.
    pub fn append_prompt(&mut self, chat_id: &str, user: &str) -> Result<bool> {
        let transaction=self.connection.transaction()?;
        let (title,count):(String,i64)=transaction.query_row("SELECT title,(SELECT COUNT(*) FROM messages WHERE chat_id=?1) FROM chats WHERE id=?1",[chat_id],|row|Ok((row.get(0)?,row.get(1)?))).context("chat não encontrado")?;
        let last:Option<(String,String)>=transaction.query_row("SELECT role,content FROM messages WHERE chat_id=?1 ORDER BY id DESC LIMIT 1",[chat_id],|row|Ok((row.get(0)?,row.get(1)?))).optional()?;
        if last.is_some_and(|(role,content)|role=="user" && content==user) {return Ok(false);}
        let now=Utc::now();
        if count==0 && title=="Novo chat" {transaction.execute("UPDATE chats SET title=?1 WHERE id=?2",params![compact_title(user),chat_id])?;}
        insert_message(&transaction,chat_id,"user",user,now)?;
        transaction.execute("UPDATE chats SET updated_at=?1 WHERE id=?2",params![now.to_rfc3339(),chat_id])?;
        transaction.commit()?;
        Ok(true)
    }

    /// Fecha a troca com o que voltou do modelo — ou com o erro que veio no
    /// lugar dele. Enquanto isto não acontece, o pedido fica pendente.
    pub fn append_answer(&mut self, chat_id: &str, assistant: &str) -> Result<()> {
        anyhow::ensure!(self.contains_chat(chat_id)?,"chat não encontrado");
        let now=Utc::now();
        let transaction=self.connection.transaction()?;
        insert_message(&transaction,chat_id,"assistant",assistant,now)?;
        transaction.execute("UPDATE chats SET updated_at=?1 WHERE id=?2",params![now.to_rfc3339(),chat_id])?;
        transaction.commit()?;
        Ok(())
    }

    pub fn append_exchange(&mut self, chat_id: &str, user: &str, assistant: &str) -> Result<()> {
        self.append_prompt(chat_id,user)?;
        self.append_answer(chat_id,assistant)
    }

    /// Um chat que ainda não recebeu nada: é o único momento em que vale gastar
    /// uma chamada de modelo para batizá-lo.
    pub fn chat_is_unnamed(&self, chat_id: &str) -> Result<bool> {
        let (title,count):(String,i64)=self.connection.query_row("SELECT title,(SELECT COUNT(*) FROM messages WHERE chat_id=?1) FROM chats WHERE id=?1",[chat_id],|row|Ok((row.get(0)?,row.get(1)?))).context("chat não encontrado")?;
        Ok(count==0 && title=="Novo chat")
    }

    /// Troca o título sem mexer no `updated_at`: rebatizar não é movimento de
    /// conversa e não pode reordenar a lista lateral.
    pub fn rename_chat(&mut self, chat_id: &str, title: &str) -> Result<()> {
        let title=compact_title(title);
        anyhow::ensure!(self.connection.execute("UPDATE chats SET title=?1 WHERE id=?2",params![title,chat_id])?>0,"chat não encontrado");
        Ok(())
    }

    pub fn clear_chat(&mut self, chat_id: &str) -> Result<()> {
        anyhow::ensure!(self.contains_chat(chat_id)?,"chat não encontrado");
        let transaction=self.connection.transaction()?;
        transaction.execute("DELETE FROM messages WHERE chat_id=?1",[chat_id])?;
        transaction.execute("UPDATE chats SET title='Novo chat',updated_at=?1 WHERE id=?2",params![Utc::now().to_rfc3339(),chat_id])?;
        transaction.commit()?;
        Ok(())
    }

    pub fn delete_chat(&mut self, chat_id: &str) -> Result<()> {
        anyhow::ensure!(self.connection.execute("DELETE FROM chats WHERE id=?1",[chat_id])?>0,"chat não encontrado");
        Ok(())
    }

    pub fn delete_project(&mut self, project_id: &str) -> Result<Vec<String>> {
        let chat_ids=self.chat_ids_for_project(project_id)?;
        anyhow::ensure!(self.connection.execute("DELETE FROM projects WHERE id=?1",[project_id])?>0,"projeto não encontrado");
        Ok(chat_ids)
    }

    pub fn conversation(&self, chat_id: &str) -> Result<Vec<ChatMessage>> {
        anyhow::ensure!(self.contains_chat(chat_id)?,"chat não encontrado");
        Ok(self.messages(chat_id)?.into_iter().map(|message|ChatMessage{role:message.role,content:message.content}).collect())
    }

    pub fn contains_chat(&self, chat_id: &str) -> Result<bool> {
        Ok(self.connection.query_row("SELECT 1 FROM chats WHERE id=?1",[chat_id],|row|row.get::<_,i64>(0)).optional()?.is_some())
    }

    /// A pasta do projeto onde o chat foi criado, quando o projeto tem uma. É
    /// esse caminho que vai para o índice e para o prompt; projeto sem pasta
    /// devolve nada e quem chama decide o que usar no lugar.
    pub fn chat_root(&self, chat_id: &str) -> Result<Option<PathBuf>> {
        let root:String=self.connection.query_row("SELECT projects.root_path FROM chats JOIN projects ON projects.id=chats.project_id WHERE chats.id=?1",[chat_id],|row|row.get(0)).context("chat não encontrado")?;
        let root=root.trim();
        Ok((!root.is_empty()).then(||PathBuf::from(root)))
    }

    pub fn database_path(&self) -> &Path {&self.path}

    /// O nome do arquivo do banco, sem o caminho até ele.
    pub fn database_name(&self) -> String { self.path.file_name().map(|name|name.to_string_lossy().into_owned()).unwrap_or_else(||self.path.display().to_string()) }

    /// Cada tabela do banco com quantas linhas ela guarda, em ordem alfabética.
    pub fn table_counts(&self) -> Result<Vec<TableCount>> {
        let names={
            let mut statement=self.connection.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")?;
            statement.query_map([],|row|row.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?
        };
        names.into_iter().map(|name|{
            let rows=self.connection.query_row(&format!("SELECT COUNT(*) FROM \"{}\"",name.replace('"',"\"\"")),[],|row|row.get::<_,i64>(0))?;
            Ok(TableCount{name,rows})
        }).collect()
    }

    /// Garante que todo chat tem um código: cria a coluna nos bancos antigos,
    /// preenche quem ficou sem e só então exige que ela seja única.
    fn ensure_chat_codes(&mut self)->Result<()> {
        if !self.connection.prepare("SELECT 1 FROM pragma_table_info('chats') WHERE name='code'")?.exists([])? {
            self.connection.execute_batch("ALTER TABLE chats ADD COLUMN code TEXT NOT NULL DEFAULT ''")?;
        }
        let pending={
            let mut statement=self.connection.prepare("SELECT id FROM chats WHERE code=''")?;
            statement.query_map([],|row|row.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?
        };
        for id in pending {
            let code=self.unused_chat_code()?;
            self.connection.execute("UPDATE chats SET code=?1 WHERE id=?2",params![code,id])?;
        }
        // Índice parcial: a importação do JSON antigo insere com o código vazio
        // e só depois é preenchida, então o vazio não pode disputar unicidade.
        self.connection.execute_batch("CREATE UNIQUE INDEX IF NOT EXISTS chats_code ON chats(code) WHERE code<>''")?;
        Ok(())
    }

    fn unused_chat_code(&self)->Result<String> {
        for _ in 0..32 {
            let code=new_chat_code();
            if self.connection.query_row("SELECT 1 FROM chats WHERE code=?1",[&code],|row|row.get::<_,i64>(0)).optional()?.is_none(){return Ok(code);}
        }
        anyhow::bail!("não foi possível gerar um identificador livre para o chat")
    }

    fn project_exists(&self, project_id:&str)->Result<bool>{Ok(self.connection.query_row("SELECT 1 FROM projects WHERE id=?1",[project_id],|row|row.get::<_,i64>(0)).optional()?.is_some())}

    /// Os chats de um projeto — a Portaria usa isto para mostrar só o que
    /// passou pelos portões deste projeto.
    pub fn chat_ids_for_project(&self,project_id:&str)->Result<Vec<String>>{
        let mut statement=self.connection.prepare("SELECT id FROM chats WHERE project_id=?1")?;
        Ok(statement.query_map([project_id],|row|row.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    fn messages(&self,chat_id:&str)->Result<Vec<WorkspaceMessage>>{
        let rows={
            let mut statement=self.connection.prepare("SELECT role,content,created_at FROM messages WHERE chat_id=?1 ORDER BY id")?;
            statement.query_map([chat_id],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?)))?.collect::<rusqlite::Result<Vec<_>>>()?
        };
        rows.into_iter().map(|(role,content,created_at)|Ok(WorkspaceMessage{role,content,created_at:parse_time(&created_at)?})).collect()
    }

    fn migrate_legacy_json(&mut self,path:&Path)->Result<()> {
        let migrated=self.connection.query_row("SELECT value FROM app_metadata WHERE key='legacy_json_migrated'",[],|row|row.get::<_,String>(0)).optional()?.is_some();
        if migrated || !path.is_file(){return Ok(());}
        let raw=fs::read_to_string(path).with_context(||format!("could not read {}",path.display()))?;
        let legacy:WorkspaceData=serde_json::from_str(&raw).with_context(||format!("invalid legacy workspace in {}",path.display()))?;
        let skip_generated=legacy.projects.len()==1 && legacy.projects[0].name=="src-tauri" && legacy.chats.iter().all(|chat|chat.messages.is_empty());
        let transaction=self.connection.transaction()?;
        if !skip_generated {import_legacy(&transaction,&legacy)?;}
        transaction.execute("INSERT OR REPLACE INTO app_metadata(key,value) VALUES('legacy_json_migrated','1')",[])?;
        transaction.commit()?;
        Ok(())
    }
}

fn import_legacy(transaction:&Transaction<'_>,legacy:&WorkspaceData)->Result<()> {
    for project in &legacy.projects {transaction.execute("INSERT OR IGNORE INTO projects(id,name,root_path,created_at) VALUES(?1,?2,?3,?4)",params![project.id,project.name,project.root_path,project.created_at.to_rfc3339()])?;}
    for chat in &legacy.chats {
        transaction.execute("INSERT OR IGNORE INTO chats(id,code,project_id,title,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6)",params![chat.id,"",chat.project_id,chat.title,chat.created_at.to_rfc3339(),chat.updated_at.to_rfc3339()])?;
        let existing:i64=transaction.query_row("SELECT COUNT(*) FROM messages WHERE chat_id=?1",[&chat.id],|row|row.get(0))?;
        if existing==0 {for message in &chat.messages {insert_message(transaction,&chat.id,&message.role,&message.content,message.created_at)?;}}
    }
    Ok(())
}

fn insert_message(transaction:&Transaction<'_>,chat_id:&str,role:&str,content:&str,created_at:DateTime<Utc>)->Result<()> {
    transaction.execute("INSERT INTO messages(chat_id,role,content,created_at) VALUES(?1,?2,?3,?4)",params![chat_id,role,content,created_at.to_rfc3339()])?;
    Ok(())
}

/// Seis caracteres do alfabeto de Crockford — sem I, L, O e U, que se
/// confundem com 1, 0 e V quando alguém lê o código em voz alta.
fn new_chat_code()->String {
    const ALPHABET:&[u8;32]=b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    Uuid::new_v4().as_bytes().iter().take(6).map(|byte|ALPHABET[(byte%32) as usize] as char).collect()
}

fn parse_time(value:&str)->Result<DateTime<Utc>>{Ok(DateTime::parse_from_rfc3339(value).with_context(||format!("invalid timestamp `{value}`"))?.with_timezone(&Utc))}

fn compact_title(input: &str) -> String {
    let mut title=input.split_whitespace().take(7).collect::<Vec<_>>().join(" ");
    if input.split_whitespace().count()>7 {title.push('…');}
    if title.is_empty(){"Novo chat".into()}else{title}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(root:&tempfile::TempDir)->WorkspaceStore{WorkspaceStore::open(root.path().join("workspace.sqlite3"),None).expect("workspace")}

    #[test]
    fn starts_without_an_automatic_project() {
        let root=tempfile::tempdir().expect("root");
        let store=store(&root);
        let data=store.snapshot().expect("snapshot");
        assert!(data.projects.is_empty());
        assert!(data.chats.is_empty());
    }

    #[test]
    fn every_chat_gets_its_own_short_code() {
        let root=tempfile::tempdir().expect("root");
        let mut store=store(&root);
        let project=store.create_project("Produto",None).expect("project");
        let codes=(0..40).map(|_|store.create_chat(&project.id,None).expect("chat").code).collect::<Vec<_>>();

        assert!(codes.iter().all(|code|code.len()==6),"o código tem seis caracteres");
        assert!(codes.iter().all(|code|code.chars().all(|letter|"0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(letter))),"sem I, L, O nem U");
        assert_eq!(codes.iter().collect::<std::collections::BTreeSet<_>>().len(),codes.len(),"dois chats nunca compartilham o código");
        let saved=store.snapshot().expect("snapshot");
        assert_eq!(saved.chats.iter().map(|chat|chat.code.clone()).collect::<std::collections::BTreeSet<_>>().len(),codes.len(),"o código sobrevive ao banco");
    }

    #[test]
    fn an_older_database_without_codes_is_filled_in_on_open() {
        let root=tempfile::tempdir().expect("root");
        let path=root.path().join("workspace.sqlite3");
        {
            let connection=rusqlite::Connection::open(&path).expect("sqlite");
            connection.execute_batch(
                "CREATE TABLE projects (id TEXT PRIMARY KEY,name TEXT NOT NULL,root_path TEXT NOT NULL DEFAULT '',created_at TEXT NOT NULL);
                 CREATE TABLE chats (id TEXT PRIMARY KEY,project_id TEXT NOT NULL,title TEXT NOT NULL,created_at TEXT NOT NULL,updated_at TEXT NOT NULL);
                 INSERT INTO projects VALUES('p','Antigo','',      '2024-01-01T00:00:00Z');
                 INSERT INTO chats VALUES('c1','p','Um','2024-01-01T00:00:00Z','2024-01-01T00:00:00Z');
                 INSERT INTO chats VALUES('c2','p','Dois','2024-01-02T00:00:00Z','2024-01-02T00:00:00Z');"
            ).expect("esquema antigo");
        }
        let store=WorkspaceStore::open(path,None).expect("workspace");
        let chats=store.snapshot().expect("snapshot").chats;
        assert_eq!(chats.len(),2);
        assert!(chats.iter().all(|chat|chat.code.len()==6),"quem já existia também ganha um código");
        assert_ne!(chats[0].code,chats[1].code);
    }

    #[test]
    fn a_project_knows_which_chats_are_its_own() {
        let root=tempfile::tempdir().expect("root");
        let mut store=store(&root);
        let mine=store.create_project("Meu",None).expect("project");
        let other=store.create_project("Outro",None).expect("project");
        let first=store.create_chat(&mine.id,None).expect("chat");
        let second=store.create_chat(&mine.id,None).expect("chat");
        store.create_chat(&other.id,None).expect("chat");

        let ids=store.chat_ids_for_project(&mine.id).expect("ids");
        assert_eq!(ids.iter().collect::<std::collections::BTreeSet<_>>(),[&first.id,&second.id].into_iter().collect());
        assert!(store.chat_ids_for_project("inexistente").expect("ids").is_empty());
    }

    #[test]
    fn counts_every_table_of_the_database_by_name() {
        let root=tempfile::tempdir().expect("root");
        let path=root.path().join("workspace.sqlite3");
        let mut store=WorkspaceStore::open(path,None).expect("workspace");
        let project=store.create_project("Produto",None).expect("project");
        let chat=store.create_chat(&project.id,None).expect("chat");
        store.append_exchange(&chat.id,"pergunta","resposta").expect("exchange");

        assert_eq!(store.database_name(),"workspace.sqlite3");
        let counts=store.table_counts().expect("counts");
        let named=|name:&str|counts.iter().find(|table|table.name==name).unwrap_or_else(||panic!("{name} deveria estar no banco")).rows;
        assert_eq!(named("projects"),1);
        assert_eq!(named("chats"),1);
        assert_eq!(named("messages"),2);
        assert!(counts.iter().all(|table|!table.name.starts_with("sqlite_")),"as tabelas internas do SQLite não interessam");
        assert!(counts.windows(2).all(|pair|pair[0].name<=pair[1].name),"a lista sai em ordem alfabética");
    }

    #[test]
    fn persists_projects_chats_and_messages_in_sqlite() {
        let root=tempfile::tempdir().expect("root");
        let path=root.path().join("workspace.sqlite3");
        let chat_id={
            let mut store=WorkspaceStore::open(path.clone(),None).expect("workspace");
            let project=store.create_project("Produto",None).expect("project");
            let chat=store.create_chat(&project.id,None).expect("chat");
            store.append_exchange(&chat.id,"Implemente o painel agora","Pronto").expect("exchange");
            chat.id
        };
        let reloaded=WorkspaceStore::open(path,None).expect("reloaded");
        let data=reloaded.snapshot().expect("snapshot");
        let persisted=data.chats.iter().find(|item|item.id==chat_id).expect("persisted chat");
        assert_eq!(persisted.title,"Implemente o painel agora");
        assert_eq!(persisted.messages.len(),2);
    }

    #[test]
    fn project_delete_cascades_to_chats_and_messages() {
        let root=tempfile::tempdir().expect("root");
        let mut store=store(&root);
        let project=store.create_project("Produto",None).expect("project");
        let chat=store.create_chat(&project.id,None).expect("chat");
        store.append_exchange(&chat.id,"Pergunta","Resposta").expect("exchange");
        assert_eq!(store.delete_project(&project.id).expect("delete"),vec![chat.id]);
        let data=store.snapshot().expect("snapshot");
        assert!(data.projects.is_empty());
        assert!(data.chats.is_empty());
    }

    #[test]
    fn deletes_one_chat_without_removing_its_project() {
        let root=tempfile::tempdir().expect("root");
        let mut store=store(&root);
        let project=store.create_project("Produto",None).expect("project");
        let first=store.create_chat(&project.id,Some("Primeiro".into())).expect("first chat");
        let second=store.create_chat(&project.id,Some("Segundo".into())).expect("second chat");
        store.delete_chat(&first.id).expect("delete chat");
        let data=store.snapshot().expect("snapshot");
        assert_eq!(data.projects.len(),1);
        assert_eq!(data.chats.len(),1);
        assert_eq!(data.chats[0].id,second.id);
    }

    #[test]
    fn ignores_the_generated_empty_src_tauri_legacy_project() {
        let root=tempfile::tempdir().expect("root");
        let legacy_path=root.path().join("workspace.json");
        let now=Utc::now();
        let legacy=WorkspaceData{projects:vec![ProjectRecord{id:"project".into(),name:"src-tauri".into(),root_path:"/tmp/src-tauri".into(),created_at:now}],chats:vec![ChatRecord{id:"chat".into(),code:String::new(),project_id:"project".into(),title:"Novo chat".into(),messages:vec![],created_at:now,updated_at:now}]};
        fs::write(&legacy_path,serde_json::to_vec(&legacy).expect("json")).expect("legacy fixture");
        let store=WorkspaceStore::open(root.path().join("workspace.sqlite3"),Some(&legacy_path)).expect("workspace");
        assert!(store.snapshot().expect("snapshot").projects.is_empty());
    }

    /// O chat é a porta do projeto: quem manda uma mensagem nele tem de ser
    /// lido dentro da pasta que o projeto aponta, nunca na de onde o
    /// aplicativo subiu.
    #[test]
    fn a_chat_answers_with_the_folder_of_its_own_project() {
        let root=tempfile::tempdir().expect("root");
        let mut store=store(&root);
        let with_folder=store.create_project("Meu Emissor",Some("/home/isaach/Arquivos/Meu Emissor".into())).expect("project");
        let without_folder=store.create_project("Rascunho",None).expect("project");
        let chat=store.create_chat(&with_folder.id,None).expect("chat");
        let loose=store.create_chat(&without_folder.id,None).expect("chat");

        assert_eq!(store.chat_root(&chat.id).expect("raiz").expect("pasta do projeto"),PathBuf::from("/home/isaach/Arquivos/Meu Emissor"));
        assert!(store.chat_root(&loose.id).expect("raiz").is_none(),"projeto sem pasta não inventa caminho");
        assert!(store.chat_root("outro").is_err(),"chat que não existe não tem raiz");
    }

    #[test]
    fn only_a_chat_without_history_is_worth_a_model_call() {
        let root=tempfile::tempdir().expect("root");
        let mut store=store(&root);
        let project=store.create_project("Produto",None).expect("project");
        let chat=store.create_chat(&project.id,None).expect("chat");

        assert!(store.chat_is_unnamed(&chat.id).expect("chat novo"),"um chat recém-criado ainda não tem nome");
        store.append_exchange(&chat.id,"Preciso revisar o cálculo do frete no checkout","Vamos olhar o cálculo.").expect("exchange");
        assert!(!store.chat_is_unnamed(&chat.id).expect("chat usado"),"depois da primeira troca o título já foi escolhido uma vez");
        assert_eq!(store.snapshot().expect("snapshot").chats[0].title,"Preciso revisar o cálculo do frete no…","o resumo local entra na hora");
    }

    #[test]
    fn renaming_a_chat_never_reorders_the_sidebar() {
        let root=tempfile::tempdir().expect("root");
        let mut store=store(&root);
        let project=store.create_project("Produto",None).expect("project");
        let older=store.create_chat(&project.id,None).expect("older");
        let newer=store.create_chat(&project.id,None).expect("newer");
        let moment=store.snapshot().expect("snapshot").chats.iter().find(|chat|chat.id==older.id).expect("older").updated_at;

        store.rename_chat(&older.id,"Cálculo do frete no checkout").expect("rename");
        let saved=store.snapshot().expect("snapshot");
        let renamed=saved.chats.iter().find(|chat|chat.id==older.id).expect("older");
        assert_eq!(renamed.title,"Cálculo do frete no checkout");
        assert_eq!(renamed.updated_at,moment,"rebatizar não é movimento de conversa");
        assert!(store.rename_chat("chat-que-não-existe","Título").is_err());
        assert_eq!(saved.chats[0].id,newer.id,"o chat mais recente continua no topo");
    }

    #[test]
    fn imports_real_legacy_history_once() {
        let root=tempfile::tempdir().expect("root");
        let legacy_path=root.path().join("workspace.json");
        let now=Utc::now();
        let legacy=WorkspaceData{projects:vec![ProjectRecord{id:"project".into(),name:"Produto".into(),root_path:"".into(),created_at:now}],chats:vec![ChatRecord{id:"chat".into(),code:String::new(),project_id:"project".into(),title:"Discussão".into(),messages:vec![WorkspaceMessage{role:"user".into(),content:"Olá".into(),created_at:now}],created_at:now,updated_at:now}]};
        fs::write(&legacy_path,serde_json::to_vec(&legacy).expect("json")).expect("legacy fixture");
        let database_path=root.path().join("workspace.sqlite3");
        let first=WorkspaceStore::open(database_path.clone(),Some(&legacy_path)).expect("first migration");
        assert_eq!(first.snapshot().expect("first snapshot").chats[0].messages.len(),1);
        drop(first);
        let second=WorkspaceStore::open(database_path,Some(&legacy_path)).expect("second open");
        assert_eq!(second.snapshot().expect("second snapshot").chats[0].messages.len(),1);
    }
}
