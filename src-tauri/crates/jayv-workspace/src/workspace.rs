use crate::{gatekeeper::{EntryCheck, ExitCheck, GateFeed}, i18n::Text, model::ChatMessage, turns::{self, QuestionView, Turn, TurnStatus, TurnView}};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fs, path::{Path, PathBuf}, time::Duration};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all="camelCase")]
pub struct ProjectRecord {
    pub id: String,
    pub name: String,
    pub root_path: String,
    pub created_at: DateTime<Utc>,
    /// As chaves dos remotes da pasta (`github.com/acme/api`): é por elas que a
    /// tela sabe quais repositórios da organização já estão neste computador.
    #[serde(default)]
    pub repo_keys: Vec<String>,
    /// A organização de que este projeto é o chat: a pasta dele junta os
    /// repositórios dela. Nulo nos projetos de um repositório só.
    #[serde(default)]
    pub org_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all="camelCase")]
pub struct WorkspaceMessage {
    pub role: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
    /// O turno a que esta linha pertence. Histórico importado de antes dos
    /// turnos não tem nenhum, e continua legível sem semáforo.
    #[serde(default)]
    pub turn_id: Option<String>,
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
    /// O que cada pedido do chat levou dos dois portões: é daqui que o balão
    /// tira o código, o semáforo e o botão de retentar.
    #[serde(default)]
    pub turns: Vec<TurnView>,
    /// A pergunta que espera resposta, quando há uma. O box a desenha a partir
    /// do banco como desenha todo o resto: fechar e reabrir o aplicativo
    /// encontra a mesma pergunta esperando.
    #[serde(default)]
    pub question: Option<QuestionView>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// O modo que o desenvolvedor fixou para o chat: `auto` (o Jev escolhe a
    /// cada pedido), `plan` ou `build`. Fica só nesta máquina.
    #[serde(default="auto_mode")]
    pub work_mode: String,
    /// O último pedido escrito no chat, para o cartão. Vem também no retrato
    /// leve (`overview`), em que `messages` chega vazio.
    #[serde(default)]
    pub last_prompt: Option<String>,
    /// Quantas mensagens o chat tem, mesmo quando `messages` vem vazio.
    #[serde(default)]
    pub message_count: usize,
}

pub const MODE_PLAN:&str="plan";
pub const MODE_BUILD:&str="build";
/// O chat sem modo fixo: o Jev escolhe planejamento ou build a cada pedido.
pub const MODE_AUTO:&str="auto";
/// O modo como o chat o guarda, ou nada se o valor não é um dos três.
/// As sessões dos agentes por chat, guardadas para sobreviver ao reinício do
/// app. Não sobem para o Supabase: a sessão mora no disco do agente desta
/// máquina. `instructions` é a impressão, em hexadecimal, das instruções com
/// que ela começou.
const AGENT_SESSIONS:&str="CREATE TABLE IF NOT EXISTS agent_sessions (
    chat_id TEXT PRIMARY KEY REFERENCES chats(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    root TEXT NOT NULL,
    session_id TEXT NOT NULL,
    turns INTEGER NOT NULL,
    writes INTEGER NOT NULL,
    instructions TEXT NOT NULL DEFAULT '0',
    updated_at TEXT NOT NULL
);";
/// Até quantos minutos depois de um bloqueio o pedido seguinte do chat é
/// tratado como reenvio dele.
pub const RESEND_MINUTES:i64=15;
/// A marca da economia de um pedido barrado.
pub const BLOCKED_SAVING:&str="saved_tokens:blocked";
/// Por quanto tempo uma sessão guardada ainda é retomada.
const AGENT_SESSION_HOURS:i64=24;

pub fn work_mode(value:&str)->Option<&'static str> { [MODE_AUTO,MODE_PLAN,MODE_BUILD].into_iter().find(|mode|*mode==value) }

