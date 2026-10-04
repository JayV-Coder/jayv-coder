//! As camadas do núcleo, de baixo para cima. Um módulo só usa os da própria
//! camada ou de camadas abaixo dela; é o que deixa cada camada virar um crate
//! do workspace sem ciclo (o Cargo recusa crate que dependa de quem depende
//! dele). Quem precisar de algo de uma camada de cima inverte a direção: o
//! tipo ou a constante desce, e a de cima reexporta.
//!
//! As camadas que já saíram para `crates/` (base e armazenamento) têm a
//! fronteira garantida pelo próprio Cargo; este teste cobre as que ainda
//! moram em `src/`. Por ora só o código conta; os testes ainda cruzam camadas
//! em alguns lugares e mudam junto quando cada crate sair.

/// Camada e os módulos dela, na ordem em que podem depender umas das outras.
pub const LAYERS:&[(&str,&[&str])]=&[
    ("base",&["i18n","config","model","lockdown","firewall","progress"]),
    ("store",&["local","cache","checkpoint"]),
    ("cloud",&["cloud"]),
    ("agents",&["llm","providers","agents","router","tools","sandbox","usage","bench"]),
    ("jev",&["jev","asking","gatekeeper","expertise","policy","core_settings"]),
    ("plans",&["features"]),
    ("orgs",&["checkout","repo_keys"]),
    ("code",&["rag","search","symbols","project_map","graph","context_engine"]),
    ("memory",&["memory","project_memory"]),
    ("workspace",&["workspace","turns"]),
    ("orchestration",&["orchestrator","parallel","split","review"]),
    ("live",&["live_files"]),
    ("app",&["desktop","mcp","sync","layers","outbox_tests"]),
];

#[cfg(test)]
mod tests {
    use super::LAYERS;
    use std::{fs, path::Path};

    fn layer_of(module:&str)->Option<usize> { LAYERS.iter().position(|(_,modules)|modules.contains(&module)) }

    fn sources(dir:&Path,found:&mut Vec<std::path::PathBuf>) {
        for entry in fs::read_dir(dir).expect("pasta do código").flatten() {
            let path=entry.path();
            if path.is_dir() {sources(&path,found);} else if path.extension().is_some_and(|ext|ext=="rs") {found.push(path);}
        }
    }

    /// O módulo de topo a que o arquivo pertence: `cloud/remote.rs` é `cloud`.
    fn module_of(root:&Path,file:&Path)->String {
        let first=file.strip_prefix(root).expect("dentro de src").components().next().expect("caminho").as_os_str().to_string_lossy().to_string();
        first.trim_end_matches(".rs").to_string()
    }

    #[test] fn every_module_has_a_layer() {
        let root=Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files=vec![]; sources(&root,&mut files);
        let missing:Vec<String>=files.iter().map(|file|module_of(&root,file)).filter(|module|module!="lib"&&module!="main"&&layer_of(module).is_none()).collect();
        assert!(missing.is_empty(),"módulo sem camada em layers.rs: {missing:?}");
    }

    #[test] fn no_module_reaches_a_layer_above_its_own() {
        let root=Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files=vec![]; sources(&root,&mut files);
        let mut crossings=vec![];
        for file in &files {
            let module=module_of(&root,file);
            let Some(own)=layer_of(&module) else {continue};
            let source=fs::read_to_string(file).expect("arquivo");
            let code=source.split("#[cfg(test)]").next().unwrap_or_default();
            for (number,line) in code.lines().enumerate() {
                for used in line.split("crate::").skip(1).map(|rest|rest.split(|c:char|!(c.is_alphanumeric()||c=='_')).next().unwrap_or_default()) {
                    if let Some(theirs)=layer_of(used) && theirs>own {
                        crossings.push(format!("{}:{} {module} ({}) → {used} ({})",file.strip_prefix(&root).unwrap().display(),number+1,LAYERS[own].0,LAYERS[theirs].0));
                    }
                }
            }
        }
        assert!(crossings.is_empty(),"camada de baixo usando a de cima:\n{}",crossings.join("\n"));
    }
}
