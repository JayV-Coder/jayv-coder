//! As skills que o desenvolvedor instala no JayV: pastas com um `SKILL.md`
//! (o formato das skills do Claude: um cabeçalho com `name` e `description`
//! e, depois dele, as instruções), mais os arquivos que ela quiser trazer.
//!
//! Moram neste computador, como os servidores MCP: a pasta é copiada para a
//! pasta de dados do app e o banco local guarda o nome, a descrição e se está
//! ligada. Quem escolhe a skill de cada pedido é o Jev (`skill_choice`); aqui
//! só se instala, lista e lê.

use crate::i18n::Text;
use anyhow::{bail, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const SCHEMA:&str="
CREATE TABLE IF NOT EXISTS skills (
  name TEXT PRIMARY KEY,
  description TEXT NOT NULL,
  path TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1,
  installed_at TEXT NOT NULL
);";

/// O arquivo que define a skill, na raiz da pasta dela.
pub const FILE:&str="SKILL.md";
const NAME_MAX:usize=64;
const DESCRIPTION_MAX:usize=1024;
/// O que uma pasta de skill pode trazer: mais que isso não é skill, é projeto.
const FILES_MAX:usize=300;
const BYTES_MAX:u64=10*1024*1024;
/// Quanto das instruções vai ao modelo; o resto fica no arquivo, que ele lê.
pub const BODY_MAX_CHARS:usize=16_000;
const SKIPPED:[&str;3]=[".git","node_modules","target"];

/// Uma skill instalada. `path` é a pasta dela dentro da pasta de dados do app.
#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct Skill {
    pub name:String,
    pub description:String,
    pub path:String,
    pub enabled:bool,
    pub installed_at:String,
    /// O `slug` da organização que deu a skill; nulo na instalada aqui. A da
    /// organização não tem pasta: as instruções vêm em `inline`.
    #[serde(default)]
    pub origin:Option<String>,
    #[serde(skip)]
    pub inline:Option<String>,
}

/// O que o `SKILL.md` diz: o nome, a descrição e as instruções.
#[derive(Debug,Clone,PartialEq)]
pub struct Document { pub name:String, pub description:String, pub body:String }

/// A pasta onde as skills ficam, dentro da pasta de dados do app.
pub fn root(data_dir:&Path)->PathBuf { data_dir.join("skills") }

fn unquote(value:&str)->String {
    let value=value.trim();
    let quoted=value.len()>=2&&((value.starts_with('"')&&value.ends_with('"'))||(value.starts_with('\'')&&value.ends_with('\'')));
    if quoted { value[1..value.len()-1].to_string() } else { value.to_string() }
}

/// Lê o `SKILL.md`: um cabeçalho entre duas linhas `---` com `name` e
/// `description` (numa linha só, entre aspas, ou em bloco `>`/`|`) e, depois
/// dele, as instruções.
pub fn parse(text:&str)->Result<Document> {
    let text=text.trim_start_matches('\u{feff}').replace("\r\n","\n");
    let Some(rest)=text.trim_start().strip_prefix("---\n") else { bail!(Text::new("skills.invalid.header")) };
    let Some((header,body))=rest.split_once("\n---") else { bail!(Text::new("skills.invalid.header")) };
    let body=body.split_once('\n').map_or("",|(_,after)|after).trim().to_string();
    let (mut name,mut description)=(String::new(),String::new());
    let lines:Vec<&str>=header.lines().collect();
    let mut index=0;
    while index<lines.len() {
        let line=lines[index];
        index+=1;
        let Some((key,value))=line.split_once(':') else { continue };
        if line.starts_with(' ')||line.starts_with('\t') { continue; }
        let key=key.trim();
        if key!="name"&&key!="description" { continue; }
        let mut value=unquote(value);
        // Bloco `>` ou `|`: as linhas recuadas que vêm depois são o texto.
        if matches!(value.as_str(),">"|"|"|">-"|"|-"|">+"|"|+") {
            let mut parts=vec![];
            while index<lines.len()&&(lines[index].starts_with(' ')||lines[index].starts_with('\t')||lines[index].trim().is_empty()) { parts.push(lines[index].trim()); index+=1; }
            value=parts.into_iter().filter(|part|!part.is_empty()).collect::<Vec<_>>().join(" ");
        }
        if key=="name" { name=value; } else { description=value; }
    }
    let name=name.trim().to_string();
    if name.is_empty()||name.len()>NAME_MAX||!name.chars().all(|char|char.is_ascii_alphanumeric()||char=='-'||char=='_') { bail!(Text::new("skills.invalid.name")); }
    let description:String=description.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(DESCRIPTION_MAX).collect();
    if description.is_empty() { bail!(Text::new("skills.invalid.description")); }
    if body.is_empty() { bail!(Text::new("skills.invalid.body")); }
    Ok(Document{name,description,body})
}

