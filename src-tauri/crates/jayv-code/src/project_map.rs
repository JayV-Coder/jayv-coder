//! O mapa do projeto que vai ao agente no começo de cada sessão: as pastas,
//! os arquivos de que mais gente depende e os pontos de entrada.
//!
//! É a ideia do Cartographer sem o custo dele: em vez de subagentes lendo o
//! repositório inteiro para escrever um resumo, o mapa sai do índice de
//! símbolos, na máquina, em zero token. O agente recebe a planta antes de
//! sair explorando, e a sessão retomada já o tem — não vai de novo.

use crate::rag::RepositoryRag;
use std::collections::BTreeMap;

/// Abaixo disto o agente se acha sozinho mais barato do que lendo o mapa.
pub const MIN_FILES:usize=80;
const FOLDERS:usize=12;
const HUBS:usize=8;
const HUB_NAMES:usize=4;
const ENTRY_POINTS:usize=6;
/// O teto do mapa, em caracteres (~1.500 tokens).
const MAX_CHARS:usize=6_000;
const ENTRY_NAMES:[&str;12]=["main.rs","lib.rs","main.ts","main.tsx","index.ts","index.tsx","App.tsx","main.py","__main__.py","app.py","main.go","index.js"];
const HEADER:&str="PROJECT MAP, built locally from a symbol index of this repository. Use it to go straight to the right files instead of exploring.";

/// O mapa, ou nada quando o projeto é pequeno ou o índice não leu nenhum
/// arquivo de código.
pub fn render(rag:&RepositoryRag)->Option<String> {
    if rag.len()<MIN_FILES||rag.symbols().is_empty() { return None; }
    let mut sections=vec![HEADER.to_string()];
    let folders=folders(rag);
    if !folders.is_empty() { sections.push(format!("Folders (files):\n{}",folders.join("\n"))); }
    let hubs=rag.symbols().hubs(HUBS).into_iter().map(|(path,users)|{
        let names=rag.symbols().headline(path,HUB_NAMES);
        if names.is_empty() { format!("- {path} (used by {users})") } else { format!("- {path} (used by {users}): {}",names.join(", ")) }
    }).collect::<Vec<_>>();
    if !hubs.is_empty() { sections.push(format!("Central files (most used by others):\n{}",hubs.join("\n"))); }
    let entries=entry_points(rag);
    if !entries.is_empty() { sections.push(format!("Entry points:\n{}",entries.join("\n"))); }
    if sections.len()==1 { return None; }
    let map=sections.join("\n\n");
    Some(if map.chars().count()>MAX_CHARS { map.chars().take(MAX_CHARS).collect() } else { map })
}

/// As pastas de até dois níveis, das mais cheias para as mais vazias.
fn folders(rag:&RepositoryRag)->Vec<String> {
    let mut counts:BTreeMap<String,usize>=BTreeMap::new();
    for path in rag.paths() {
        let parts=path.split('/').collect::<Vec<_>>();
        if parts.len()<2 { continue; }
        let folder=parts[..parts.len().saturating_sub(1).min(2)].join("/");
        *counts.entry(folder).or_default()+=1;
    }
    let mut folders=counts.into_iter().collect::<Vec<_>>();
    folders.sort_by(|left,right|right.1.cmp(&left.1).then(left.0.cmp(&right.0)));
    folders.into_iter().take(FOLDERS).map(|(folder,count)|format!("- {folder}/ ({count})")).collect()
}

/// Os arquivos com nome de ponto de entrada, dos mais rasos para os mais fundos.
fn entry_points(rag:&RepositoryRag)->Vec<String> {
    let mut found=rag.paths().filter(|path|ENTRY_NAMES.contains(&path.rsplit('/').next().unwrap_or(path))).collect::<Vec<_>>();
    found.sort_by_key(|path|(path.matches('/').count(),*path));
    found.into_iter().take(ENTRY_POINTS).map(|path|format!("- {path}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn indexed(files:&[(String,String)])->(tempfile::TempDir,RepositoryRag) {
        let dir=tempfile::tempdir().expect("repositório");
        for (path,body) in files {
            let target=dir.path().join(path);
            std::fs::create_dir_all(target.parent().expect("pasta")).expect("pasta");
            std::fs::write(target,body).expect("arquivo");
        }
        let mut rag=RepositoryRag::new(dir.path().to_path_buf());
        rag.index(&crate::firewall::ContextFirewall::new(Default::default())).expect("índice");
        (dir,rag)
    }

    #[test]
    fn a_small_project_gets_no_map() {
        let (_dir,rag)=indexed(&[("src/main.rs".into(),"fn main() {}".into())]);
        assert!(render(&rag).is_none());
    }

    #[test]
    fn the_map_shows_folders_central_files_and_entry_points_within_its_ceiling() {
        let mut files=vec![("src/main.rs".to_string(),"fn main() { load_config(); }".to_string()),("src/config.rs".into(),"pub fn load_config() {}\npub struct Settings;".into())];
        for index in 0..MIN_FILES { files.push((format!("src/handlers/h{index}.rs"),format!("pub fn handle_{index}() {{ load_config(); }}"))); }
        let (_dir,rag)=indexed(&files);
        let map=render(&rag).expect("mapa");
        assert!(map.starts_with(HEADER));
        assert!(map.contains("- src/handlers/ (80)"),"{map}");
        assert!(map.contains("- src/config.rs (used by 81): Settings, load_config"),"{map}");
        assert!(map.contains("Entry points:\n- src/main.rs"),"{map}");
        assert!(map.chars().count()<=MAX_CHARS);
    }
}
