// No Windows o executável é um app de janela: sem isto, abrir o JayV pelo menu
// abria junto um terminal vazio. Os subcomandos da CLI se prendem ao console
// de quem os chamou (`attach_parent_console`).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use anyhow::{Context,Result};
use clap::{Parser,Subcommand};
use jayv_lib::{config::Config,orchestrator::Orchestrator,run_desktop,workspace::{database_location,startup_log_location,WorkspaceStore}};
use std::{fs,io::Write,path::PathBuf};

/// O orquestrador com os agentes e modelos do banco, o mesmo que o aplicativo
/// de mesa usa.
fn orchestrator(config_path:PathBuf,root:PathBuf)->Result<Orchestrator> {
    let store=WorkspaceStore::open(database_location(&config_path,&root))?;
    let mut orchestrator=Orchestrator::new(config_path,root)?;
    orchestrator.use_llm(&store.llm_settings()?);
    Ok(orchestrator)
}

#[derive(Debug,Parser)]
#[command(name="jayv",version,about="JayV — Tauri desktop and Rust CLI")]
struct Cli { #[arg(short,long,global=true)] config:Option<PathBuf>, #[arg(long,global=true,default_value=".")] root:PathBuf, #[command(subcommand)] command:Option<Commands> }
#[derive(Debug,Subcommand)]
enum Commands {
    Status,
    Run { #[arg(required=true,num_args=1..)] task:Vec<String> },
    Index,
    Version,
    /// Runs a task file (YAML: a beginner's script and a verify command per task) through JayV and straight to the agent, each side in its own copy, and compares rounds until done and dollars spent.
    Bench { tasks:PathBuf },
    /// Serves the repository symbol index over MCP (stdio), for coding agents.
    Mcp,
}

/// Guarda o erro que impediu o aplicativo de mesa de abrir. Falhar aqui não
/// pode esconder o erro original, então qualquer problema de escrita é ignorado.
fn record_startup_error(error:&anyhow::Error) {
    let Some(path)=startup_log_location() else {return};
    if let Some(parent)=path.parent(){let _=fs::create_dir_all(parent);}
    if let Ok(mut file)=fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _=writeln!(file,"[{}] JayV {}: {error:#}",chrono::Local::now().to_rfc3339(),env!("CARGO_PKG_VERSION"));
    }
}

/// Os argumentos sem os links `jayv://`: eles são do plugin de deep link, que
/// os lê do processo e os entrega à janela já aberta.
fn parse<I,T>(args:I)->Result<Cli,clap::Error> where I:IntoIterator<Item=T>,T:Into<std::ffi::OsString>+Clone {
    Cli::try_parse_from(args.into_iter().map(Into::into).filter(|arg:&std::ffi::OsString|!arg.to_string_lossy().starts_with("jayv://")))
}

/// Devolve à CLI o console do terminal que a chamou, para `jayv status` e
/// companhia continuarem escrevendo onde o desenvolvedor lê.
#[cfg(windows)]
fn attach_parent_console() {
    #[link(name="kernel32")]
    unsafe extern "system" { fn AttachConsole(process:u32)->i32; }
    const ATTACH_PARENT_PROCESS:u32=u32::MAX;
    // SAFETY: chamada do Win32 sem ponteiros; falhar só significa que não há
    // console pai.
    unsafe { AttachConsole(ATTACH_PARENT_PROCESS); }
}

fn main()->Result<()> {
    #[cfg(windows)]
    // O `mcp` fala pelos canos que o agente abriu: nada de console por cima.
    if std::env::args_os().nth(1).is_none_or(|first|first!="mcp") && std::env::args_os().skip(1).any(|arg|!arg.to_string_lossy().starts_with("jayv://")) { attach_parent_console(); }
    let cli=parse(std::env::args_os()).unwrap_or_else(|error|error.exit());
    let desktop=cli.command.is_none();
    let result=run(cli);
    if desktop { if let Err(error)=&result { record_startup_error(error); } }
    result
}

fn run(cli:Cli)->Result<()> {
    let config_path=Config::discover(cli.config); let root=cli.root.canonicalize().context("workspace root does not exist")?;
    match cli.command {
        None=>run_desktop(config_path,root),
        Some(Commands::Status)=>{let orchestrator=orchestrator(config_path.clone(),root)?;println!("JayV {}",env!("CARGO_PKG_VERSION"));println!("Status: operational");println!("Configuration: {}",config_path.display());println!("Providers: {}",orchestrator.executable_provider_count());println!("Models: {}",orchestrator.executable_model_count());println!("Indexed files: {}",orchestrator.rag.len());Ok(())},
        Some(Commands::Index)=>{let orchestrator=Orchestrator::new(config_path,root)?;println!("Indexed {} files",orchestrator.rag.len());Ok(())},
        Some(Commands::Mcp)=>{let orchestrator=Orchestrator::new(config_path,root)?;jayv_lib::mcp::serve(orchestrator.rag,orchestrator.firewall)},
        Some(Commands::Version)=>{println!("JayV v{}",env!("CARGO_PKG_VERSION"));Ok(())},
        Some(Commands::Bench{tasks})=>{
            let (suite,project)=jayv_lib::bench::load(&tasks,&root)?;
            let runtime=tokio::runtime::Runtime::new()?;
            runtime.block_on(async move {let mut orchestrator=orchestrator(config_path,project.clone())?;let results=jayv_lib::bench::run(&mut orchestrator,&suite,&project).await?;println!("{}",jayv_lib::bench::report(&results));Ok(())})
        },
        Some(Commands::Run{task})=>{let runtime=tokio::runtime::Runtime::new()?;runtime.block_on(async move {let mut orchestrator=orchestrator(config_path,root)?;let result=orchestrator.process(&task.join(" "),Some("cli"),&jayv_lib::progress::Pulse::silent()).await;if let Some(response)=result.result{println!("{}",response.response);Ok(())}else{Err(anyhow::anyhow!(result.error.unwrap_or_else(||"task failed".into())))}})},
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// O sistema abre o JayV com o link do login (`jayv://auth/callback?code=…`)
    /// como argumento. Se o clap o lesse como subcomando, o processo morreria
    /// antes de repassar o link à janela aberta — e o login pelo GitHub não
    /// voltaria nunca.
    #[test] fn the_login_link_starts_the_desktop_app() {
        let cli=parse(["jayv","jayv://auth/callback?code=abc&state=x"]).expect("o link não é argumento do clap");
        assert!(cli.command.is_none());
        assert!(parse(["jayv"]).expect("sem nada").command.is_none());
        assert!(matches!(parse(["jayv","status"]).expect("subcomando").command,Some(Commands::Status)));
    }
}