/// As instruções da skill, prontas para o modelo: cortadas no limite, com o
/// aviso de que o resto está no arquivo.
pub fn read_body(skill:&Skill)->Result<String> {
    let body=match &skill.inline {
        Some(body)=>body.clone(),
        None=>parse(&std::fs::read_to_string(Path::new(&skill.path).join(FILE))?)?.body,
    };
    if body.chars().count()<=BODY_MAX_CHARS { return Ok(body); }
    let cut:String=body.chars().take(BODY_MAX_CHARS).collect();
    Ok(format!("{cut}\n[... cut: the rest of the instructions is in {FILE}]"))
}

/// As skills instaladas, em ordem de nome.
pub fn load(connection:&Connection)->Result<Vec<Skill>> {
    let mut statement=connection.prepare("SELECT name,description,path,enabled,installed_at FROM skills ORDER BY name")?;
    let rows=statement.query_map([],|row|Ok(Skill{name:row.get(0)?,description:row.get(1)?,path:row.get(2)?,enabled:row.get::<_,i64>(3)?!=0,installed_at:row.get(4)?,origin:None,inline:None}))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn record(connection:&Connection,document:&Document,path:&Path)->Result<Skill> {
    let enabled=connection.query_row("SELECT enabled FROM skills WHERE name=?1",[&document.name],|row|row.get::<_,i64>(0)).map_or(true,|value|value!=0);
    let installed_at=chrono::Utc::now().to_rfc3339();
    connection.execute("INSERT INTO skills(name,description,path,enabled,installed_at) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(name) DO UPDATE SET description=?2,path=?3,installed_at=?5",
        params![document.name,document.description,path.to_string_lossy(),enabled as i64,installed_at])?;
    Ok(Skill{name:document.name.clone(),description:document.description.clone(),path:path.to_string_lossy().into(),enabled,installed_at,origin:None,inline:None})
}

/// Copia a pasta da skill para `target`, sem seguir links e sem as pastas de
/// dependências, dentro do limite de arquivos e de tamanho.
fn copy_folder(source:&Path,target:&Path)->Result<()> {
    let (mut files,mut bytes)=(0usize,0u64);
    for entry in walkdir::WalkDir::new(source).follow_links(false).into_iter().filter_entry(|entry|entry.depth()==0||!SKIPPED.contains(&entry.file_name().to_string_lossy().as_ref())) {
        let entry=entry?;
        let relative=entry.path().strip_prefix(source)?;
        let destination=target.join(relative);
        if entry.file_type().is_dir() { std::fs::create_dir_all(&destination)?; continue; }
        if !entry.file_type().is_file() { continue; }
        files+=1;
        bytes+=entry.metadata()?.len();
        if files>FILES_MAX||bytes>BYTES_MAX { bail!(Text::new("skills.invalid.size")); }
        if let Some(parent)=destination.parent() { std::fs::create_dir_all(parent)?; }
        std::fs::copy(entry.path(),&destination)?;
    }
    Ok(())
}

/// As pastas de skill de `source`: ela mesma, quando tem `SKILL.md`, ou as
/// subpastas que têm um (uma pasta com várias skills).
fn skill_folders(source:&Path)->Result<Vec<PathBuf>> {
    let mut folders=vec![];
    if source.join(FILE).is_file() { folders.push(source.to_path_buf()); } else if source.is_dir() {
        let mut inside:Vec<PathBuf>=std::fs::read_dir(source)?.flatten().map(|entry|entry.path()).filter(|path|path.join(FILE).is_file()).collect();
        inside.sort();
        folders.extend(inside);
    }
    if folders.is_empty() { bail!(Text::new("skills.invalid.none")); }
    Ok(folders)
}

/// O que `install_folder` instalaria, sem copiar nem gravar nada: o
/// `SKILL.md` de cada skill da pasta, já conferido. A tela mostra a skill
/// na lista e só instala no Salvar das configurações.
pub fn preview_folder(source:&Path)->Result<Vec<Document>> {
    skill_folders(source)?.iter().map(|folder|parse(&std::fs::read_to_string(folder.join(FILE))?)).collect()
}

/// Instala uma skill a partir da pasta dela (com `SKILL.md`) ou de uma pasta
/// com várias skills (uma por subpasta). Uma skill com o mesmo nome é
/// substituída, mantendo se estava ligada. Devolve as que entraram.
pub fn install_folder(connection:&Connection,root:&Path,source:&Path)->Result<Vec<Skill>> {
    let folders=skill_folders(source)?;
    let mut installed=vec![];
    for folder in folders {
        let document=parse(&std::fs::read_to_string(folder.join(FILE))?)?;
        let target=root.join(&document.name);
        // Reinstalar a partir da própria cópia não tem o que copiar.
        if folder!=target {
            let staging=root.join(format!(".installing-{}",document.name));
            let _=std::fs::remove_dir_all(&staging);
            std::fs::create_dir_all(&staging)?;
            if let Err(error)=copy_folder(&folder,&staging) { let _=std::fs::remove_dir_all(&staging); return Err(error); }
            let _=std::fs::remove_dir_all(&target);
            std::fs::rename(&staging,&target)?;
        }
        installed.push(record(connection,&document,&target)?);
    }
    Ok(installed)
}

/// Instala uma skill a partir do texto do `SKILL.md` (colado na tela).
pub fn install_text(connection:&Connection,root:&Path,text:&str)->Result<Skill> {
    let document=parse(text)?;
    let target=root.join(&document.name);
    std::fs::create_dir_all(&target)?;
    std::fs::write(target.join(FILE),text)?;
    record(connection,&document,&target)
}

pub fn set_enabled(connection:&Connection,name:&str,enabled:bool)->Result<()> {
    connection.execute("UPDATE skills SET enabled=?2 WHERE name=?1",params![name,enabled as i64])?;
    Ok(())
}

/// Tira a skill do banco e apaga a pasta dela, só se estiver dentro da pasta
/// das skills do app.
pub fn remove(connection:&Connection,root:&Path,name:&str)->Result<()> {
    let path:Option<String>=connection.query_row("SELECT path FROM skills WHERE name=?1",[name],|row|row.get(0)).ok();
    connection.execute("DELETE FROM skills WHERE name=?1",[name])?;
    if let Some(path)=path { let path=PathBuf::from(path); if path.starts_with(root)&&path!=root { let _=std::fs::remove_dir_all(path); } }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE:&str="---\nname: release-notes\ndescription: Writes release notes from the merged pull requests.\n---\n# Release notes\n\nList each change.\n";

    fn database()->Connection { let connection=Connection::open_in_memory().expect("banco"); connection.execute_batch(SCHEMA).expect("esquema"); connection }

    #[test] fn the_header_gives_the_name_the_description_and_the_instructions() {
        let document=parse(SAMPLE).expect("skill");
        assert_eq!(document.name,"release-notes");
        assert_eq!(document.description,"Writes release notes from the merged pull requests.");
        assert!(document.body.starts_with("# Release notes"),"{}",document.body);
    }

    #[test] fn a_folded_or_quoted_description_is_read_as_one_line() {
        let folded=parse("---\r\nname: pdf\r\ndescription: >\r\n  Reads and fills\r\n  PDF forms.\r\n---\r\nBody\r\n").expect("skill");
        assert_eq!(folded.description,"Reads and fills PDF forms.");
        let quoted=parse("---\nname: \"pdf\"\ndescription: 'Use for PDFs: forms, text'\n---\nBody").expect("skill");
        assert_eq!((quoted.name.as_str(),quoted.description.as_str()),("pdf","Use for PDFs: forms, text"));
    }

    #[test] fn a_file_without_header_name_description_or_body_is_refused() {
        for text in ["no header","---\nname: a\n---\nbody","---\ndescription: d\n---\nbody","---\nname: bad name\ndescription: d\n---\nbody","---\nname: a\ndescription: d\n---\n"] {
            assert!(parse(text).is_err(),"{text}");
        }
    }

    #[test] fn installing_a_folder_copies_it_and_reinstalling_keeps_the_switch() {
        let (source,data)=(tempfile::tempdir().expect("origem"),tempfile::tempdir().expect("dados"));
        std::fs::write(source.path().join(FILE),SAMPLE).expect("skill");
        std::fs::create_dir_all(source.path().join("scripts")).expect("pasta");
        std::fs::write(source.path().join("scripts/run.sh"),"echo ok").expect("script");
        std::fs::create_dir_all(source.path().join("node_modules")).expect("pasta");
        std::fs::write(source.path().join("node_modules/x.js"),"x").expect("dependência");
        let connection=database();
        let root=root(data.path());
        std::fs::create_dir_all(&root).expect("raiz");
        let installed=install_folder(&connection,&root,source.path()).expect("instalada");
        assert_eq!(installed.len(),1);
        assert!(root.join("release-notes/scripts/run.sh").is_file());
        assert!(!root.join("release-notes/node_modules").exists(),"dependências não entram");
        set_enabled(&connection,"release-notes",false).expect("desligar");
        install_folder(&connection,&root,source.path()).expect("reinstalada");
        assert!(!load(&connection).expect("lista")[0].enabled,"reinstalar não religa");
        assert!(read_body(&load(&connection).expect("lista")[0]).expect("corpo").contains("List each change"));
    }

    #[test] fn a_folder_with_several_skills_installs_each_one_and_a_folder_with_none_is_refused() {
        let (source,data)=(tempfile::tempdir().expect("origem"),tempfile::tempdir().expect("dados"));
        for name in ["one","two"] {
            std::fs::create_dir_all(source.path().join(name)).expect("pasta");
            std::fs::write(source.path().join(name).join(FILE),format!("---\nname: {name}\ndescription: Does {name}.\n---\nSteps")).expect("skill");
        }
        let connection=database();
        let root=root(data.path());
        std::fs::create_dir_all(&root).expect("raiz");
        assert_eq!(install_folder(&connection,&root,source.path()).expect("instaladas").len(),2);
        let empty=tempfile::tempdir().expect("vazia");
        assert!(install_folder(&connection,&root,empty.path()).is_err());
    }

    #[test] fn previewing_a_folder_reads_each_skill_without_copying_or_recording() {
        let (source,data)=(tempfile::tempdir().expect("origem"),tempfile::tempdir().expect("dados"));
        for name in ["one","two"] {
            std::fs::create_dir_all(source.path().join(name)).expect("pasta");
            std::fs::write(source.path().join(name).join(FILE),format!("---\nname: {name}\ndescription: Does {name}.\n---\nSteps")).expect("skill");
        }
        let names:Vec<String>=preview_folder(source.path()).expect("prévia").into_iter().map(|document|document.name).collect();
        assert_eq!(names,["one","two"]);
        assert!(!root(data.path()).exists(),"a prévia não cria a pasta das skills");
        let empty=tempfile::tempdir().expect("vazia");
        assert!(preview_folder(empty.path()).is_err(),"pasta sem SKILL.md é recusada já na prévia");
        std::fs::write(source.path().join("one").join(FILE),"sem cabeçalho").expect("quebrada");
        assert!(preview_folder(source.path()).is_err(),"um SKILL.md inválido é recusado já na prévia");
    }

    #[test] fn pasted_text_is_installed_and_removing_deletes_only_inside_the_skills_folder() {
        let data=tempfile::tempdir().expect("dados");
        let root=root(data.path());
        std::fs::create_dir_all(&root).expect("raiz");
        let connection=database();
        let skill=install_text(&connection,&root,SAMPLE).expect("colada");
        assert!(Path::new(&skill.path).join(FILE).is_file());
        let outside=tempfile::tempdir().expect("fora");
        std::fs::write(outside.path().join("keep.txt"),"x").expect("arquivo");
        connection.execute("UPDATE skills SET path=?1",[outside.path().to_string_lossy()]).expect("troca");
        remove(&connection,&root,"release-notes").expect("remove");
        assert!(outside.path().join("keep.txt").exists(),"fora da pasta das skills nada é apagado");
        assert!(load(&connection).expect("lista").is_empty());
    }

    #[test] fn a_long_body_is_cut_for_the_model() {
        let data=tempfile::tempdir().expect("dados");
        let root=root(data.path());
        std::fs::create_dir_all(&root).expect("raiz");
        let connection=database();
        let text=format!("---\nname: big\ndescription: Big.\n---\n{}","a".repeat(BODY_MAX_CHARS+500));
        let skill=install_text(&connection,&root,&text).expect("grande");
        let body=read_body(&skill).expect("corpo");
        assert!(body.contains("[... cut:")&&body.chars().count()<BODY_MAX_CHARS+120);
    }
}
