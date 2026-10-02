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
}

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
        ensure_message_turns(&connection)?;
        ensure_chat_named(&connection)?;
        ensure_turn_partial(&connection)?;
        ensure_turn_local(&connection)?;
        ensure_message_uid(&connection)?;
        ensure_project_repo_keys(&connection)?;
        crate::usage::store::ensure(&connection)?;
        connection.execute_batch(crate::expertise::SCHEMA)?;
        connection.execute_batch(crate::policy::SCHEMA)?;
        crate::local::outbox::install(&connection)?;
        turns::requeue_interrupted_turns(&connection)?;
        let mut store=Self{connection,path};
        store.ensure_chat_codes()?;
        // Os remotes mudam fora do app (um `git remote add`): a abertura
        // confere de novo. Pasta que sumiu só deixa a lista como estava.
        if let Err(error)=store.refresh_repo_keys() {eprintln!("repo keys: {error:#}");}
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
            chats.push(ChatRecord{id:id.clone(),code,project_id,title,messages:self.messages(&id)?,turns:turns::views_for_chat(&self.connection,&id)?,question:turns::pending_question(&self.connection,&id)?,created_at:parse_time(&created_at)?,updated_at:parse_time(&updated_at)?});
        }
        Ok(WorkspaceData{projects,chats})
    }

    pub fn create_project(&mut self, name: &str, root_path: Option<String>) -> Result<ProjectRecord> {
        let name=name.trim();
        anyhow::ensure!(!name.is_empty(),Text::new("project.nameRequired"));
        let root_path=root_path.map(|path|path.trim().to_string()).unwrap_or_default();
        // Uma pasta é de um projeto só: dois projetos nela dividiriam o índice,
        // o cache e a narração dos agentes sem que ninguém percebesse.
        if !root_path.is_empty() {
            let wanted=folder_key(&root_path);
            let mut statement=self.connection.prepare("SELECT name,root_path FROM projects WHERE trim(root_path)<>''")?;
            let folders=statement.query_map([],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
            if let Some((owner,_))=folders.into_iter().find(|(_,path)|folder_key(path)==wanted) { anyhow::bail!(Text::new("project.pathTaken").with("name",owner)); }
        }
        let project=ProjectRecord{id:Uuid::new_v4().to_string(),name:name.into(),root_path,created_at:Utc::now()};
        let keys=serde_json::to_string(&crate::repo_keys::of_folder(&project.root_path))?;
        self.connection.execute("INSERT INTO projects(id,name,root_path,created_at,repo_keys) VALUES(?1,?2,?3,?4,?5)",params![project.id,project.name,project.root_path,project.created_at.to_rfc3339(),keys])?;
        Ok(project)
    }

    /// Recalcula as chaves dos remotes de cada projeto com pasta e grava só o
    /// que mudou: cada gravação vira uma subida na fila.
    pub fn refresh_repo_keys(&mut self) -> Result<()> {
        let folders={
            let mut statement=self.connection.prepare("SELECT id,root_path,repo_keys FROM projects WHERE trim(root_path)<>''")?;
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
        let chat=ChatRecord{id:Uuid::new_v4().to_string(),code:self.unused_chat_code()?,project_id:project_id.into(),title,messages:vec![],turns:vec![],question:None,created_at:now,updated_at:now};
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
    pub fn claim_next_turn(&mut self) -> Result<Option<(Turn,String)>> {
        if turns::is_flying(&self.connection)? {return Ok(None);}
        let Some(turn)=turns::next_queued(&self.connection)? else {return Ok(None)};
        let prompt:String=self.connection.query_row(
            "SELECT content FROM messages WHERE turn_id=?1 AND role='user' ORDER BY created_at,id LIMIT 1",[&turn.id],|row|row.get(0),
        ).with_context(||format!("turn `{}` is queued without a written request",turn.id))?;
        turns::set_status(&self.connection,&turn.id,TurnStatus::Flying)?;
        Ok(Some((Turn{status:TurnStatus::Flying,..turn},prompt)))
    }

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

    pub fn clear_chat(&mut self, chat_id: &str) -> Result<()> {
        anyhow::ensure!(self.contains_chat(chat_id)?,Text::new("chat.notFound"));
        let transaction=self.connection.transaction()?;
        transaction.execute("DELETE FROM messages WHERE chat_id=?1",[chat_id])?;
        transaction.execute("UPDATE chats SET title='',named=0,updated_at=?1 WHERE id=?2",params![Utc::now().to_rfc3339(),chat_id])?;
        transaction.commit()?;
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

    pub fn set_turn_partial(&mut self, turn_id:&str, text:&str) -> Result<()> {turns::set_partial(&self.connection,turn_id,text)}

    pub fn clear_turn_partial(&mut self, turn_id:&str) -> Result<()> {turns::clear_partial(&self.connection,turn_id)}

    pub fn ask_question(&mut self, turn_id:&str, kind:&str, prompt:&str, options:&[String], source:&str) -> Result<()> {turns::ask(&self.connection,turn_id,kind,prompt,options,source)}

    pub fn pending_question(&self, chat_id:&str) -> Result<Option<QuestionView>> {turns::pending_question(&self.connection,chat_id)}

    pub fn question_of(&self, turn_id:&str) -> Result<Option<QuestionView>> {turns::question_of(&self.connection,turn_id)}

    pub fn settle_question(&mut self, turn_id:&str, status:&str, answered_by:Option<&str>) -> Result<bool> {turns::settle_question(&self.connection,turn_id,status,answered_by)}

    pub fn chat_of_turn(&self, turn_id:&str) -> Result<Option<String>> {turns::chat_of(&self.connection,turn_id)}

    pub fn question_origin(&self, turn_id:&str) -> Result<Option<String>> {turns::question_origin(&self.connection,turn_id)}

    /// Os agentes e modelos que a tela Configuração do LLM grava.
    pub fn llm_settings(&self) -> Result<crate::llm::LlmSettings> {crate::llm::load(&self.connection)}

    pub fn save_llm_settings(&mut self, settings:&crate::llm::LlmSettings) -> Result<crate::llm::LlmSettings> {crate::llm::save(&mut self.connection,settings)}

    pub fn core_settings(&self, defaults:&crate::core_settings::CoreSettings) -> Result<crate::core_settings::CoreSettings> {crate::core_settings::load(&self.connection,defaults)}

    pub fn save_core_settings(&mut self, settings:&crate::core_settings::CoreSettings) -> Result<crate::core_settings::CoreSettings> {crate::core_settings::save(&self.connection,settings)}

    pub fn database_path(&self) -> &Path {&self.path}

    pub fn expertise(&self) -> Result<crate::expertise::Expertise> {crate::expertise::load(&self.connection)}

    pub fn save_expertise(&mut self, level:&str) -> Result<crate::expertise::Expertise> {crate::expertise::save(&self.connection,level)}

    /// A conexão crua, para a fila de saída e a sincronização: é o único
    /// código de fora que fala SQL com o banco do usuário.
    pub fn record_usage(&self, entry:&crate::usage::Entry) -> Result<bool> {crate::usage::store::write(&self.connection,entry)}

    pub fn usage_report(&self, query:&crate::usage::store::Query) -> Result<crate::usage::store::Report> {crate::usage::store::report(&self.connection,query)}

    pub fn chat_usage(&self, chat_id:&str) -> Result<Vec<crate::usage::store::TurnUsage>> {crate::usage::store::chat_turns(&self.connection,chat_id)}

    pub fn connection(&self) -> &Connection {&self.connection}

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

pub(crate) fn parse_time(value:&str)->Result<DateTime<Utc>>{Ok(DateTime::parse_from_rfc3339(value).with_context(||format!("invalid timestamp `{value}`"))?.with_timezone(&Utc))}

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
        assert!(include_str!("../tauri.conf.json").contains(&format!("\"identifier\": \"{APP_IDENTIFIER}\"")),"a pasta de dados é a do identificador do aplicativo");
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
    fn the_queue_survives_closing_the_app() {
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
        let (turn,prompt)=store.claim_next_turn().expect("consulta").expect("o pedido interrompido voltou para a fila");

        assert_eq!(prompt,"não me perca");
        assert_eq!(turn.chat_id,chat_id);
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

        assert_eq!(store.question_origin(&answer_turn.id).expect("origem").as_deref(),Some("Troque o provedor padrão em config.yaml"));
        assert_eq!(store.chat_of_turn(&answer_turn.id).expect("chat").as_deref(),Some(chat.id.as_str()));
        assert!(store.question_origin(&question_turn.id).expect("origem").is_none(),"o pedido original não responde a pergunta nenhuma");
        assert!(store.snapshot().expect("snapshot").chats.into_iter().find(|item|item.id==chat.id).expect("chat").question.is_none(),"respondida não trava mais a caixa");
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
        assert_eq!(route(&turn.id),Some(turns::TurnRoute{provider:"codex".into(),model:"gpt-5.5".into(),mode:Some("build".into()),agent:Some("developer".into())}));
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

}