fn auto_mode()->String { MODE_AUTO.into() }

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all="camelCase")]
pub struct WorkspaceData {
    pub projects: Vec<ProjectRecord>,
    pub chats: Vec<ChatRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TableCount { pub name: String, pub rows: i64 }

/// Onde o banco mora. Com um arquivo de configuração achado — o repositório,
/// em desenvolvimento —, ao lado dele, na pasta `.jev`. O aplicativo instalado
/// não traz configuração e sobe de pastas onde não se escreve (o AppImage
/// montado, o `Program Files`, a `/` do macOS), então o banco vai para a pasta
/// de dados do sistema, a mesma em que o WebView já guarda as dele.
pub fn database_location(config_path:&Path,root:&Path)->PathBuf { database_location_in(config_path,root,dirs::data_dir()) }

const APP_IDENTIFIER:&str="ai.jayv.desktop";

/// Onde fica o motivo de o aplicativo não ter aberto. Aberto pelo menu não há
/// terminal, e sem este arquivo o erro some junto com a janela.
/// A pasta de dados do app, onde moram o banco e as skills instaladas.
pub fn app_data_dir()->Option<PathBuf> { dirs::data_dir().map(|data_dir|data_dir.join(APP_IDENTIFIER)) }

pub fn startup_log_location()->Option<PathBuf> { dirs::data_dir().map(|data_dir|data_dir.join(APP_IDENTIFIER).join("startup-error.log")) }

fn database_location_in(config_path:&Path,root:&Path,data_dir:Option<PathBuf>)->PathBuf {
    match data_dir {
        Some(data_dir) if !config_path.is_file()=>data_dir.join(APP_IDENTIFIER).join("workspace.sqlite3"),
        _=>config_path.parent().unwrap_or(root).join(".jev").join("workspace.sqlite3"),
    }
}

pub struct WorkspaceStore {
    connection: Connection,
    path: PathBuf,
}

impl WorkspaceStore {
    pub fn open(path: PathBuf) -> Result<Self> {
        if let Some(parent)=path.parent(){fs::create_dir_all(parent).with_context(||format!("could not create {}",parent.display()))?;}
        let connection=Connection::open(&path).with_context(||format!("could not open {}",path.display()))?;
        connection.busy_timeout(Duration::from_secs(5))?;
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path,fs::Permissions::from_mode(0o600))?;
        }
        Self::prepare(connection,path)
    }

    /// O banco de antes do login: nada do que se escreve nele sobrevive ao
    /// fechamento, e nada dele sobe — não há de quem seja.
    pub fn in_memory() -> Result<Self> {
        Self::prepare(Connection::open_in_memory()?,PathBuf::from(":memory:"))
    }

    /// O banco de um usuário, um arquivo por conta na pasta de dados. O id vem
    /// de um token já validado, mas ainda assim só entra no nome do arquivo
    /// se for um UUID: é caminho de disco.
    pub fn for_user(dir: &Path, user_id: &str) -> Result<Self> {
        let user=Uuid::parse_str(user_id).with_context(||format!("invalid user id: `{user_id}`"))?;
        Self::open(dir.join(format!("workspace-{}.sqlite3",user.hyphenated())))
    }

    fn prepare(connection: Connection, path: PathBuf) -> Result<Self> {
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
               named INTEGER NOT NULL DEFAULT 0,
               created_at TEXT NOT NULL,
               updated_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS messages (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               uid TEXT NOT NULL UNIQUE DEFAULT (lower(hex(randomblob(16)))),
               chat_id TEXT NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
               turn_id TEXT,
               role TEXT NOT NULL,
               content TEXT NOT NULL,
               created_at TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS chats_project_updated ON chats(project_id, updated_at DESC);
             CREATE INDEX IF NOT EXISTS messages_chat_order ON messages(chat_id, created_at, id);
             CREATE TABLE IF NOT EXISTS app_metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL);"
        )?;
        connection.execute_batch(turns::SCHEMA)?;
        crate::llm::ensure(&connection)?;
        connection.execute_batch(crate::mcp::SCHEMA)?;
        crate::mcp::seed_defaults(&connection)?;
        connection.execute_batch(jayv_agents::skills::SCHEMA)?;
        connection.execute_batch(jayv_agents::org_extensions::SCHEMA)?;
        ensure_message_turns(&connection)?;
        ensure_chat_named(&connection)?;
        ensure_chat_work_mode(&connection)?;
        ensure_turn_partial(&connection)?;
        ensure_turn_local(&connection)?;
        ensure_turn_grants(&connection)?;
        ensure_message_uid(&connection)?;
        ensure_project_repo_keys(&connection)?;
        ensure_project_org(&connection)?;
        crate::usage::store::ensure(&connection)?;
        connection.execute_batch(crate::expertise::SCHEMA)?;
        connection.execute_batch(crate::policy::SCHEMA)?;
        crate::features::ensure(&connection)?;
        connection.execute_batch(crate::project_memory::SCHEMA)?;
        crate::search::ensure(&connection)?;
        connection.execute_batch(AGENT_SESSIONS)?;
        crate::local::outbox::install(&connection)?;
        fail_interrupted_turns(&connection)?;
        let mut store=Self{connection,path};
        store.ensure_chat_codes()?;
        // Os remotes mudam fora do app (um `git remote add`): a abertura
        // confere de novo. Pasta que sumiu só deixa a lista como estava.
        if let Err(error)=store.refresh_repo_keys() {eprintln!("repo keys: {error:#}");}
        Ok(store)
    }

    pub fn snapshot(&self) -> Result<WorkspaceData> {self.portrait(true)}

    /// O retrato da lateral e dos cartões: projetos e chats com os turnos,
    /// sem as mensagens — o último pedido e a contagem ficam no lugar delas.
    /// A conversa de um chat vem inteira por `chat_record`, quando ele é
    /// aberto. Reler tudo de todos os chats era a maior chamada da tela.
    pub fn overview(&self) -> Result<WorkspaceData> {self.portrait(false)}

    fn portrait(&self, full: bool) -> Result<WorkspaceData> {
        let project_rows={
            let mut statement=self.connection.prepare("SELECT id,name,root_path,created_at,repo_keys,org_id FROM projects ORDER BY created_at,id")?;
            statement.query_map([],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?,row.get::<_,String>(4)?,row.get::<_,Option<String>>(5)?)))?.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let projects=project_rows.into_iter().map(|(id,name,root_path,created_at,keys,org_id)|Ok(ProjectRecord{id,name,root_path,created_at:parse_time(&created_at)?,repo_keys:serde_json::from_str(&keys).unwrap_or_default(),org_id})).collect::<Result<Vec<_>>>()?;
        let chat_rows={
            let mut statement=self.connection.prepare("SELECT id,code,project_id,title,created_at,updated_at,work_mode FROM chats ORDER BY updated_at DESC,id")?;
            statement.query_map([],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?,row.get::<_,String>(4)?,row.get::<_,String>(5)?,row.get::<_,String>(6)?)))?.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let mut chats=Vec::with_capacity(chat_rows.len());
        for (id,code,project_id,title,created_at,updated_at,work_mode) in chat_rows {
            let (last_prompt,message_count)=self.message_summary(&id)?;
            chats.push(ChatRecord{id:id.clone(),code,project_id,title,messages:if full {self.messages(&id)?} else {vec![]},turns:turns::views_for_chat(&self.connection,&id)?,question:turns::pending_question(&self.connection,&id)?,created_at:parse_time(&created_at)?,updated_at:parse_time(&updated_at)?,work_mode,last_prompt,message_count});
        }
        Ok(WorkspaceData{projects,chats})
    }

    /// Um chat só, inteiro: o que a tela relê quando um pedido dele entra ou
    /// fecha, em vez de reler a conversa de todos os chats. Nada quando o chat
    /// não existe mais.
    pub fn chat_record(&self, chat_id: &str) -> Result<Option<ChatRecord>> {
        let row=self.connection.query_row("SELECT id,code,project_id,title,created_at,updated_at,work_mode FROM chats WHERE id=?1",[chat_id],
            |row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?,row.get::<_,String>(4)?,row.get::<_,String>(5)?,row.get::<_,String>(6)?))).optional()?;
        let Some((id,code,project_id,title,created_at,updated_at,work_mode))=row else { return Ok(None) };
        let (last_prompt,message_count)=self.message_summary(&id)?;
        Ok(Some(ChatRecord{messages:self.messages(&id)?,turns:turns::views_for_chat(&self.connection,&id)?,question:turns::pending_question(&self.connection,&id)?,id,code,project_id,title,created_at:parse_time(&created_at)?,updated_at:parse_time(&updated_at)?,work_mode,last_prompt,message_count}))
    }

    pub fn create_project(&mut self, name: &str, root_path: Option<String>) -> Result<ProjectRecord> {
        self.insert_project(name,root_path,None)
    }

    /// O projeto do chat da organização neste computador, com a pasta da
    /// organização como raiz. Reaproveita o que já existe: o desta pasta, ou o
    /// que veio de outro computador pela sincronização (sem pasta aqui), que
    /// ganha esta. Só cria quando não há nenhum.
    pub fn organization_project(&mut self, org_id: &str, name: &str, folder: &str) -> Result<ProjectRecord> {
        let org_id=org_id.trim();
        anyhow::ensure!(Uuid::parse_str(org_id).is_ok(),"invalid organization id: `{org_id}`");
        let folder=folder.trim();
        anyhow::ensure!(Path::new(folder).is_dir(),Text::new("project.folderMissing").with("path",folder));
        let wanted=folder_key(folder);
        let mut existing={
            let mut statement=self.connection.prepare("SELECT id,name,root_path,created_at FROM projects WHERE org_id=?1 ORDER BY created_at,id")?;
            let rows=statement.query_map([org_id],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
            rows.into_iter().map(|(id,name,root_path,created_at)|Ok(ProjectRecord{id,name,root_path,created_at:parse_time(&created_at)?,repo_keys:vec![],org_id:Some(org_id.into())})).collect::<Result<Vec<_>>>()?
        };
        if let Some(project)=existing.iter().find(|project|!project.root_path.trim().is_empty() && folder_key(&project.root_path)==wanted) {return Ok(project.clone());}
        if let Some(index)=existing.iter().position(|project|project.root_path.trim().is_empty()) {
            self.ensure_folder_free(folder)?;
            let mut project=existing.swap_remove(index);
            // A pasta é da máquina e não sobe: o `UPDATE` não entra na fila.
            self.connection.execute("UPDATE projects SET root_path=?1 WHERE id=?2",params![folder,project.id])?;
            project.root_path=folder.into();
            return Ok(project);
        }
        self.insert_project(name,Some(folder.into()),Some(org_id.into()))
    }

    fn ensure_folder_free(&self, root_path: &str) -> Result<()> {
        // Uma pasta é de um projeto só: dois projetos nela dividiriam o índice,
        // o cache e a narração dos agentes sem que ninguém percebesse.
        let wanted=folder_key(root_path);
        let mut statement=self.connection.prepare("SELECT name,root_path FROM projects WHERE trim(root_path)<>''")?;
        let folders=statement.query_map([],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        if let Some((owner,_))=folders.into_iter().find(|(_,path)|folder_key(path)==wanted) { anyhow::bail!(Text::new("project.pathTaken").with("name",owner)); }
        Ok(())
    }

    fn insert_project(&mut self, name: &str, root_path: Option<String>, org_id: Option<String>) -> Result<ProjectRecord> {
        let name=name.trim();
        anyhow::ensure!(!name.is_empty(),Text::new("project.nameRequired"));
        let root_path=root_path.map(|path|path.trim().to_string()).unwrap_or_default();
        if !root_path.is_empty() { self.ensure_folder_free(&root_path)?; }
        // A pasta da organização não é um repositório: os remotes dela não
        // dizem nada, o vínculo é o `org_id`.
        let repo_keys=if org_id.is_some() {vec![]} else {crate::repo_keys::of_folder(&root_path)};
        let project=ProjectRecord{id:Uuid::new_v4().to_string(),name:name.into(),root_path,created_at:Utc::now(),repo_keys,org_id};
        let keys=serde_json::to_string(&project.repo_keys)?;
        self.connection.execute("INSERT INTO projects(id,name,root_path,created_at,repo_keys,org_id) VALUES(?1,?2,?3,?4,?5,?6)",params![project.id,project.name,project.root_path,project.created_at.to_rfc3339(),keys,project.org_id])?;
        Ok(project)
    }

    /// Recalcula as chaves dos remotes de cada projeto com pasta e grava só o
    /// que mudou: cada gravação vira uma subida na fila.
    pub fn refresh_repo_keys(&mut self) -> Result<()> {
        let folders={
            let mut statement=self.connection.prepare("SELECT id,root_path,repo_keys FROM projects WHERE trim(root_path)<>'' AND org_id IS NULL")?;
            statement.query_map([],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?)))?.collect::<rusqlite::Result<Vec<_>>>()?
        };
        for (id,root,current) in folders {
            // A pasta de outra máquina (vinda pela sync) ou apagada não diz
            // nada: fica o que estava.
            if !Path::new(root.trim()).is_dir() {continue;}
            let keys=serde_json::to_string(&crate::repo_keys::of_folder(&root))?;
            if keys!=current {self.connection.execute("UPDATE projects SET repo_keys=?1 WHERE id=?2",params![keys,id])?;}
        }
        Ok(())
    }

    pub fn create_chat(&mut self, project_id: &str, title: Option<String>) -> Result<ChatRecord> {
        anyhow::ensure!(self.project_exists(project_id)?,Text::new("project.notFound"));
        let now=Utc::now();
        let title=title.unwrap_or_default().trim().to_string();
        let chat=ChatRecord{id:Uuid::new_v4().to_string(),code:self.unused_chat_code()?,project_id:project_id.into(),title,messages:vec![],turns:vec![],question:None,created_at:now,updated_at:now,work_mode:auto_mode(),last_prompt:None,message_count:0};
        self.connection.execute("INSERT INTO chats(id,code,project_id,title,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6)",params![chat.id,chat.code,chat.project_id,chat.title,chat.created_at.to_rfc3339(),chat.updated_at.to_rfc3339()])?;
        Ok(chat)
    }

    /// Aceita o pedido: abre o turno e grava o texto no mesmo ato, antes de
    /// existir qualquer processo para atendê-lo. É o único caminho de entrada
    /// de um pedido no aplicativo, e o ponto do sistema em que "enviado" passa
    /// a significar "está no disco". Depois daqui, fechar o aplicativo, perder
    /// a rede, trocar de chat ou mandar outra coisa por cima não faz diferença:
    /// o pedido existe, numerado, e a tela o lê do banco como lê qualquer
    /// mensagem antiga. Devolve o turno já na fila.
    ///
    /// `requested` só vem no reenvio, e aí é o turno que falhou voltando ao ar
    /// com o mesmo número: a resposta anterior sai, o pedido fica onde estava.
    pub fn enqueue_prompt(&mut self, chat_id: &str, user: &str, requested: Option<&str>) -> Result<Turn> {
        let transaction=self.connection.transaction()?;
        let (named,count):(bool,i64)=transaction.query_row("SELECT named,(SELECT COUNT(*) FROM messages WHERE chat_id=?1) FROM chats WHERE id=?1",[chat_id],|row|Ok((row.get(0)?,row.get(1)?))).context(Text::new("chat.notFound"))?;
        let turn=turns::open_or_reopen(&transaction,chat_id,requested)?;
        // A tentativa anterior deixou uma resposta — quase sempre a linha de
        // erro. Ela sai para não empilhar duas respostas sob o mesmo balão; o
        // pedido não é tocado.
        transaction.execute("DELETE FROM messages WHERE turn_id=?1 AND role='assistant'",[&turn.id])?;
        let now=Utc::now();
        if count==0 && !named {transaction.execute("UPDATE chats SET title=?1 WHERE id=?2",params![compact_title(user),chat_id])?;}
        // Repetir a mesma frase é um pedido novo e tem de aparecer duas vezes
        // no chat. Só o reenvio do mesmo turno é que não pode escrever de novo
        // o que já está escrito — e isso se decide pelo turno, nunca pelo
        // texto: foi comparar conteúdo que já fez pedido sumir da conversa.
        if !written(&transaction,&turn.id)? {insert_message(&transaction,chat_id,Some(&turn.id),"user",user,now)?;}
        transaction.execute("UPDATE chats SET updated_at=?1 WHERE id=?2",params![now.to_rfc3339(),chat_id])?;
        transaction.commit()?;
        Ok(turn)
    }

    /// Grava o pedido de um turno já aberto. Devolve `false` quando ele já
    /// estava escrito — o reenvio do mesmo turno.
    pub fn append_prompt(&mut self, chat_id: &str, turn_id: &str, user: &str) -> Result<bool> {
        let transaction=self.connection.transaction()?;
        let (named,count):(bool,i64)=transaction.query_row("SELECT named,(SELECT COUNT(*) FROM messages WHERE chat_id=?1) FROM chats WHERE id=?1",[chat_id],|row|Ok((row.get(0)?,row.get(1)?))).context(Text::new("chat.notFound"))?;
        if written(&transaction,turn_id)? {return Ok(false);}
        let now=Utc::now();
        if count==0 && !named {transaction.execute("UPDATE chats SET title=?1 WHERE id=?2",params![compact_title(user),chat_id])?;}
        insert_message(&transaction,chat_id,Some(turn_id),"user",user,now)?;
        transaction.execute("UPDATE chats SET updated_at=?1 WHERE id=?2",params![now.to_rfc3339(),chat_id])?;
        transaction.commit()?;
        Ok(true)
    }

    /// Chama o próximo da fila e o põe no ar, devolvendo junto o texto do
    /// pedido lido do banco. Quem atende nunca recebe o texto de fora: ele sai
    /// da mesma linha que a tela desenha, então o que é processado e o que o
    /// desenvolvedor lê são forçosamente a mesma coisa. Enquanto houver um
    /// pedido no ar ninguém é chamado — é isto, e não a ordem das chamadas na
    /// interface, que garante um pedido de cada vez.
    pub fn claim_next_turn(&mut self) -> Result<Option<(Turn,String)>> {self.claim_next_turn_within(1)}

    /// O mesmo, com até `limit` pedidos no ar ao mesmo tempo — de chats
    /// diferentes: o chat que já tem pedido no ar espera ele voltar.
    pub fn claim_next_turn_within(&mut self, limit:usize) -> Result<Option<(Turn,String)>> {
        if turns::flying_count(&self.connection)?>=limit.max(1) {return Ok(None);}
        loop {
            let Some(turn)=turns::next_queued_free(&self.connection)? else {return Ok(None)};
            let prompt:Option<String>=self.connection.query_row(
                "SELECT content FROM messages WHERE turn_id=?1 AND role='user' ORDER BY created_at,id LIMIT 1",[&turn.id],|row|row.get(0),
            ).optional()?;
            // Um turno na fila sem o pedido escrito (o chat foi limpo aqui ou em
            // outra máquina) não tem o que atender: falha e dá a vez ao
            // seguinte, em vez de travar a fila de todos os chats.
            let Some(prompt)=prompt else {
                eprintln!("turn `{}` is queued without a written request; failing it",turn.id);
                turns::set_status(&self.connection,&turn.id,TurnStatus::Failed)?;
                continue;
            };
            turns::set_status(&self.connection,&turn.id,TurnStatus::Flying)?;
            return Ok(Some((Turn{status:TurnStatus::Flying,..turn},prompt)));
        }
    }

    /// Se há pedido no ar agora.
    pub fn turn_in_flight(&self) -> Result<bool> {turns::is_flying(&self.connection)}

    /// Quantos pedidos deste chat ainda estão em aberto, contando o que está
    /// sendo atendido agora.
    pub fn queue_depth(&self, chat_id:&str) -> Result<u32> {turns::queue_depth(&self.connection,chat_id)}

    /// Fecha a troca com o que voltou do modelo — ou com o erro que veio no
    /// lugar dele. Enquanto isto não acontece, o pedido fica pendente.
    pub fn append_answer(&mut self, chat_id: &str, turn_id: &str, assistant: &str) -> Result<()> {
        anyhow::ensure!(self.contains_chat(chat_id)?,Text::new("chat.notFound"));
        let now=Utc::now();
        let transaction=self.connection.transaction()?;
        insert_message(&transaction,chat_id,Some(turn_id),"assistant",assistant,now)?;
        transaction.execute("UPDATE chats SET updated_at=?1 WHERE id=?2",params![now.to_rfc3339(),chat_id])?;
        transaction.commit()?;
        Ok(())
    }

    /// Apaga o que voltou do modelo neste turno — só isso. O pedido continua
    /// escrito: retentar não pode fazer sumir do chat o que o desenvolvedor
    /// mandou, e sem esta limpeza a segunda tentativa deixaria duas respostas
    /// empilhadas sob o mesmo balão.
    pub fn clear_turn_answer(&mut self, turn_id: &str) -> Result<()> {
        self.connection.execute("DELETE FROM messages WHERE turn_id=?1 AND role='assistant'",[turn_id])?;
        Ok(())
    }

    /// O que o agente disse antes de o pedido parar ("Parar" ou o teto de
    /// minutos) entra no chat logo antes do aviso da parada. A tela mostrou
    /// esse texto crescendo; o fim do pedido não pode apagá-lo.
    pub fn keep_said_before(&mut self, chat_id: &str, turn_id: &str, said: &str) -> Result<()> {
        let transaction=self.connection.transaction()?;
        let first:Option<String>=transaction.query_row("SELECT created_at FROM messages WHERE turn_id=?1 AND role='assistant' ORDER BY created_at,id LIMIT 1",[turn_id],|row|row.get(0)).optional()?;
        let at=first.as_deref().and_then(|at|turns::parse_time(at).ok()).map_or_else(Utc::now,|at|at-chrono::Duration::milliseconds(1));
        insert_message(&transaction,chat_id,Some(turn_id),"assistant",said,at)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn append_exchange(&mut self, chat_id: &str, turn_id: &str, user: &str, assistant: &str) -> Result<()> {
        self.append_prompt(chat_id,turn_id,user)?;
        self.append_answer(chat_id,turn_id,assistant)
    }

    /// Um chat que nenhum modelo batizou ainda: é o único momento em que vale
    /// gastar uma chamada para lhe dar um nome. Lê a marca no banco em vez de
    /// deduzir do histórico, porque o pedido entra no histórico antes de sair.
    pub fn chat_is_unnamed(&self, chat_id: &str) -> Result<bool> {
        let named:bool=self.connection.query_row("SELECT named FROM chats WHERE id=?1",[chat_id],|row|row.get(0)).context(Text::new("chat.notFound"))?;
        Ok(!named)
    }

    /// Troca o título sem mexer no `updated_at`: rebatizar não é movimento de
    /// conversa e não pode reordenar a lista lateral.
    pub fn rename_chat(&mut self, chat_id: &str, title: &str) -> Result<()> {
        let title=compact_title(title);
        anyhow::ensure!(self.connection.execute("UPDATE chats SET title=?1,named=1 WHERE id=?2",params![title,chat_id])?>0,Text::new("chat.notFound"));
        Ok(())
    }

    /// O modo fixado no chat; chat sem modo gravado fica no automático.
    /// O que foi liberado para o pedido deste turno, além das configurações.
    pub fn turn_grants(&self, turn_id: &str) -> Result<crate::llm::Grants> {
        let stored:Option<Option<String>>=self.connection.query_row("SELECT grants FROM turns WHERE id=?1",[turn_id],|row|row.get(0)).optional()?;
        Ok(stored.flatten().and_then(|json|serde_json::from_str(&json).ok()).unwrap_or_default())
    }

    pub fn set_turn_grants(&mut self, turn_id: &str, grants: &crate::llm::Grants) -> Result<()> {
        let json=(!grants.is_empty()).then(||serde_json::to_string(grants)).transpose()?;
        self.connection.execute("UPDATE turns SET grants=?1 WHERE id=?2",params![json,turn_id])?;
        Ok(())
    }

    /// Os comandos sempre permitidos no projeto do chat, na ordem em que
    /// foram permitidos.
    pub fn allowed_commands(&self, chat_id: &str) -> Result<Vec<String>> {
        let mut statement=self.connection.prepare("SELECT a.command FROM allowed_commands a JOIN chats c ON c.project_id=a.project_id WHERE c.id=?1 ORDER BY a.created_at,a.command")?;
        let commands=statement.query_map([chat_id],|row|row.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?;
        Ok(commands)
    }

    /// O que o desenvolvedor deixou ligado no seletor de permissões deste chat:
    /// vale para todo pedido dele, até desligar.
    pub fn chat_grants(&self, chat_id: &str) -> Result<crate::llm::Grants> {
        let stored:Option<String>=self.connection.query_row("SELECT grants FROM chat_grants WHERE chat_id=?1",[chat_id],|row|row.get(0)).optional()?;
        Ok(stored.and_then(|json|serde_json::from_str(&json).ok()).unwrap_or_default())
    }

    pub fn set_chat_grants(&mut self, chat_id: &str, grants: &crate::llm::Grants) -> Result<()> {
        anyhow::ensure!(self.contains_chat(chat_id)?,Text::new("chat.notFound"));
        if grants.is_empty() {
            self.connection.execute("DELETE FROM chat_grants WHERE chat_id=?1",[chat_id])?;
        } else {
            self.connection.execute("INSERT INTO chat_grants(chat_id,grants) VALUES(?1,?2) ON CONFLICT(chat_id) DO UPDATE SET grants=excluded.grants",params![chat_id,serde_json::to_string(grants)?])?;
        }
        Ok(())
    }

    pub fn allow_command(&mut self, chat_id: &str, command: &str) -> Result<()> {
        let command=command.trim();
        anyhow::ensure!(!command.is_empty(),"empty command");
        self.connection.execute("INSERT OR IGNORE INTO allowed_commands(project_id,command,created_at) SELECT project_id,?2,?3 FROM chats WHERE id=?1",params![chat_id,command,Utc::now().to_rfc3339()])?;
        Ok(())
    }

    pub fn forget_allowed_command(&mut self, chat_id: &str, command: &str) -> Result<()> {
        self.connection.execute("DELETE FROM allowed_commands WHERE command=?2 AND project_id=(SELECT project_id FROM chats WHERE id=?1)",params![chat_id,command])?;
        Ok(())
    }

    pub fn work_mode(&self, chat_id: &str) -> Result<String> {
        Ok(self.connection.query_row("SELECT work_mode FROM chats WHERE id=?1",[chat_id],|row|row.get(0)).optional()?.unwrap_or_else(auto_mode))
    }

    /// Fixa o modo do chat. Como o título, não mexe no `updated_at`: trocar
    /// de modo não é conversa e não reordena a lista.
    pub fn set_work_mode(&mut self, chat_id: &str, mode: &str) -> Result<()> {
        let mode=work_mode(mode).with_context(||format!("unknown work mode `{mode}`"))?;
        anyhow::ensure!(self.connection.execute("UPDATE chats SET work_mode=?1 WHERE id=?2",params![mode,chat_id])?>0,Text::new("chat.notFound"));
        Ok(())
    }

    pub fn clear_chat(&mut self, chat_id: &str) -> Result<()> {
        anyhow::ensure!(self.contains_chat(chat_id)?,Text::new("chat.notFound"));
        let transaction=self.connection.transaction()?;
        transaction.execute("DELETE FROM messages WHERE chat_id=?1",[chat_id])?;
        // Os pedidos que ainda esperavam saem junto com o texto deles: sem o
        // pedido escrito, a fila não teria o que mandar ao agente.
        transaction.execute("UPDATE turns SET status=?1 WHERE chat_id=?2 AND status=?3",params![TurnStatus::Failed.as_str(),chat_id,TurnStatus::Queued.as_str()])?;
        transaction.execute("UPDATE chats SET title='',named=0,updated_at=?1 WHERE id=?2",params![Utc::now().to_rfc3339(),chat_id])?;
        // Chat limpo é conversa nova: a sessão do agente fica para trás.
        transaction.execute("DELETE FROM agent_sessions WHERE chat_id=?1",[chat_id])?;
        transaction.commit()?;
        Ok(())
    }

    /// A sessão do agente guardada para o chat, se ainda vale: com mais de um
    /// dia sem uso ela é esquecida, porque o agente pode já tê-la apagado.
    pub fn agent_session(&self, chat_id:&str) -> Result<Option<crate::memory::AgentSession>> {
        let row:Option<(String,String,String,String,i64,bool,String,String)>=self.connection.query_row(
            "SELECT provider,model,root,session_id,turns,writes,instructions,updated_at FROM agent_sessions WHERE chat_id=?1",[chat_id],
            |row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?,row.get(7)?)),
        ).optional()?;
        let Some((provider,model,root,id,turns,writes,instructions,updated_at))=row else { return Ok(None) };
        let fresh=DateTime::parse_from_rfc3339(&updated_at).is_ok_and(|at|Utc::now()-at.with_timezone(&Utc)<=chrono::Duration::hours(AGENT_SESSION_HOURS));
        if !fresh { self.connection.execute("DELETE FROM agent_sessions WHERE chat_id=?1",[chat_id])?; return Ok(None); }
        Ok(Some(crate::memory::AgentSession{provider,model,root,id,turns:turns.max(0) as usize,writes,instructions:u64::from_str_radix(&instructions,16).unwrap_or(0)}))
    }

    /// Guarda a sessão do agente do chat, para ela sobreviver ao reinício do
    /// app. Fica só nesta máquina: a sessão mora no disco do agente daqui.
    pub fn keep_agent_session(&mut self, chat_id:&str, session:&crate::memory::AgentSession) -> Result<()> {
        self.connection.execute(
            "INSERT INTO agent_sessions(chat_id,provider,model,root,session_id,turns,writes,instructions,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)
             ON CONFLICT(chat_id) DO UPDATE SET provider=excluded.provider,model=excluded.model,root=excluded.root,session_id=excluded.session_id,
               turns=excluded.turns,writes=excluded.writes,instructions=excluded.instructions,updated_at=excluded.updated_at",
            params![chat_id,session.provider,session.model,session.root,session.id,session.turns as i64,session.writes,format!("{:x}",session.instructions),Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn forget_agent_session(&mut self, chat_id:&str) -> Result<()> {
        self.connection.execute("DELETE FROM agent_sessions WHERE chat_id=?1",[chat_id])?;
        Ok(())
    }

    pub fn delete_chat(&mut self, chat_id: &str) -> Result<()> {
        anyhow::ensure!(self.connection.execute("DELETE FROM chats WHERE id=?1",[chat_id])?>0,Text::new("chat.notFound"));
        Ok(())
    }

    pub fn delete_project(&mut self, project_id: &str) -> Result<Vec<String>> {
        let chat_ids=self.chat_ids_for_project(project_id)?;
        anyhow::ensure!(self.connection.execute("DELETE FROM projects WHERE id=?1",[project_id])?>0,Text::new("project.notFound"));
        Ok(chat_ids)
    }

    pub fn conversation(&self, chat_id: &str) -> Result<Vec<ChatMessage>> {
        anyhow::ensure!(self.contains_chat(chat_id)?,Text::new("chat.notFound"));
        // É o histórico que o modelo lê: avisos gravados para a tela vão em inglês.
        Ok(self.messages(chat_id)?.into_iter().map(|message|ChatMessage{role:message.role,content:crate::i18n::for_model(&message.content)}).collect())
    }

    /// O histórico que vai ao modelo junto do turno que está sendo atendido:
    /// só os turnos já fechados, na ordem dos turnos. O pedido atual entra
    /// pelo `process`, e os que ainda esperam na fila não podem aparecer antes
    /// da hora — nem a resposta de um deles colada ao pedido de outro.
    pub fn history_before_open_turns(&self, chat_id: &str) -> Result<Vec<ChatMessage>> {
        anyhow::ensure!(self.contains_chat(chat_id)?,Text::new("chat.notFound"));
        let mut statement=self.connection.prepare(
            "SELECT m.role,m.content FROM messages m LEFT JOIN turns t ON t.id=m.turn_id
             WHERE m.chat_id=?1 AND (m.turn_id IS NULL OR (t.id IS NOT NULL AND t.status NOT IN (?2,?3)))
             ORDER BY COALESCE(t.ordinal,0),m.created_at,m.id")?;
        let rows=statement.query_map(params![chat_id,TurnStatus::Queued.as_str(),TurnStatus::Flying.as_str()],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows.into_iter().map(|(role,content)|ChatMessage{role,content:crate::i18n::for_model(&content)}).collect())
    }

    pub fn contains_chat(&self, chat_id: &str) -> Result<bool> {
        Ok(self.connection.query_row("SELECT 1 FROM chats WHERE id=?1",[chat_id],|row|row.get::<_,i64>(0)).optional()?.is_some())
    }

    /// A pasta do projeto onde o chat foi criado, quando o projeto tem uma. É
    /// esse caminho que vai para o índice e para o prompt; projeto sem pasta
    /// devolve nada e quem chama decide o que usar no lugar.
    /// O projeto de um chat, para dar dono ao que o turno gastar.
    pub fn chat_project(&self, chat_id: &str) -> Result<Option<String>> {
        Ok(self.connection.query_row("SELECT project_id FROM chats WHERE id=?1",[chat_id],|row|row.get(0)).optional()?)
    }

    /// A política de LLM do projeto do chat, do cache que a sincronização
    /// mantém.
    pub fn chat_policy(&self, chat_id: &str) -> Result<Option<crate::policy::ProjectPolicy>> {crate::policy::for_chat(&self.connection,chat_id)}

    pub fn replace_project_policies(&mut self, rows: &[crate::policy::RemotePolicy]) -> Result<()> {crate::policy::replace_all(&mut self.connection,rows)}
    /// Os recursos do plano que valem agora (todos, se a lista nunca desceu).
    pub fn entitlements(&self) -> Result<crate::features::Entitlements> {crate::features::load(&self.connection)}
    pub fn replace_entitlements(&mut self, remote: &crate::features::RemoteFeatures) -> Result<()> {crate::features::replace(&self.connection,remote)}

    pub fn average_output(&self) -> Result<u64> {crate::usage::store::average_output(&self.connection)}

    pub fn chat_root(&self, chat_id: &str) -> Result<Option<PathBuf>> {
        let root:String=self.connection.query_row("SELECT projects.root_path FROM chats JOIN projects ON projects.id=chats.project_id WHERE chats.id=?1",[chat_id],|row|row.get(0)).context(Text::new("chat.notFound"))?;
        let root=root.trim();
        Ok((!root.is_empty()).then(||PathBuf::from(root)))
    }

    /// Abre o turno do pedido que está sendo enviado agora.
    pub fn open_turn(&mut self, chat_id:&str) -> Result<Turn> {turns::open_turn(&self.connection,chat_id)}

    /// O turno do pedido que sai agora: novo, ou o mesmo que falhou e voltou.
    pub fn open_or_reopen_turn(&mut self, chat_id:&str, requested:Option<&str>) -> Result<Turn> {
        turns::open_or_reopen(&self.connection,chat_id,requested)
    }

    /// Põe um turno falho de volta no ar, com o mesmo número.
    pub fn reopen_turn(&mut self, turn_id:&str) -> Result<Turn> {turns::reopen_turn(&self.connection,turn_id)}

    pub fn turn(&self, turn_id:&str) -> Result<Option<Turn>> {turns::turn(&self.connection,turn_id)}

    pub fn set_turn_status(&mut self, turn_id:&str, status:TurnStatus) -> Result<()> {turns::set_status(&self.connection,turn_id,status)}

    pub fn record_entry_check(&mut self, check:&EntryCheck) -> Result<()> {turns::record_entry(&self.connection,check)}

    pub fn record_exit_checks(&mut self, turn:&Turn, checks:&[ExitCheck]) -> Result<()> {turns::record_exits(&self.connection,turn,checks)}

    pub fn gate_feed(&self, chats:Option<&BTreeSet<String>>) -> Result<GateFeed> {turns::feed(&self.connection,chats)}

    pub fn record_beat(&mut self, turn_id:&str, kind:&str, detail:&serde_json::Value) -> Result<u32> {turns::record_beat(&self.connection,turn_id,kind,detail)}

    pub fn turn_activity(&self, turn_id:&str) -> Result<Vec<turns::Activity>> {turns::activity(&self.connection,turn_id)}

    pub fn turn_evidence(&self, turn_id:&str) -> Result<turns::TurnEvidence> {turns::evidence(&self.connection,turn_id)}

    pub fn set_turn_partial(&mut self, turn_id:&str, text:&str) -> Result<()> {turns::set_partial(&self.connection,turn_id,text)}

    pub fn clear_turn_partial(&mut self, turn_id:&str) -> Result<()> {turns::clear_partial(&self.connection,turn_id)}

    pub fn ask_question(&mut self, turn_id:&str, kind:&str, prompt:&str, options:&[String], source:&str) -> Result<()> {turns::ask(&self.connection,turn_id,kind,prompt,options,source)}

    pub fn pending_question(&self, chat_id:&str) -> Result<Option<QuestionView>> {turns::pending_question(&self.connection,chat_id)}

    pub fn question_of(&self, turn_id:&str) -> Result<Option<QuestionView>> {turns::question_of(&self.connection,turn_id)}

    pub fn settle_question(&mut self, turn_id:&str, status:&str, answered_by:Option<&str>) -> Result<bool> {turns::settle_question(&self.connection,turn_id,status,answered_by)}

    pub fn chat_of_turn(&self, turn_id:&str) -> Result<Option<String>> {turns::chat_of(&self.connection,turn_id)}

    pub fn question_origin(&self, turn_id:&str) -> Result<Vec<String>> {turns::question_origin(&self.connection,turn_id)}

    pub fn question_verdict(&self, turn_id:&str) -> Result<Option<crate::gatekeeper::EntryVerdict>> {turns::question_verdict(&self.connection,turn_id)}

    /// O plano à espera de execução neste chat (`turns::pending_plan`).
    pub fn pending_plan(&self, turn_id:&str) -> Result<Option<String>> {turns::pending_plan(&self.connection,turn_id)}
    pub fn previous_answer(&self, turn_id:&str) -> Result<Option<(crate::gatekeeper::EntryVerdict,chrono::DateTime<Utc>)>> {turns::previous_answer(&self.connection,turn_id)}

    pub fn answers_gate(&self, turn_id:&str) -> Result<bool> {turns::answers_gate(&self.connection,turn_id)}

    /// A economia que o bloqueio do pedido anterior do chat registrou, quando
    /// este pedido chegou em até `RESEND_MINUTES` depois dele: foi reenvio, e
    /// o bloqueio não poupou o que tinha contado. Nada quando o anterior não
    /// foi barrado, quando veio mais tarde ou quando a economia já foi desfeita.
    pub fn blocked_saving_to_revoke(&self, turn_id:&str) -> Result<Option<f64>> {
        let previous:Option<(String,String,String,String)>=self.connection.query_row(
            "SELECT previous.id,previous.status,previous.created_at,current.created_at FROM turns current
               JOIN turns previous ON previous.chat_id=current.chat_id AND previous.ordinal<current.ordinal
             WHERE current.id=?1 ORDER BY previous.ordinal DESC LIMIT 1",
            [turn_id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)),
        ).optional()?;
        let Some((previous,status,blocked_at,now))=previous else { return Ok(None) };
        if status!=TurnStatus::Blocked.as_str() { return Ok(None); }
        let gap=turns::parse_time(&now)?-turns::parse_time(&blocked_at)?;
        if gap>chrono::Duration::minutes(RESEND_MINUTES) { return Ok(None); }
        let saved:f64=self.connection.query_row("SELECT COALESCE(SUM(amount),0) FROM jev_records WHERE turn_id=?1 AND kind=?2",params![previous,BLOCKED_SAVING],|row|row.get(0))?;
        let revoked:f64=self.connection.query_row("SELECT COALESCE(SUM(amount),0) FROM jev_records WHERE turn_id=?1 AND kind=?2 AND amount<0",params![turn_id,BLOCKED_SAVING],|row|row.get(0))?;
        Ok((saved>0.0&&revoked==0.0).then_some(saved))
    }

    /// Os agentes e modelos que a tela Configuração do LLM grava.
    pub fn llm_settings(&self) -> Result<crate::llm::LlmSettings> {crate::llm::load(&self.connection)}

    pub fn save_llm_settings(&mut self, settings:&crate::llm::LlmSettings) -> Result<crate::llm::LlmSettings> {crate::llm::save(&mut self.connection,settings)}

    pub fn skills(&self) -> Result<Vec<jayv_agents::skills::Skill>> {jayv_agents::skills::load(&self.connection)}

    pub fn install_skill_folder(&mut self, root:&Path, source:&Path) -> Result<Vec<jayv_agents::skills::Skill>> {std::fs::create_dir_all(root)?; jayv_agents::skills::install_folder(&self.connection,root,source)}

    pub fn install_skill_text(&mut self, root:&Path, text:&str) -> Result<jayv_agents::skills::Skill> {std::fs::create_dir_all(root)?; jayv_agents::skills::install_text(&self.connection,root,text)}

    pub fn set_skill_enabled(&mut self, name:&str, enabled:bool) -> Result<()> {jayv_agents::skills::set_enabled(&self.connection,name,enabled)}

    pub fn remove_skill(&mut self, root:&Path, name:&str) -> Result<()> {jayv_agents::skills::remove(&self.connection,root,name)}

    /// Os servidores MCP e as skills que as organizações dão ao projeto do chat.
    pub fn chat_org_extensions(&self, chat_id:&str) -> Result<(Vec<crate::mcp::McpServer>,Vec<jayv_agents::skills::Skill>)> {jayv_agents::org_extensions::for_chat(&self.connection,chat_id)}

    /// O que as organizações de quem usa dão, para a tela (sem segredos).
    pub fn org_extensions(&self) -> Result<jayv_agents::org_extensions::OrgExtensions> {jayv_agents::org_extensions::listing(&self.connection)}

    pub fn replace_org_extensions(&mut self, rows:&[jayv_agents::org_extensions::Row]) -> Result<()> {jayv_agents::org_extensions::replace_all(&mut self.connection,rows)}

    pub fn mcp_servers(&self) -> Result<Vec<crate::mcp::McpServer>> {crate::mcp::load(&self.connection)}

    pub fn save_mcp_servers(&mut self, servers:Vec<crate::mcp::McpServer>) -> Result<Vec<crate::mcp::McpServer>> {crate::mcp::save(&mut self.connection,servers)}

    pub fn core_settings(&self, defaults:&crate::core_settings::CoreSettings) -> Result<crate::core_settings::CoreSettings> {crate::core_settings::load(&self.connection,defaults)}

    pub fn save_core_settings(&mut self, settings:&crate::core_settings::CoreSettings) -> Result<crate::core_settings::CoreSettings> {crate::core_settings::save(&self.connection,settings)}

    pub fn database_path(&self) -> &Path {&self.path}

    pub fn expertise(&self) -> Result<crate::expertise::Expertise> {crate::expertise::load(&self.connection)}

    pub fn save_expertise(&mut self, level:&str) -> Result<crate::expertise::Expertise> {crate::expertise::save(&self.connection,level)}

    pub fn lean_code(&self) -> Result<bool> {crate::expertise::load_lean(&self.connection)}

    pub fn save_lean_code(&mut self, enabled:bool) -> Result<bool> {crate::expertise::save_lean(&self.connection,enabled)}

    /// A conexão crua, para a fila de saída e a sincronização: é o único
    /// código de fora que fala SQL com o banco do usuário.
    pub fn record_usage(&self, entry:&crate::usage::Entry) -> Result<bool> {crate::usage::store::write(&self.connection,entry)}

    pub fn usage_report(&self, query:&crate::usage::store::Query) -> Result<crate::usage::store::Report> {crate::usage::store::report(&self.connection,query)}

    pub fn chat_usage(&self, chat_id:&str) -> Result<Vec<crate::usage::store::TurnUsage>> {crate::usage::store::chat_turns(&self.connection,chat_id)}

    pub fn connection(&self) -> &Connection {&self.connection}

    pub fn project_notes(&self, project_id:&str) -> Result<Vec<crate::project_memory::ProjectNote>> {crate::project_memory::notes(&self.connection,project_id)}
    pub fn save_project_note(&mut self, draft:&crate::project_memory::NoteDraft) -> Result<crate::project_memory::ProjectNote> {crate::project_memory::save(&self.connection,draft)}
    pub fn delete_project_note(&mut self, project_id:&str, id:&str) -> Result<()> {crate::project_memory::delete(&self.connection,project_id,id)}
    pub fn learn_project_note(&mut self, project_id:&str, criterion:&str, sentence:&str) -> Result<Option<crate::project_memory::ProjectNote>> {crate::project_memory::learn(&self.connection,project_id,criterion,sentence)}
    pub fn repeated_requests(&self, project_id:&str) -> Result<Vec<crate::project_memory::RepeatedRequest>> {crate::project_memory::repeated_requests(&self.connection,project_id)}
    pub fn search_chats(&self, project_id:&str, text:&str, limit:usize) -> Result<Vec<crate::search::SearchHit>> {crate::search::search(&self.connection,project_id,text,limit)}
    pub fn recall(&self, project_id:&str, chat_id:&str, request:&str) -> Result<Option<crate::search::Recall>> {crate::search::recall(&self.connection,project_id,chat_id,request)}
    /// Os critérios que seguraram o pedido anterior a este no mesmo chat.
    pub fn previous_failing_criteria(&self, turn_id:&str) -> Result<Vec<String>> {turns::previous_failing_criteria(&self.connection,turn_id)}
    /// Como os pedidos do desenvolvedor passaram pela portaria nos últimos dias.
    pub fn gate_history(&self, days:i64) -> Result<crate::expertise::GateHistory> {crate::expertise::gate_history(&self.connection,days)}

    pub fn connection_mut(&mut self) -> &mut Connection {&mut self.connection}

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
        anyhow::bail!("could not generate a free chat code")
    }

    fn project_exists(&self, project_id:&str)->Result<bool>{Ok(self.connection.query_row("SELECT 1 FROM projects WHERE id=?1",[project_id],|row|row.get::<_,i64>(0)).optional()?.is_some())}

    /// Os chats de um projeto — a Portaria usa isto para mostrar só o que
    /// passou pelos portões deste projeto.
    pub fn chat_ids_for_project(&self,project_id:&str)->Result<Vec<String>>{
        let mut statement=self.connection.prepare("SELECT id FROM chats WHERE project_id=?1")?;
        Ok(statement.query_map([project_id],|row|row.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Os chats de um punhado de projetos (os de uma organização), ou só um
    /// deles. Um chat de fora desses projetos não entra: o recorte nunca
    /// alarga a vista.
    pub fn chat_ids_for_projects(&self,project_ids:&[String],chat_id:Option<&str>)->Result<BTreeSet<String>>{
        let mut chats=BTreeSet::new();
        for project_id in project_ids { chats.extend(self.chat_ids_for_project(project_id)?); }
        if let Some(chat_id)=chat_id { chats.retain(|id|id==chat_id); }
        Ok(chats)
    }

    /// O último pedido do chat e quantas mensagens ele tem.
    fn message_summary(&self,chat_id:&str)->Result<(Option<String>,usize)>{
        let last=self.connection.query_row("SELECT content FROM messages WHERE chat_id=?1 AND role='user' ORDER BY created_at DESC,id DESC LIMIT 1",[chat_id],|row|row.get::<_,String>(0)).optional()?;
        let count:i64=self.connection.query_row("SELECT COUNT(*) FROM messages WHERE chat_id=?1",[chat_id],|row|row.get(0))?;
        Ok((last,count as usize))
    }

    fn messages(&self,chat_id:&str)->Result<Vec<WorkspaceMessage>>{
        let rows={
            let mut statement=self.connection.prepare("SELECT role,content,created_at,turn_id FROM messages WHERE chat_id=?1 ORDER BY created_at,id")?;
            statement.query_map([chat_id],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,Option<String>>(3)?)))?.collect::<rusqlite::Result<Vec<_>>>()?
        };
        rows.into_iter().map(|(role,content,created_at,turn_id)|Ok(WorkspaceMessage{role,content,created_at:parse_time(&created_at)?,turn_id})).collect()
    }
}

/// Abre espaço para o turno nas mensagens dos bancos antigos. Quem já estava
/// escrito fica com o turno vazio: é conversa de antes da portaria, e um balão
/// sem semáforo continua legível.
fn ensure_message_turns(connection:&Connection)->Result<()> {
    if !connection.prepare("SELECT 1 FROM pragma_table_info('messages') WHERE name='turn_id'")?.exists([])? {
        connection.execute_batch("ALTER TABLE messages ADD COLUMN turn_id TEXT")?;
    }
    Ok(())
}

/// Abre espaço para a resposta parcial nos bancos antigos. Quem já estava
/// escrito fica sem rascunho: aquelas respostas chegaram inteiras, de uma vez, e
/// não têm meio caminho para mostrar.
fn ensure_turn_partial(connection:&Connection)->Result<()> {
    if !connection.prepare("SELECT 1 FROM pragma_table_info('turns') WHERE name='partial'")?.exists([])? {
        connection.execute_batch("ALTER TABLE turns ADD COLUMN partial TEXT")?;
    }
    Ok(())
}

/// Os turnos baixados de outra máquina entram com `local = 0`; os que já
/// estavam aqui nasceram aqui.
fn ensure_turn_local(connection:&Connection)->Result<()> {
    if !connection.prepare("SELECT 1 FROM pragma_table_info('turns') WHERE name='local'")?.exists([])? {
        connection.execute_batch("ALTER TABLE turns ADD COLUMN local INTEGER NOT NULL DEFAULT 1")?;
    }
    Ok(())
}

/// As permissões liberadas para um pedido só e as do chat inteiro
/// (`llm::Grants`, em JSON) e os
/// comandos que o desenvolvedor mandou sempre permitir num projeto. Ficam
/// neste computador: são permissões dele, para os agentes daqui.
fn ensure_turn_grants(connection:&Connection)->Result<()> {
    if !connection.prepare("SELECT 1 FROM pragma_table_info('turns') WHERE name='grants'")?.exists([])? {
        connection.execute_batch("ALTER TABLE turns ADD COLUMN grants TEXT")?;
    }
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS allowed_commands (
           project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
           command TEXT NOT NULL,
           created_at TEXT NOT NULL,
           PRIMARY KEY(project_id,command)
         );
         CREATE TABLE IF NOT EXISTS chat_grants (
           chat_id TEXT PRIMARY KEY REFERENCES chats(id) ON DELETE CASCADE,
           grants TEXT NOT NULL
         );",
    )?;
    Ok(())
}

/// O id com que a mensagem é conhecida no Supabase. O `ALTER` não aceita
/// padrão calculado, então os bancos antigos ganham a coluna vazia, preenchida
/// em seguida, e só então única.
fn ensure_message_uid(connection:&Connection)->Result<()> {
    if !connection.prepare("SELECT 1 FROM pragma_table_info('messages') WHERE name='uid'")?.exists([])? {
        connection.execute_batch("ALTER TABLE messages ADD COLUMN uid TEXT")?;
    }
    connection.execute_batch(
        "UPDATE messages SET uid=lower(hex(randomblob(16))) WHERE uid IS NULL;
         CREATE UNIQUE INDEX IF NOT EXISTS messages_uid ON messages(uid);",
    )?;
    Ok(())
}

/// Quem batizou o chat: o resumo local do primeiro pedido, ou o modelo. Sem
/// esta marca a única pista era "o chat não tem mensagem nenhuma" — e desde que
/// o pedido passa a ser gravado antes de sair, essa pista deixa de existir.
/// Um chat que já existia e tem histórico é dado por batizado.
/// As chaves dos remotes do git da pasta (`["github.com/acme/api"]`), com o
/// `origin` primeiro: sobem para o servidor ligar o projeto à organização.
fn ensure_project_repo_keys(connection:&Connection)->Result<()> {
    if connection.prepare("SELECT 1 FROM pragma_table_info('projects') WHERE name='repo_keys'")?.exists([])? {return Ok(());}
    connection.execute_batch("ALTER TABLE projects ADD COLUMN repo_keys TEXT NOT NULL DEFAULT '[]'")?;
    Ok(())
}

fn ensure_project_org(connection:&Connection)->Result<()> {
    if connection.prepare("SELECT 1 FROM pragma_table_info('projects') WHERE name='org_id'")?.exists([])? {return Ok(());}
    connection.execute_batch("ALTER TABLE projects ADD COLUMN org_id TEXT")?;
    Ok(())
}

/// O modo do chat nos bancos antigos: todo chat que já existia segue no
/// automático, que era o único jeito antes.
fn ensure_chat_work_mode(connection:&Connection)->Result<()> {
    if !connection.prepare("SELECT 1 FROM pragma_table_info('chats') WHERE name='work_mode'")?.exists([])? {
        connection.execute_batch("ALTER TABLE chats ADD COLUMN work_mode TEXT NOT NULL DEFAULT 'auto'")?;
    }
    Ok(())
}

fn ensure_chat_named(connection:&Connection)->Result<()> {
    if connection.prepare("SELECT 1 FROM pragma_table_info('chats') WHERE name='named'")?.exists([])? {return Ok(());}
    connection.execute_batch("ALTER TABLE chats ADD COLUMN named INTEGER NOT NULL DEFAULT 0")?;
    connection.execute_batch("UPDATE chats SET named=1 WHERE EXISTS(SELECT 1 FROM messages WHERE messages.chat_id=chats.id)")?;
    Ok(())
}

/// Se o pedido deste turno já está escrito. É o que separa o reenvio de um
/// turno que falhou — o texto já está lá — de um pedido novo que por acaso
/// repete a frase anterior.
fn written(connection:&Connection,turn_id:&str)->Result<bool> {
    Ok(connection.query_row("SELECT EXISTS(SELECT 1 FROM messages WHERE turn_id=?1 AND role='user')",[turn_id],|row|row.get(0))?)
}

/// O pedido que o fechamento do app pegou no ar vira falho, com o motivo no
/// lugar da resposta: o balão oferece o reenvio, e nada recomeça sozinho.
fn fail_interrupted_turns(connection:&Connection)->Result<()> {
    let transaction=connection.unchecked_transaction()?;
    let notice=crate::i18n::notice(&[Text::new("turn.interrupted")]);
    for (turn,chat) in turns::fail_interrupted_turns(&transaction)? {
        transaction.execute("DELETE FROM messages WHERE turn_id=?1 AND role='assistant'",[&turn])?;
        insert_message(&transaction,&chat,Some(&turn),"assistant",&notice,Utc::now())?;
    }
    transaction.commit()?;
    Ok(())
}

/// O `uid` sai daqui e não do padrão da coluna: nos bancos antigos a coluna
/// entrou por `ALTER`, que não aceita padrão calculado.
fn insert_message(transaction:&Transaction<'_>,chat_id:&str,turn_id:Option<&str>,role:&str,content:&str,created_at:DateTime<Utc>)->Result<()> {
    transaction.execute("INSERT INTO messages(uid,chat_id,turn_id,role,content,created_at) VALUES(?1,?2,?3,?4,?5,?6)",params![Uuid::new_v4().simple().to_string(),chat_id,turn_id,role,content,created_at.to_rfc3339()])?;
    Ok(())
}

/// Seis caracteres do alfabeto de Crockford — sem I, L, O e U, que se
/// confundem com 1, 0 e V quando alguém lê o código em voz alta.
fn new_chat_code()->String {
    const ALPHABET:&[u8;32]=b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    Uuid::new_v4().as_bytes().iter().take(6).map(|byte|ALPHABET[(byte%32) as usize] as char).collect()
}

pub use crate::turns::parse_time;

/// A pasta como ela é comparada: o caminho real quando ela existe (atalhos e
/// `..` resolvidos), sem barra no fim e, no Windows, sem diferença de
/// maiúsculas nem de barra.
fn folder_key(path:&str)->String {
    let path=path.trim();
    let real=fs::canonicalize(path).map(|real|real.display().to_string()).unwrap_or_else(|_|path.to_string());
    let real=if cfg!(windows) { real.strip_prefix(r"\\?\").unwrap_or(&real).replace('\\',"/").to_lowercase() } else { real };
    let trimmed=real.trim_end_matches('/');
    if trimmed.is_empty() { "/".into() } else { trimmed.into() }
}

fn compact_title(input: &str) -> String {
    let mut title=input.split_whitespace().take(7).collect::<Vec<_>>().join(" ");
    if input.split_whitespace().count()>7 {title.push('…');}
    title
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(root:&tempfile::TempDir)->WorkspaceStore{WorkspaceStore::open(root.path().join("workspace.sqlite3")).expect("workspace")}

    #[test]
    fn the_database_sits_next_to_an_existing_configuration() {
        let root=tempfile::tempdir().expect("root");
        let config=root.path().join("config.yaml");
        fs::write(&config,"models: {}").expect("config");
        assert_eq!(database_location_in(&config,root.path(),Some("/dados".into())),root.path().join(".jev/workspace.sqlite3"));
    }

    #[test]
    fn without_configuration_the_database_goes_to_the_system_data_folder() {
        let root=tempfile::tempdir().expect("root");
        let config=root.path().join("config.yaml");
        assert_eq!(database_location_in(&config,root.path(),Some("/dados".into())),PathBuf::from("/dados/ai.jayv.desktop/workspace.sqlite3"));
        assert_eq!(database_location_in(&config,root.path(),None),root.path().join(".jev/workspace.sqlite3"));
        assert!(include_str!("../../../tauri.conf.json").contains(&format!("\"identifier\": \"{APP_IDENTIFIER}\"")),"a pasta de dados é a do identificador do aplicativo");
    }

    #[test]
    fn the_request_and_the_turn_enter_the_database_together() {
        let root=tempfile::tempdir().expect("root");
        let mut store=store(&root);
        let project=store.create_project("Produto",None).expect("project");
        let chat=store.create_chat(&project.id,None).expect("chat");

        let turn=store.enqueue_prompt(&chat.id,"desenha o cabeçalho",None).expect("fila");

        let saved=store.snapshot().expect("snapshot");
        let saved=saved.chats.iter().find(|entry|entry.id==chat.id).expect("chat salvo");
        assert_eq!(saved.messages.len(),1,"o pedido está no disco antes de qualquer ida ao modelo");
        assert_eq!(saved.messages[0].content,"desenha o cabeçalho");
        assert_eq!(saved.messages[0].turn_id.as_deref(),Some(turn.id.as_str()),"o pedido nasce preso ao seu turno");
        assert_eq!(turn.status,TurnStatus::Queued);
    }

    /// O retrato leve traz o chat sem as mensagens, com o último pedido e a
    /// contagem; o resto é igual ao inteiro.
    #[test]
    fn the_overview_leaves_the_messages_out() {
        let root=tempfile::tempdir().expect("root");
        let mut store=store(&root);
        let project=store.create_project("Produto",None).expect("project");
        let chat=store.create_chat(&project.id,None).expect("chat");
        let turn=store.enqueue_prompt(&chat.id,"primeiro",None).expect("um");
        store.append_answer(&chat.id,&turn.id,"feito").expect("resposta");
        store.enqueue_prompt(&chat.id,"segundo",None).expect("dois");
        let light=store.overview().expect("leve");
        let full=store.snapshot().expect("inteiro");
        let (light,full)=(&light.chats[0],&full.chats[0]);
        assert!(light.messages.is_empty());
        assert_eq!((light.last_prompt.as_deref(),light.message_count),(Some("segundo"),3));
        assert_eq!((full.last_prompt.as_deref(),full.message_count,full.messages.len()),(Some("segundo"),3,3));
        assert_eq!(serde_json::to_value(&light.turns).unwrap(),serde_json::to_value(&full.turns).unwrap());
    }

    /// O chat relido sozinho é o mesmo que vem no retrato de todos.
    #[test]
    fn one_chat_reads_the_same_as_in_the_snapshot() {
        let root=tempfile::tempdir().expect("root");
        let mut store=store(&root);
        let project=store.create_project("Produto",None).expect("project");
        let chat=store.create_chat(&project.id,None).expect("chat");
        store.enqueue_prompt(&chat.id,"desenha o cabeçalho",None).expect("fila");
        let alone=store.chat_record(&chat.id).expect("leitura").expect("chat");
        let all=store.snapshot().expect("snapshot");
        assert_eq!(serde_json::to_value(&alone).unwrap(),serde_json::to_value(all.chats.iter().find(|item|item.id==chat.id).unwrap()).unwrap());
        assert!(store.chat_record("sumiu").expect("leitura").is_none());
    }

    /// O pedido parado guarda o que o agente já tinha dito, antes do aviso
    /// da parada.
    #[test]
    fn what_was_said_before_a_stop_stays_before_the_notice() {
        let root=tempfile::tempdir().expect("root");
        let mut store=store(&root);
        let project=store.create_project("Produto",None).expect("project");
        let chat=store.create_chat(&project.id,None).expect("chat");
        let turn=store.enqueue_prompt(&chat.id,"refatora o roteador",None).expect("fila");
        let notice=crate::i18n::notice(&[Text::new("turn.cancelled")]);
        store.append_answer(&chat.id,&turn.id,&notice).expect("aviso");
        store.keep_said_before(&chat.id,&turn.id,"Comecei pelo roteador.").expect("dito");
        let contents:Vec<String>=store.conversation(&chat.id).expect("conversa").into_iter().map(|message|message.content).collect();
        assert_eq!(contents,vec!["refatora o roteador".to_string(),"Comecei pelo roteador.".into(),"The developer stopped this request before it finished.".into()]);
    }

    #[test]
    fn a_request_caught_by_the_shutdown_reopens_failed_with_the_reason() {
        let root=tempfile::tempdir().expect("root");
        let (chat,turn,waiting)={
            let mut store=store(&root);
            let project=store.create_project("Produto",None).expect("project");
            let chat=store.create_chat(&project.id,None).expect("chat");
            let turn=store.enqueue_prompt(&chat.id,"refatora o roteador",None).expect("fila");
            store.set_turn_status(&turn.id,TurnStatus::Flying).expect("no ar");
            let waiting=store.enqueue_prompt(&chat.id,"e depois os testes",None).expect("fila");
            (chat,turn,waiting)
        };

        let store=store(&root);

        assert_eq!(store.turn(&turn.id).expect("turno").expect("existe").status,TurnStatus::Failed,"nada recomeça sozinho");
        assert_eq!(store.turn(&waiting.id).expect("turno").expect("existe").status,TurnStatus::Queued,"o que só esperava a vez continua na fila");
        let saved=store.snapshot().expect("snapshot");
        let answer=saved.chats.iter().find(|entry|entry.id==chat.id).expect("chat").messages.iter().find(|message|message.role=="assistant").expect("o motivo no lugar da resposta");
        assert_eq!(answer.turn_id.as_deref(),Some(turn.id.as_str()));
        assert_eq!(crate::i18n::read_notice(&answer.content).map(|lines|lines[0].key.clone()).as_deref(),Some("turn.interrupted"));
        assert!(store.conversation(&chat.id).expect("conversa").iter().any(|message|message.content.starts_with("The app closed")),"o modelo lê o motivo em inglês");
    }

    #[test]
    fn sending_the_same_sentence_twice_keeps_both_in_the_chat() {
        let root=tempfile::tempdir().expect("root");
        let mut store=store(&root);
        let project=store.create_project("Produto",None).expect("project");
        let chat=store.create_chat(&project.id,None).expect("chat");

        let first=store.enqueue_prompt(&chat.id,"de novo",None).expect("primeiro");
        let second=store.enqueue_prompt(&chat.id,"de novo",None).expect("segundo");

        assert_ne!(first.id,second.id,"repetir a frase é um pedido novo, com número novo");
        let saved=store.snapshot().expect("snapshot");
        let saved=saved.chats.iter().find(|entry|entry.id==chat.id).expect("chat salvo");
        assert_eq!(saved.messages.len(),2,"nenhum pedido some do chat por parecer com o anterior");
    }

    #[test]
    fn retrying_does_not_write_the_request_twice() {
        let root=tempfile::tempdir().expect("root");
        let mut store=store(&root);
        let project=store.create_project("Produto",None).expect("project");
        let chat=store.create_chat(&project.id,None).expect("chat");
        let turn=store.enqueue_prompt(&chat.id,"tenta",None).expect("primeira");
        store.append_answer(&chat.id,&turn.id,"deu erro").expect("resposta");
        store.set_turn_status(&turn.id,TurnStatus::Failed).expect("falhou");

        let again=store.enqueue_prompt(&chat.id,"tenta",Some(&turn.id)).expect("retentativa");

        assert_eq!(again.id,turn.id,"a retentativa é o mesmo pedido");
        assert_eq!(again.status,TurnStatus::Queued,"e ele volta para o fim do seu próprio trabalho, não para o ar");
        let saved=store.snapshot().expect("snapshot");
        let saved=saved.chats.iter().find(|entry|entry.id==chat.id).expect("chat salvo");
        assert_eq!(saved.messages.len(),1,"o pedido continua único e a resposta que falhou saiu");
        assert_eq!(saved.messages[0].role,"user");
    }

    /// Com dois pedidos ao mesmo tempo, o de outro projeto sai junto; o
    /// segundo do mesmo chat, e o de outro chat do mesmo projeto, esperam.
    #[test]
    fn projects_run_side_by_side_but_each_project_stays_in_order() {
        let root=tempfile::tempdir().expect("root");
        let mut store=store(&root);
        let shop=store.create_project("Loja",None).expect("project");
        let blog=store.create_project("Blog",None).expect("project");
        let first=store.create_chat(&shop.id,None).expect("chat");
        let sibling=store.create_chat(&shop.id,None).expect("chat");
        let other=store.create_chat(&blog.id,None).expect("chat");
        store.enqueue_prompt(&first.id,"a1",None).expect("a1");
        store.enqueue_prompt(&first.id,"a2",None).expect("a2");
        store.enqueue_prompt(&sibling.id,"s1",None).expect("s1");
        store.enqueue_prompt(&other.id,"b1",None).expect("b1");
        let (a1,prompt)=store.claim_next_turn_within(2).expect("consulta").expect("a1");
        assert_eq!(prompt,"a1");
        let (_,prompt)=store.claim_next_turn_within(2).expect("consulta").expect("b1");
        assert_eq!(prompt,"b1","o outro projeto não espera");
        assert!(store.claim_next_turn_within(4).expect("consulta").is_none(),"o a2 e o s1 esperam o a1, mesmo com vaga");
        store.set_turn_status(&a1.id,TurnStatus::Answered).expect("respondido");
        let (_,prompt)=store.claim_next_turn_within(2).expect("consulta").expect("a2");
        assert_eq!(prompt,"a2","na ordem da fila");
        assert!(store.claim_next_turn_within(4).expect("consulta").is_none(),"o s1 espera o a2");
    }

    #[test]
    fn the_queued_text_is_read_back_from_the_database_not_the_screen() {
        let root=tempfile::tempdir().expect("root");
        let mut store=store(&root);
        let project=store.create_project("Produto",None).expect("project");
        let chat=store.create_chat(&project.id,None).expect("chat");
        store.enqueue_prompt(&chat.id,"primeiro",None).expect("um");
        store.enqueue_prompt(&chat.id,"segundo",None).expect("dois");

        let (turn,prompt)=store.claim_next_turn().expect("consulta").expect("há fila");
        assert_eq!(prompt,"primeiro","quem processa lê o pedido do disco, não de uma variável da tela");
        assert_eq!(turn.status,TurnStatus::Flying,"assumir a vez tira o pedido da fila e o põe no ar");
        assert!(store.claim_next_turn().expect("consulta").is_none(),"o segundo espera o primeiro voltar");

        store.set_turn_status(&turn.id,TurnStatus::Answered).expect("respondido");
        let (_,following)=store.claim_next_turn().expect("consulta").expect("agora é a vez dele");
        assert_eq!(following,"segundo");
    }

    #[test]
    fn clearing_a_chat_does_not_stall_the_queue_of_the_others() {
        let root=tempfile::tempdir().expect("root");
        let mut store=store(&root);
        let project=store.create_project("Produto",None).expect("project");
        let cleared=store.create_chat(&project.id,None).expect("chat limpo");
        let other=store.create_chat(&project.id,None).expect("outro chat");
        let waiting=store.enqueue_prompt(&cleared.id,"esperando",None).expect("na fila");
        store.enqueue_prompt(&other.id,"o de outro chat",None).expect("outro");

        store.clear_chat(&cleared.id).expect("limpa");
        assert_eq!(store.turn(&waiting.id).expect("lê").expect("turno").status,TurnStatus::Failed,"o pedido sem texto sai da fila");
        let (_,prompt)=store.claim_next_turn().expect("consulta").expect("a fila anda");
        assert_eq!(prompt,"o de outro chat");
    }

    #[test]
    fn a_queued_turn_without_its_text_is_failed_and_skipped() {
        let root=tempfile::tempdir().expect("root");
        let mut store=store(&root);
        let project=store.create_project("Produto",None).expect("project");
        let chat=store.create_chat(&project.id,None).expect("chat");
        let orphan=store.enqueue_prompt(&chat.id,"some",None).expect("um");
        store.enqueue_prompt(&chat.id,"fica",None).expect("dois");
        // O texto sumiu por fora (a limpeza veio de outra máquina pelo sync).
        store.connection().execute("DELETE FROM messages WHERE turn_id=?1",[&orphan.id]).expect("apaga");

        let (_,prompt)=store.claim_next_turn().expect("consulta").expect("o seguinte é chamado");
        assert_eq!(prompt,"fica");
        assert_eq!(store.turn(&orphan.id).expect("lê").expect("turno").status,TurnStatus::Failed);
    }

    #[test]
    fn only_a_failed_turn_can_be_retried() {
        let root=tempfile::tempdir().expect("root");
        let mut store=store(&root);
        let project=store.create_project("Produto",None).expect("project");
        let chat=store.create_chat(&project.id,None).expect("chat");
        let turn=store.enqueue_prompt(&chat.id,"pronto",None).expect("pedido");
        store.append_answer(&chat.id,&turn.id,"a resposta").expect("resposta");
        store.set_turn_status(&turn.id,TurnStatus::Answered).expect("respondido");

        assert!(store.enqueue_prompt(&chat.id,"pronto",Some(&turn.id)).is_err(),"reenviar um respondido apagaria a resposta");
        let saved=store.snapshot().expect("snapshot");
        assert_eq!(saved.chats.iter().find(|entry|entry.id==chat.id).expect("chat").messages.len(),2,"a resposta continua lá");
    }

    #[test]
    fn the_history_skips_open_turns_and_follows_turn_order() {
        let root=tempfile::tempdir().expect("root");
        let mut store=store(&root);
        let project=store.create_project("Produto",None).expect("project");
        let chat=store.create_chat(&project.id,None).expect("chat");
        let first=store.enqueue_prompt(&chat.id,"u1",None).expect("um");
        store.append_answer(&chat.id,&first.id,"a1").expect("r1");
        store.set_turn_status(&first.id,TurnStatus::Answered).expect("fechou");
        let second=store.enqueue_prompt(&chat.id,"u2",None).expect("dois");
        store.enqueue_prompt(&chat.id,"u3",None).expect("três");
        store.claim_next_turn().expect("consulta").expect("u2 no ar");

        let history=store.history_before_open_turns(&chat.id).expect("histórico");
        assert_eq!(history.iter().map(|message|message.content.as_str()).collect::<Vec<_>>(),["u1","a1"],"nem o atual nem o que espera");
        store.append_answer(&chat.id,&second.id,"a2").expect("r2");
        store.set_turn_status(&second.id,TurnStatus::Answered).expect("fechou");
        let history=store.history_before_open_turns(&chat.id).expect("histórico");
        assert_eq!(history.iter().map(|message|message.content.as_str()).collect::<Vec<_>>(),["u1","a1","u2","a2"],"a resposta fica colada ao seu pedido");
    }

    #[test]
    fn a_claimed_request_does_not_restart_alone_after_closing_the_app() {
        let root=tempfile::tempdir().expect("root");
        let path=root.path().join("workspace.sqlite3");
        let chat_id={
            let mut store=WorkspaceStore::open(path.clone()).expect("workspace");
            let project=store.create_project("Produto",None).expect("project");
            let chat=store.create_chat(&project.id,None).expect("chat");
            store.enqueue_prompt(&chat.id,"não me perca",None).expect("fila");
            store.claim_next_turn().expect("consulta").expect("assume");
            chat.id
        };

        let mut store=WorkspaceStore::open(path).expect("reabre");
        assert!(store.claim_next_turn().expect("consulta").is_none(),"o que estava no ar não recomeça sozinho");
        let turn=store.snapshot().expect("snapshot").chats.into_iter().find(|chat|chat.id==chat_id).expect("chat").messages.into_iter().find_map(|message|message.turn_id).expect("turno");
        store.reopen_turn(&turn).expect("o reenvio é de quem pediu");
        let (_,prompt)=store.claim_next_turn().expect("consulta").expect("reenviado, volta à fila");
        assert_eq!(prompt,"não me perca","o pedido não se perde");
    }

    /// O defeito relatado: mandar uma segunda coisa enquanto a primeira estava
    /// sendo respondida fazia a segunda sumir da tela. Ela sumia porque nunca
    /// chegava ao banco — ficava presa esperando o cadeado do modelo — e a
    /// primeira, ao terminar, redesenhava a conversa a partir do disco.
    #[test]
    fn sending_over_a_running_request_never_erases_what_was_written() {
        let root=tempfile::tempdir().expect("root");
        let mut store=store(&root);
        let project=store.create_project("Produto",None).expect("project");
        let chat=store.create_chat(&project.id,None).expect("chat");

        store.enqueue_prompt(&chat.id,"o primeiro pedido",None).expect("primeiro");
        let (first,_)=store.claim_next_turn().expect("consulta").expect("assume");
        // O modelo está respondendo o primeiro. O desenvolvedor manda mais dois.
        let second=store.enqueue_prompt(&chat.id,"o segundo pedido",None).expect("segundo");
        let third=store.enqueue_prompt(&chat.id,"o terceiro pedido",None).expect("terceiro");

        let written_snapshot=|store:&WorkspaceStore|store.snapshot().expect("snapshot").chats.iter().find(|entry|entry.id==chat.id).expect("chat").messages.iter().filter(|message|message.role=="user").map(|message|message.content.clone()).collect::<Vec<_>>();
        assert_eq!(written_snapshot(&store),["o primeiro pedido","o segundo pedido","o terceiro pedido"],"os três estão no disco antes de qualquer resposta");

        // A primeira resposta chega e a tela é redesenhada a partir do banco.
        store.append_answer(&chat.id,&first.id,"pronto").expect("resposta");
        store.set_turn_status(&first.id,TurnStatus::Answered).expect("fechado");
        assert_eq!(written_snapshot(&store),["o primeiro pedido","o segundo pedido","o terceiro pedido"],"nada some do chat quando a conversa é redesenhada");

        assert_eq!(store.queue_depth(&chat.id).expect("fila"),2,"os dois que esperam continuam na fila");
        assert_eq!(store.claim_next_turn().expect("consulta").expect("vez").0.id,second.id,"a vez é de quem chegou primeiro");
        store.set_turn_status(&second.id,TurnStatus::Answered).expect("fechado");
        assert_eq!(store.claim_next_turn().expect("consulta").expect("vez").0.id,third.id);
    }

    #[test]
    fn leaving_the_chat_mid_queue_does_not_undo_the_queue() {
        let root=tempfile::tempdir().expect("root");
        let mut store=store(&root);
        let project=store.create_project("Produto",None).expect("project");
        let here=store.create_chat(&project.id,None).expect("chat");
        let there=store.create_chat(&project.id,None).expect("outro");
        store.enqueue_prompt(&here.id,"fica esperando",None).expect("pedido");
        store.claim_next_turn().expect("consulta").expect("assume");
        store.enqueue_prompt(&here.id,"este também",None).expect("pedido");

        // O desenvolvedor vai para outro chat e manda outra coisa de lá.
        store.enqueue_prompt(&there.id,"de outro chat",None).expect("pedido");

        assert_eq!(store.queue_depth(&here.id).expect("fila"),2,"a fila do chat de origem não se desfaz porque ninguém está olhando");
        assert_eq!(store.queue_depth(&there.id).expect("fila"),1);
    }

    #[test]
    fn a_database_older_than_the_naming_flag_still_opens() {
        let root=tempfile::tempdir().expect("root");
        let path=root.path().join("workspace.sqlite3");
        {
            let connection=rusqlite::Connection::open(&path).expect("sqlite");
            connection.execute_batch(
                "CREATE TABLE projects (id TEXT PRIMARY KEY,name TEXT NOT NULL,root_path TEXT NOT NULL DEFAULT '',created_at TEXT NOT NULL);
                 CREATE TABLE chats (id TEXT PRIMARY KEY,code TEXT NOT NULL DEFAULT '',project_id TEXT NOT NULL,title TEXT NOT NULL,created_at TEXT NOT NULL,updated_at TEXT NOT NULL);
                 CREATE TABLE messages (id INTEGER PRIMARY KEY AUTOINCREMENT,chat_id TEXT NOT NULL,turn_id TEXT,role TEXT NOT NULL,content TEXT NOT NULL,created_at TEXT NOT NULL);
                 INSERT INTO projects VALUES('p','Antigo','','2024-01-01T00:00:00Z');
                 INSERT INTO chats VALUES('usado','AAAAAA','p','Cálculo do frete','2024-01-01T00:00:00Z','2024-01-01T00:00:00Z');
                 INSERT INTO chats VALUES('vazio','BBBBBB','p','Novo chat','2024-01-02T00:00:00Z','2024-01-02T00:00:00Z');
                 INSERT INTO messages(chat_id,role,content,created_at) VALUES('usado','user','e o frete?','2024-01-01T00:00:00Z');"
            ).expect("esquema anterior à marca");
        }

        let store=WorkspaceStore::open(path).expect("workspace");

        assert!(!store.chat_is_unnamed("usado").expect("chat com histórico"),"quem já tem conversa fica com o título que tem");
        assert!(store.chat_is_unnamed("vazio").expect("chat vazio"),"o chat que nunca foi usado ainda tem direito a um nome");
    }

    /// O par que a Portaria julga: o turno-resposta aponta para a pergunta, e a
    /// pergunta aponta para o pedido que a originou. É esse caminho de volta que
    /// dá assunto a um `SIM` — sem ele, a resposta chegaria ao portão como uma
    /// palavra solta e seria barrada por faltas que o pedido de origem já supriu.
    #[test]
    fn the_answer_turn_finds_the_request_behind_the_question() {
        let root=tempfile::tempdir().expect("root");
        let mut store=store(&root);
        let project=store.create_project("Produto",None).expect("project");
        let chat=store.create_chat(&project.id,None).expect("chat");
        let question_turn=store.enqueue_prompt(&chat.id,"Troque o provedor padrão em config.yaml",None).expect("pedido");
        store.append_answer(&chat.id,&question_turn.id,"Qual provedor?").expect("resposta");
        store.ask_question(&question_turn.id,"single","Qual provedor?",&["Anthropic".into(),"OpenAI".into()],"jev").expect("pergunta");

        let open_question=store.snapshot().expect("snapshot").chats.into_iter().find(|item|item.id==chat.id).expect("chat").question.expect("pergunta na caixa");
        assert_eq!(open_question.turn_id,question_turn.id,"é o retrato do banco que veste a caixa de enviar mensagem");

        let answer_turn=store.enqueue_prompt(&chat.id,"Resposta à pergunta «Qual provedor?»: Anthropic",None).expect("turno-resposta");
        assert!(store.settle_question(&question_turn.id,turns::QUESTION_ANSWERED,Some(&answer_turn.id)).expect("encerrar"));

        assert_eq!(store.question_origin(&answer_turn.id).expect("origem"),["Troque o provedor padrão em config.yaml"]);
        assert_eq!(store.chat_of_turn(&answer_turn.id).expect("chat").as_deref(),Some(chat.id.as_str()));
        assert!(store.question_origin(&question_turn.id).expect("origem").is_empty(),"o pedido original não responde a pergunta nenhuma");
        assert!(store.snapshot().expect("snapshot").chats.into_iter().find(|item|item.id==chat.id).expect("chat").question.is_none(),"respondida não trava mais a caixa");
    }

    /// O plano pergunta, o desenvolvedor responde, e o agente pergunta de novo:
    /// a segunda pergunta nasceu num turno-resposta. A corrente tem de voltar
    /// até o pedido de origem — parar na resposta anterior deixa a portaria
    /// julgando "Resposta à pergunta…" sem assunto, e ela barra.
    #[test]
    fn a_follow_up_answer_walks_back_to_the_request_that_started_it() {
        let root=tempfile::tempdir().expect("root");
        let mut store=store(&root);
        let project=store.create_project("Produto",None).expect("project");
        let chat=store.create_chat(&project.id,None).expect("chat");
        let request=store.enqueue_prompt(&chat.id,"Planeje a troca do provedor padrão em config.yaml",None).expect("pedido");
        store.append_answer(&chat.id,&request.id,"Qual provedor?").expect("resposta");
        store.ask_question(&request.id,"single","Qual provedor?",&["Anthropic".into(),"OpenAI".into()],"jev").expect("pergunta");
        let mut passed=crate::gatekeeper::judge(&request,"Planeje a troca do provedor padrão em config.yaml",&crate::gatekeeper::heuristic_entry("Planeje a troca do provedor padrão em config.yaml"),"local");
        passed.verdict=crate::gatekeeper::EntryVerdict::Pass;
        store.record_entry_check(&passed).expect("leitura da portaria");

        let first=store.enqueue_prompt(&chat.id,"Resposta à pergunta «Qual provedor?»: Anthropic",None).expect("primeira resposta");
        assert!(store.settle_question(&request.id,turns::QUESTION_ANSWERED,Some(&first.id)).expect("encerrar"));
        store.append_answer(&chat.id,&first.id,"Já saiu do modo plano?").expect("resposta");
        store.ask_question(&first.id,"noul","Já saiu do modo plano?",&[],"jev").expect("segunda pergunta");

        let second=store.enqueue_prompt(&chat.id,"Resposta à pergunta «Já saiu do modo plano?»: Ainda não, aviso quando sair",None).expect("segunda resposta");
        assert!(store.settle_question(&first.id,turns::QUESTION_ANSWERED,Some(&second.id)).expect("encerrar"));

        assert_eq!(
            store.question_origin(&second.id).expect("origem"),
            ["Planeje a troca do provedor padrão em config.yaml","Resposta à pergunta «Qual provedor?»: Anthropic"],
            "o pedido de origem abre a corrente, e a resposta anterior vem depois",
        );
        assert_eq!(store.question_verdict(&first.id).expect("veredito"),Some(crate::gatekeeper::EntryVerdict::Pass));
        assert_eq!(store.question_verdict(&second.id).expect("veredito"),None,"a primeira resposta ainda não tem leitura gravada");
        assert_eq!(store.question_verdict(&request.id).expect("veredito"),None,"o pedido de origem não responde a nada");
    }

    /// O plano que o chat espera sai do banco: a resposta do último turno que
    /// chegou a um agente em planejamento. Barrado ou confirmado não conta, e
    /// um turno que construiu depois do plano o encerra.
    #[test]
    fn the_plan_waiting_in_the_chat_is_read_from_the_database() {
        let root=tempfile::tempdir().expect("root");
        let mut store=store(&root);
        let project=store.create_project("Produto",None).expect("project");
        let chat=store.create_chat(&project.id,None).expect("chat");
        let planned=store.open_turn(&chat.id).expect("turno do plano");
        store.record_beat(&planned.id,"route",&serde_json::json!({"kind":"route","provider":"claude","model":"sonnet","reason":"x","mode":"plan","agent":null})).expect("route");
        store.append_exchange(&chat.id,&planned.id,"faça X","1. criar\n2. testar").expect("exchange");
        store.set_turn_status(&planned.id,TurnStatus::Answered).expect("respondido");
        let blocked=store.open_turn(&chat.id).expect("turno barrado");
        store.append_answer(&chat.id,&blocked.id,"barrado").expect("resposta");
        store.set_turn_status(&blocked.id,TurnStatus::Blocked).expect("barrado");
        let follow=store.open_turn(&chat.id).expect("continuação");
        assert!(store.pending_plan(&follow.id).expect("leitura").is_some_and(|plan|plan.contains("2. testar")),"o turno barrado não esconde o plano");
        assert!(store.pending_plan(&planned.id).expect("leitura").is_none(),"o primeiro turno não tem plano antes dele");

        store.record_beat(&follow.id,"route",&serde_json::json!({"kind":"route","provider":"claude","model":"sonnet","reason":"x","mode":"build","agent":null})).expect("route");
        store.append_exchange(&chat.id,&follow.id,"siga","feito").expect("exchange");
        store.set_turn_status(&follow.id,TurnStatus::Answered).expect("respondido");
        let after=store.open_turn(&chat.id).expect("depois");
        assert!(store.pending_plan(&after.id).expect("leitura").is_none(),"o build encerrou o plano");
    }

    /// "Pode implementar" logo depois de uma resposta é continuação: o pedido
    /// anterior do chat diz com que veredito ele foi atendido e quando. A
    /// confirmação da portaria é reconhecida pela origem da pergunta.
    #[test]
    fn a_short_follow_up_finds_the_verdict_of_the_answered_request() {
        let root=tempfile::tempdir().expect("root");
        let mut store=store(&root);
        let project=store.create_project("Produto",None).expect("project");
        let chat=store.create_chat(&project.id,None).expect("chat");
        let request=store.enqueue_prompt(&chat.id,"Planeje o cache do roteador em src/router.rs",None).expect("pedido");
        let mut passed=crate::gatekeeper::judge(&request,"Planeje o cache",&crate::gatekeeper::heuristic_entry("Planeje o cache"),"local");
        passed.verdict=crate::gatekeeper::EntryVerdict::Pass;
        store.record_entry_check(&passed).expect("leitura");
        let follow=store.enqueue_prompt(&chat.id,"pode implementar",None).expect("continuação");
        assert_eq!(store.previous_answer(&follow.id).expect("leitura"),None,"sem resposta ainda, não há o que continuar");

        store.append_answer(&chat.id,&request.id,"1. criar o cache\n2. testar").expect("resposta");
        store.set_turn_status(&request.id,TurnStatus::Answered).expect("respondido");
        let (verdict,at)=store.previous_answer(&follow.id).expect("leitura").expect("a resposta anterior");
        assert_eq!(verdict,crate::gatekeeper::EntryVerdict::Pass);
        assert!((Utc::now()-at).num_seconds()<60,"a hora é a da resposta");
        assert!(store.previous_answer(&request.id).expect("leitura").is_none(),"o primeiro pedido não continua nada");

        store.ask_question(&follow.id,"single","?",&crate::gatekeeper::CONFIRM_OPTIONS.map(String::from),crate::gatekeeper::GATE_SOURCE).expect("confirmação");
        let confirmed=store.enqueue_prompt(&chat.id,&crate::gatekeeper::gate_answer(&["send_as_is".into()],None).expect("escolha"),None).expect("resposta");
        assert!(store.settle_question(&follow.id,turns::QUESTION_ANSWERED,Some(&confirmed.id)).expect("encerrar"));
        assert!(store.answers_gate(&confirmed.id).expect("leitura"),"a resposta é da confirmação da portaria");
        assert!(!store.answers_gate(&follow.id).expect("leitura"));
    }

    /// A sessão do agente volta depois de fechar e abrir o app, some com o
    /// chat limpo e expira depois de um dia.
    #[test]
    fn the_agent_session_survives_a_restart_and_expires() {
        let root=tempfile::tempdir().expect("root");
        let session=crate::memory::AgentSession{provider:"claude".into(),model:"sonnet".into(),root:"/repo".into(),id:"s-1".into(),turns:2,writes:true,instructions:0xabc};
        let chat_id={
            let mut store=store(&root);
            let project=store.create_project("Produto",None).expect("project");
            let chat=store.create_chat(&project.id,None).expect("chat");
            store.keep_agent_session(&chat.id,&session).expect("guarda");
            chat.id
        };
        let mut store=store(&root);
        assert_eq!(store.agent_session(&chat_id).expect("leitura"),Some(session.clone()),"reabrir o banco traz a sessão");
        store.connection.execute("UPDATE agent_sessions SET updated_at='2000-01-01T00:00:00Z'",[]).expect("envelhece");
        assert_eq!(store.agent_session(&chat_id).expect("leitura"),None,"depois de um dia o agente pode já tê-la apagado");
        store.keep_agent_session(&chat_id,&session).expect("guarda de novo");
        store.clear_chat(&chat_id).expect("limpa");
        assert_eq!(store.agent_session(&chat_id).expect("leitura"),None,"chat limpo começa sessão nova");
        store.keep_agent_session(&chat_id,&session).expect("guarda de novo");
        store.forget_agent_session(&chat_id).expect("esquece");
        assert_eq!(store.agent_session(&chat_id).expect("leitura"),None);
    }

    /// O pedido barrado e reenviado logo em seguida não poupou nada: a
    /// economia que o bloqueio contou é desfeita uma vez.
    #[test]
    fn a_quick_resend_takes_back_the_saving_of_the_block() {
        let root=tempfile::tempdir().expect("root");
        let mut store=store(&root);
        let project=store.create_project("Produto",None).expect("project");
        let chat=store.create_chat(&project.id,None).expect("chat");
        let blocked=store.enqueue_prompt(&chat.id,"arruma isso",None).expect("barrado");
        store.set_turn_status(&blocked.id,TurnStatus::Blocked).expect("status");
        let scope=|turn:&str|crate::usage::Scope{project_id:Some(project.id.clone()),chat_id:Some(chat.id.clone()),turn_id:Some(turn.to_string())};
        store.record_usage(&crate::usage::Entry::Jev(scope(&blocked.id),crate::usage::JevMark::saved("blocked",300))).expect("economia");
        let resent=store.enqueue_prompt(&chat.id,"arruma o login em src/login.rs",None).expect("reenvio");
        assert_eq!(store.blocked_saving_to_revoke(&resent.id).expect("leitura"),Some(300.0));
        store.record_usage(&crate::usage::Entry::Jev(scope(&resent.id),crate::usage::JevMark{kind:BLOCKED_SAVING.into(),amount:-300.0,precision:crate::usage::Precision::Estimated})).expect("desfaz");
        assert_eq!(store.blocked_saving_to_revoke(&resent.id).expect("leitura"),None,"desfeita uma vez só");
        let later=store.enqueue_prompt(&chat.id,"e os testes?",None).expect("depois");
        assert_eq!(store.blocked_saving_to_revoke(&later.id).expect("leitura"),None,"o anterior não foi barrado");
        let old=store.enqueue_prompt(&chat.id,"outro",None).expect("barrado");
        store.set_turn_status(&old.id,TurnStatus::Blocked).expect("status");
        store.record_usage(&crate::usage::Entry::Jev(scope(&old.id),crate::usage::JevMark::saved("blocked",200))).expect("economia");
        store.connection.execute("UPDATE turns SET created_at='2000-01-01T00:00:00+00:00' WHERE id=?1",[&old.id]).expect("envelhece");
        let much_later=store.enqueue_prompt(&chat.id,"outro pedido",None).expect("muito depois");
        assert_eq!(store.blocked_saving_to_revoke(&much_later.id).expect("leitura"),None,"bem depois não é reenvio");
    }

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
        let store=WorkspaceStore::open(path).expect("workspace");
        let chats=store.snapshot().expect("snapshot").chats;
        assert_eq!(chats.len(),2);
        assert!(chats.iter().all(|chat|chat.code.len()==6),"quem já existia também ganha um código");
        assert_ne!(chats[0].code,chats[1].code);
    }

    #[test]
    fn the_request_and_the_answer_are_bound_to_the_same_turn() {
        let root=tempfile::tempdir().expect("root");
        let mut store=store(&root);
        let project=store.create_project("Produto",None).expect("project");
        let chat=store.create_chat(&project.id,None).expect("chat");
        let turn=store.open_turn(&chat.id).expect("turno");

        store.append_exchange(&chat.id,&turn.id,"e aí","opa").expect("troca");

        let saved=store.snapshot().expect("snapshot");
        let saved=saved.chats.iter().find(|entry|entry.id==chat.id).expect("chat salvo");
        assert_eq!(saved.messages.iter().map(|message|message.turn_id.clone()).collect::<Vec<_>>(),vec![Some(turn.id.clone()),Some(turn.id.clone())],"os dois balões apontam para o mesmo pedido");
        assert_eq!(saved.turns.len(),1,"o chat devolve o turno junto com as mensagens");
        assert_eq!(saved.turns[0].code,format!("{}·01",chat.code));
    }

    #[test]
    fn retrying_erases_the_failed_answer_but_never_the_request() {
        let root=tempfile::tempdir().expect("root");
        let mut store=store(&root);
        let project=store.create_project("Produto",None).expect("project");
        let chat=store.create_chat(&project.id,None).expect("chat");
        let legacy_turn=store.open_turn(&chat.id).expect("turno");
        store.append_exchange(&chat.id,&legacy_turn.id,"o de ontem","respondido").expect("troca");
        let turn=store.open_turn(&chat.id).expect("turno");
        store.append_exchange(&chat.id,&turn.id,"revise o frete","falhou: sem rede").expect("troca");

        store.clear_turn_answer(&turn.id).expect("limpeza");

        let saved=store.snapshot().expect("snapshot");
        let saved=saved.chats.iter().find(|entry|entry.id==chat.id).expect("chat salvo");
        assert_eq!(saved.messages.iter().map(|message|message.content.as_str()).collect::<Vec<_>>(),["o de ontem","respondido","revise o frete"],"some só o erro; o pedido e o que já estava respondido ficam");
    }

    #[test]
    fn an_old_database_without_turns_in_messages_still_opens() {
        let root=tempfile::tempdir().expect("root");
        let path=root.path().join("workspace.sqlite3");
        {
            let connection=rusqlite::Connection::open(&path).expect("sqlite");
            connection.execute_batch(
                "CREATE TABLE projects (id TEXT PRIMARY KEY,name TEXT NOT NULL,root_path TEXT NOT NULL DEFAULT '',created_at TEXT NOT NULL);
                 CREATE TABLE chats (id TEXT PRIMARY KEY,project_id TEXT NOT NULL,title TEXT NOT NULL,created_at TEXT NOT NULL,updated_at TEXT NOT NULL);
                 CREATE TABLE messages (id INTEGER PRIMARY KEY AUTOINCREMENT,chat_id TEXT NOT NULL,role TEXT NOT NULL,content TEXT NOT NULL,created_at TEXT NOT NULL);
                 INSERT INTO projects VALUES('p','Antigo','','2024-01-01T00:00:00Z');
                 INSERT INTO chats VALUES('c1','p','Um','2024-01-01T00:00:00Z','2024-01-01T00:00:00Z');
                 INSERT INTO messages(chat_id,role,content,created_at) VALUES('c1','user','oi','2024-01-01T00:00:00Z');"
            ).expect("esquema antigo");
        }
        let store=WorkspaceStore::open(path).expect("workspace");
        let chats=store.snapshot().expect("snapshot").chats;
        assert_eq!(chats[0].messages.len(),1,"o que já estava escrito não some");
        assert_eq!(chats[0].messages[0].turn_id,None,"conversa de antes dos turnos não ganha semáforo");
        assert!(chats[0].turns.is_empty());
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
    fn a_set_of_projects_narrows_to_one_of_its_chats_and_never_beyond() {
        let root=tempfile::tempdir().expect("root");
        let mut store=store(&root);
        let first=store.create_project("Um",None).expect("project");
        let second=store.create_project("Dois",None).expect("project");
        let outside=store.create_project("Fora",None).expect("project");
        let a=store.create_chat(&first.id,None).expect("chat");
        let b=store.create_chat(&second.id,None).expect("chat");
        let c=store.create_chat(&outside.id,None).expect("chat");
        let projects=[first.id.clone(),second.id.clone()];

        assert_eq!(store.chat_ids_for_projects(&projects,None).expect("ids"),[a.id.clone(),b.id.clone()].into_iter().collect());
        assert_eq!(store.chat_ids_for_projects(&projects,Some(&b.id)).expect("ids"),[b.id.clone()].into_iter().collect());
        assert!(store.chat_ids_for_projects(&projects,Some(&c.id)).expect("ids").is_empty());
        assert!(store.chat_ids_for_projects(&[],None).expect("ids").is_empty());
    }

    #[test]
    fn counts_every_table_of_the_database_by_name() {
        let root=tempfile::tempdir().expect("root");
        let path=root.path().join("workspace.sqlite3");
        let mut store=WorkspaceStore::open(path).expect("workspace");
        let project=store.create_project("Produto",None).expect("project");
        let chat=store.create_chat(&project.id,None).expect("chat");
        let turn=store.open_turn(&chat.id).expect("turno");
        store.append_exchange(&chat.id,&turn.id,"pergunta","resposta").expect("exchange");

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
            let mut store=WorkspaceStore::open(path.clone()).expect("workspace");
            let project=store.create_project("Produto",None).expect("project");
            let chat=store.create_chat(&project.id,None).expect("chat");
            let turn=store.open_turn(&chat.id).expect("turno");
            store.append_exchange(&chat.id,&turn.id,"Implemente o painel agora","Pronto").expect("exchange");
            chat.id
        };
        let reloaded=WorkspaceStore::open(path).expect("reloaded");
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
        let turn=store.open_turn(&chat.id).expect("turno");
        store.append_exchange(&chat.id,&turn.id,"Pergunta","Resposta").expect("exchange");
        assert_eq!(store.delete_project(&project.id).expect("delete"),vec![chat.id]);
        let data=store.snapshot().expect("snapshot");
        assert!(data.projects.is_empty());
        assert!(data.chats.is_empty());
    }

    /// A exclusão local do projeto tem de subir para o Supabase também a dos
    /// chats e do que eles guardam: é a fila que leva cada uma.
    #[test]
    fn project_delete_queues_its_chats_for_deletion() {
        let root=tempfile::tempdir().expect("root");
        let mut store=store(&root);
        let project=store.create_project("Produto",None).expect("project");
        let chat=store.create_chat(&project.id,None).expect("chat");
        let turn=store.open_turn(&chat.id).expect("turno");
        store.append_exchange(&chat.id,&turn.id,"Pergunta","Resposta").expect("exchange");
        store.connection().execute("DELETE FROM outbox",[]).unwrap();
        store.delete_project(&project.id).expect("delete");
        let queued:Vec<(String,String)>=store.connection().prepare("SELECT tbl,op FROM outbox ORDER BY seq").unwrap().query_map([],|row|Ok((row.get(0)?,row.get(1)?))).unwrap().map(Result::unwrap).collect();
        for table in ["projects","chats","turns","messages"] {
            assert!(queued.contains(&(table.into(),"delete".into())),"exclusão de {table} fora da fila: {queued:?}");
        }
        assert!(queued.iter().all(|(_,op)|op=="delete"),"{queued:?}");
    }

    #[test]
    fn the_organization_project_is_reused_and_takes_this_computers_folder() {
        let root=tempfile::tempdir().expect("root");
        let folder=tempfile::tempdir().expect("organization folder");
        let path=folder.path().display().to_string();
        let org="6f1c2a4e-0000-4000-8000-000000000001";
        let mut store=store(&root);
        let project=store.organization_project(org,"Acme",&path).expect("cria");
        assert_eq!(project.org_id.as_deref(),Some(org));
        assert!(project.repo_keys.is_empty(),"a pasta da organização não tem remote");
        assert_eq!(store.organization_project(org,"Acme",&format!("{path}/")).expect("de novo").id,project.id,"a mesma pasta, o mesmo projeto");
        let snapshot=store.snapshot().expect("snapshot");
        assert_eq!(snapshot.projects[0].org_id.as_deref(),Some(org),"o vínculo volta no retrato");

        // O projeto que veio de outro computador chega sem pasta: ganha esta.
        store.connection().execute("UPDATE projects SET root_path='' WHERE id=?1",[&project.id]).unwrap();
        let other=tempfile::tempdir().expect("outra pasta");
        let moved=store.organization_project(org,"Acme",&other.path().display().to_string()).expect("reaproveita");
        assert_eq!(moved.id,project.id);
        let chat=store.create_chat(&moved.id,None).unwrap();
        assert_eq!(store.chat_root(&chat.id).unwrap(),Some(other.path().to_path_buf()));

        assert!(store.organization_project("acme","Acme",&path).is_err(),"id de organização inválido");
        let missing=store.organization_project(org,"Acme","/no/such/folder").expect_err("pasta que não existe");
        assert_eq!(missing.downcast_ref::<Text>().map(|text|text.key.as_str()),Some("project.folderMissing"));
    }

    #[test]
    fn the_organization_folder_cannot_be_another_projects_folder() {
        let root=tempfile::tempdir().expect("root");
        let folder=tempfile::tempdir().expect("pasta");
        let path=folder.path().display().to_string();
        let mut store=store(&root);
        store.create_project("Solto",Some(path.clone())).expect("projeto");
        let taken=store.organization_project("6f1c2a4e-0000-4000-8000-000000000001","Acme",&path).expect_err("pasta ocupada");
        assert_eq!(taken.downcast_ref::<Text>().map(|text|text.key.as_str()),Some("project.pathTaken"));
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

    /// O balão diz quem respondeu mesmo depois de o turno fechar: o último
    /// `route` gravado vale, e o de antes do modo chega sem modo.
    #[test]
    fn an_answered_turn_remembers_who_answered_it() {
        let root=tempfile::tempdir().expect("root");
        let mut store=store(&root);
        let project=store.create_project("Produto",None).expect("project");
        let chat=store.create_chat(&project.id,None).expect("chat");
        let old=store.open_turn(&chat.id).expect("turno antigo");
        store.record_beat(&old.id,"route",&serde_json::json!({"kind":"route","provider":"claude","model":"sonnet","reason":"x"})).expect("route antigo");
        let turn=store.open_turn(&chat.id).expect("turno");
        store.record_beat(&turn.id,"route",&serde_json::json!({"kind":"route","provider":"claude","model":"haiku","reason":"x","mode":"plan","agent":null})).expect("route");
        store.record_beat(&turn.id,"route",&serde_json::json!({"kind":"route","provider":"codex","model":"gpt-5.5","reason":"x","mode":"build","agent":"developer"})).expect("route de novo");
        store.append_exchange(&chat.id,&turn.id,"pedido","resposta").expect("exchange");
        store.set_turn_status(&turn.id,TurnStatus::Answered).expect("fechado");

        let turns=store.snapshot().expect("snapshot").chats[0].turns.clone();
        let route=|id:&str|turns.iter().find(|view|view.id==id).and_then(|view|view.route.clone());
        assert_eq!(route(&turn.id),Some(turns::TurnRoute{provider:"codex".into(),model:"gpt-5.5".into(),mode:Some("build".into()),agent:Some("developer".into()),switched:None}));
        assert_eq!(route(&old.id).map(|route|(route.model,route.mode)),Some(("sonnet".into(),None)));
    }

    #[test]
    fn a_folder_belongs_to_a_single_project() {
        let root=tempfile::tempdir().expect("root");
        let mut store=store(&root);
        let folder=root.path().join("app");
        fs::create_dir_all(folder.join("src")).expect("pasta");
        let path=folder.display().to_string();
        store.create_project("App",Some(path.clone())).expect("primeiro projeto");

        for same in [path.clone(),format!("{path}/"),format!("  {path}  "),folder.join("src").join("..").display().to_string()] {
            let error=store.create_project("Outro",Some(same.clone())).expect_err("a mesma pasta");
            assert_eq!(error.downcast_ref::<Text>().map(|text|text.key.as_str()),Some("project.pathTaken"),"{same}");
        }
        assert_eq!(store.snapshot().expect("snapshot").projects.len(),1,"nenhum projeto a mais");
        store.create_project("Fonte",Some(folder.join("src").display().to_string())).expect("subpasta é outra pasta");
        store.create_project("Rascunho",None).expect("sem pasta");
        store.create_project("Outro rascunho",Some("  ".into())).expect("pasta em branco é sem pasta");
    }

    #[test]
    fn only_a_chat_without_history_is_worth_a_model_call() {
        let root=tempfile::tempdir().expect("root");
        let mut store=store(&root);
        let project=store.create_project("Produto",None).expect("project");
        let chat=store.create_chat(&project.id,None).expect("chat");

        let turn=store.open_turn(&chat.id).expect("turno");

        assert!(store.chat_is_unnamed(&chat.id).expect("chat novo"),"um chat recém-criado ainda não tem nome");
        store.append_exchange(&chat.id,&turn.id,"Preciso revisar o cálculo do frete no checkout","Vamos olhar o cálculo.").expect("exchange");
        assert_eq!(store.snapshot().expect("snapshot").chats[0].title,"Preciso revisar o cálculo do frete no…","o resumo local entra na hora");
        assert!(store.chat_is_unnamed(&chat.id).expect("chat em uso"),"o resumo local é provisório: o modelo ainda tem direito a um nome");

        store.rename_chat(&chat.id,"Cálculo do frete").expect("batismo");
        assert!(!store.chat_is_unnamed(&chat.id).expect("chat batizado"),"batizado uma vez, nunca mais se gasta modelo com isso");
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
    fn a_chat_keeps_the_work_mode_it_was_given() {
        let root=tempfile::tempdir().expect("root");
        let mut store=store(&root);
        let project=store.create_project("Produto",None).expect("project");
        let chat=store.create_chat(&project.id,None).expect("chat");
        assert_eq!((chat.work_mode.as_str(),store.work_mode(&chat.id).expect("modo").as_str()),("auto","auto"));
        store.set_work_mode(&chat.id,"plan").expect("plan");
        assert_eq!(store.snapshot().expect("snapshot").chats[0].work_mode,"plan");
        assert!(store.set_work_mode(&chat.id,"sprint").is_err(),"só os três modos");
        assert!(store.set_work_mode("chat-que-não-existe","build").is_err());
        assert_eq!(store.work_mode(&chat.id).expect("modo"),"plan");
    }


    #[test] fn the_picker_grants_last_for_the_whole_chat_until_cleared() {
        let root=tempfile::tempdir().expect("tempdir");
        let mut store=store(&root);
        let project=store.create_project("p",None).expect("projeto");
        let (chat,other)=(store.create_chat(&project.id,None).expect("chat"),store.create_chat(&project.id,None).expect("outro"));
        assert!(store.chat_grants(&chat.id).expect("lê").is_empty());
        let grants=crate::llm::Grants{git:true,commands:vec!["npm test".into()],..Default::default()};
        store.set_chat_grants(&chat.id,&grants).expect("grava");
        store.enqueue_prompt(&chat.id,"um",None).expect("turno");
        store.enqueue_prompt(&chat.id,"dois",None).expect("turno");
        assert_eq!(store.chat_grants(&chat.id).expect("lê"),grants,"vale para os pedidos seguintes também");
        assert!(store.chat_grants(&other.id).expect("lê").is_empty(),"cada chat tem as suas");
        store.set_chat_grants(&chat.id,&crate::llm::Grants::default()).expect("limpa");
        assert!(store.chat_grants(&chat.id).expect("lê").is_empty());
        assert!(store.set_chat_grants("chat-que-não-existe",&grants).is_err());
    }

    #[test] fn grants_stay_with_the_turn_and_always_allowed_commands_with_the_project() {
        let root=tempfile::tempdir().expect("tempdir");
        let mut store=store(&root);
        let project=store.create_project("p",None).expect("projeto");
        let chat=store.create_chat(&project.id,None).expect("chat");
        let turn=store.enqueue_prompt(&chat.id,"rode os testes",None).expect("turno");
        assert!(store.turn_grants(&turn.id).expect("lê").is_empty());
        let grants=crate::llm::Grants{git:true,commands:vec!["git add -A".into()],..Default::default()};
        store.set_turn_grants(&turn.id,&grants).expect("grava");
        assert_eq!(store.turn_grants(&turn.id).expect("lê"),grants);
        store.allow_command(&chat.id,"git add").expect("sempre");
        store.allow_command(&chat.id,"git add").expect("de novo, sem duplicar");
        assert_eq!(store.allowed_commands(&chat.id).expect("lista"),vec!["git add".to_string()]);
        store.forget_allowed_command(&chat.id,"git add").expect("tira");
        assert!(store.allowed_commands(&chat.id).expect("lista").is_empty());
    }
}
