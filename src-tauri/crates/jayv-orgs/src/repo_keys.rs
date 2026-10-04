//! As chaves dos repositórios de uma pasta, lidas do `.git/config`. É o que
//! liga um projeto local a um repositório de organização: só a chave
//! (`github.com/acme/api`) sobe para o Supabase, nunca a pasta.

use std::{fs, path::{Path, PathBuf}};

/// Os provedores que uma organização cadastra. Servidores próprios ficam de
/// fora por enquanto.
const HOSTS:[&str;3]=["github.com","gitlab.com","bitbucket.org"];

/// `host/caminho` em minúsculas e sem `.git`, ou nada quando a URL não é de um
/// dos provedores. Aceita `git@host:a/b.git`, `ssh://git@host[:porta]/a/b`,
/// `https://[usuário@]host/a/b` e `git://host/a/b`.
pub fn normalize(url:&str)->Option<String> {
    let url=url.trim();
    let (host,path)=if let Some((_,rest))=url.split_once("://") {
        let (authority,path)=rest.split_once('/')?;
        let host=authority.rsplit('@').next()?;
        (host.split(':').next()?,path)
    } else {
        // A forma curta do ssh: `[usuário@]host:caminho`.
        let (authority,path)=url.split_once(':')?;
        (authority.rsplit('@').next()?,path)
    };
    let host=host.to_ascii_lowercase();
    if !HOSTS.contains(&host.as_str()) {return None;}
    let path=path.trim_matches('/').to_lowercase();
    let path=path.strip_suffix(".git").unwrap_or(&path).trim_end_matches('/');
    let parts:Vec<&str>=path.split('/').filter(|part|!part.is_empty()).collect();
    if parts.len()<2 || parts.iter().any(|part|!part.chars().all(|c|c.is_ascii_alphanumeric()||"._-".contains(c))) {return None;}
    Some(format!("{host}/{}",parts.join("/")))
}

/// Os remotes de um `config` do git, na ordem em que aparecem: `(nome, url)`.
fn remotes(config:&str)->Vec<(String,String)> {
    let mut found=Vec::new();
    let mut current:Option<String>=None;
    for line in config.lines() {
        let line=line.trim();
        if line.starts_with('[') {
            current=line.strip_prefix("[remote \"").and_then(|rest|rest.strip_suffix("\"]")).map(str::to_string);
            continue;
        }
        let Some(name)=&current else {continue};
        if let Some((key,value))=line.split_once('=') {
            if key.trim().eq_ignore_ascii_case("url") {found.push((name.clone(),value.trim().to_string()));}
        }
    }
    found
}

/// A pasta do git de verdade: o `.git` pode ser um arquivo `gitdir: ...`
/// (worktree, submódulo), e a worktree guarda o `config` no `commondir`.
fn git_dir(root:&Path)->Option<PathBuf> {
    let dot=root.join(".git");
    let dir=if dot.is_dir() {dot} else {
        let text=fs::read_to_string(&dot).ok()?;
        let target=text.lines().find_map(|line|line.trim().strip_prefix("gitdir:"))?.trim();
        let target=Path::new(target);
        if target.is_absolute() {target.to_path_buf()} else {root.join(target)}
    };
    match fs::read_to_string(dir.join("commondir")) {
        Ok(common)=>{
            let common=Path::new(common.trim());
            Some(if common.is_absolute() {common.to_path_buf()} else {dir.join(common)})
        }
        Err(_)=>Some(dir),
    }
}

/// As chaves da pasta, `origin` primeiro e o resto pelo nome do remote, sem
/// repetir. Pasta sem git, ou sem remote de um provedor, dá lista vazia.
pub fn of_folder(root:&str)->Vec<String> {
    if root.trim().is_empty() {return vec![];}
    let Some(dir)=git_dir(Path::new(root.trim())) else {return vec![]};
    let Ok(config)=fs::read_to_string(dir.join("config")) else {return vec![]};
    let mut found=remotes(&config);
    found.sort_by(|(a,_),(b,_)|(a!="origin",a).cmp(&(b!="origin",b)));
    let mut keys:Vec<String>=Vec::new();
    for (_,url) in found {
        if let Some(key)=normalize(&url) {
            if !keys.contains(&key) {keys.push(key);}
        }
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn each_url_form_becomes_the_same_key() {
        for url in [
            "git@github.com:Acme/API.git",
            "https://github.com/acme/api",
            "https://user@github.com/acme/api.git/",
            "ssh://git@github.com:22/Acme/Api.git",
            "git://github.com/acme/api",
        ] {
            assert_eq!(normalize(url).as_deref(),Some("github.com/acme/api"),"{url}");
        }
    }

    #[test] fn gitlab_keeps_subgroups_and_bitbucket_its_host() {
        assert_eq!(normalize("git@gitlab.com:grupo/sub/app.git").as_deref(),Some("gitlab.com/grupo/sub/app"));
        assert_eq!(normalize("https://bitbucket.org/acme/web.git").as_deref(),Some("bitbucket.org/acme/web"));
    }

    #[test] fn other_hosts_and_broken_paths_are_left_out() {
        assert_eq!(normalize("git@git.empresa.local:acme/api.git"),None);
        assert_eq!(normalize("https://github.com/acme"),None);
        assert_eq!(normalize("/home/dev/repo.git"),None);
        assert_eq!(normalize("https://github.com/acme/ap i"),None);
    }

    fn folder_with(config:&str)->tempfile::TempDir {
        let dir=tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();
        fs::write(dir.path().join(".git/config"),config).unwrap();
        dir
    }

    #[test] fn origin_comes_first_then_the_rest_by_name_without_repeats() {
        let dir=folder_with(
            "[core]\n\tbare = false\n[remote \"upstream\"]\n\turl = git@github.com:acme/api.git\n\tfetch = +refs/heads/*:refs/remotes/upstream/*\n\
             [remote \"backup\"]\n\turl = https://gitlab.com/ana/api\n[remote \"origin\"]\n\turl = git@github.com:ana/api.git\n\
             [remote \"mirror\"]\n\turl = https://github.com/acme/api.git\n[branch \"main\"]\n\tremote = origin\n");
        assert_eq!(of_folder(dir.path().to_str().unwrap()),vec!["github.com/ana/api","gitlab.com/ana/api","github.com/acme/api"]);
    }

    #[test] fn a_worktree_reads_the_config_of_the_main_repository() {
        let main=folder_with("[remote \"origin\"]\n\turl = git@github.com:acme/api.git\n");
        let worktree_meta=main.path().join(".git/worktrees/feature");
        fs::create_dir_all(&worktree_meta).unwrap();
        fs::write(worktree_meta.join("commondir"),"../..\n").unwrap();
        let worktree=tempfile::tempdir().unwrap();
        fs::write(worktree.path().join(".git"),format!("gitdir: {}\n",worktree_meta.display())).unwrap();
        assert_eq!(of_folder(worktree.path().to_str().unwrap()),vec!["github.com/acme/api"]);
    }

    #[test] fn a_folder_without_git_has_no_keys() {
        let dir=tempfile::tempdir().unwrap();
        assert!(of_folder(dir.path().to_str().unwrap()).is_empty());
        assert!(of_folder("").is_empty());
    }
}
