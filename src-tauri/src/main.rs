use anyhow::{Context,Result};
use clap::{Parser,Subcommand};
use jayv_lib::{config::Config,orchestrator::Orchestrator,run_desktop,workspace::{database_location,WorkspaceStore}};
use std::path::PathBuf;

/// O orquestrador com os agentes e modelos do banco, o mesmo que o aplicativo
/// de mesa usa.
fn orchestrator(config_path:PathBuf,root:PathBuf)->Result<Orchestrator> {
    let store=WorkspaceStore::open(database_location(&config_path,&root),None)?;
    let mut orchestrator=Orchestrator::new(config_path,root)?;
    orchestrator.use_llm(&store.llm_settings()?);
    Ok(orchestrator)
}

#[derive(Debug,Parser)]
#[command(name="jayv",version,about="JayV — Tauri desktop and Rust CLI")]
struct Cli { #[arg(short,long,global=true)] config:Option<PathBuf>, #[arg(long,global=true,default_value=".")] root:PathBuf, #[command(subcommand)] command:Option<Commands> }
#[derive(Debug,Subcommand)]
enum Commands { Status, Run { #[arg(required=true,num_args=1..)] task:Vec<String> }, Index, Version }

fn main()->Result<()> {
    let cli=Cli::parse(); let config_path=Config::discover(cli.config); let root=cli.root.canonicalize().context("workspace root does not exist")?;
    match cli.command {
        None=>run_desktop(config_path,root),
        Some(Commands::Status)=>{let orchestrator=orchestrator(config_path.clone(),root)?;println!("JayV {}",env!("CARGO_PKG_VERSION"));println!("Status: operational");println!("Configuration: {}",config_path.display());println!("Providers: {}",orchestrator.executable_provider_count());println!("Models: {}",orchestrator.executable_model_count());println!("Indexed files: {}",orchestrator.rag.len());Ok(())},
        Some(Commands::Index)=>{let orchestrator=Orchestrator::new(config_path,root)?;println!("Indexed {} files",orchestrator.rag.len());Ok(())},
        Some(Commands::Version)=>{println!("JayV v{}",env!("CARGO_PKG_VERSION"));Ok(())},
        Some(Commands::Run{task})=>{let runtime=tokio::runtime::Runtime::new()?;runtime.block_on(async move {let mut orchestrator=orchestrator(config_path,root)?;let result=orchestrator.process(&task.join(" "),Some("cli"),&jayv_lib::progress::Pulse::silent()).await;if let Some(response)=result.result{println!("{}",response.response);Ok(())}else{Err(anyhow::anyhow!(result.error.unwrap_or_else(||"task failed".into())))}})},
    }
}
