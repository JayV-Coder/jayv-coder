use anyhow::{Context,Result};
use clap::{Parser,Subcommand};
use jev_orchestrator::{config::Config,orchestrator::Orchestrator,run_desktop};
use std::path::PathBuf;

#[derive(Debug,Parser)]
#[command(name="jev",version,about="Jev AI Orchestrator — Tauri desktop and Rust CLI")]
struct Cli { #[arg(short,long,global=true)] config:Option<PathBuf>, #[arg(long,global=true,default_value=".")] root:PathBuf, #[command(subcommand)] command:Option<Commands> }
#[derive(Debug,Subcommand)]
enum Commands { Status, Run { #[arg(required=true,num_args=1..)] task:Vec<String> }, Index, Version }

fn main()->Result<()> {
    let cli=Cli::parse(); let config_path=Config::discover(cli.config); let root=cli.root.canonicalize().context("workspace root does not exist")?;
    match cli.command {
        None=>run_desktop(config_path,root),
        Some(Commands::Status)=>{let orchestrator=Orchestrator::new(config_path.clone(),root)?;println!("Jev AI Orchestrator {}",env!("CARGO_PKG_VERSION"));println!("Status: operational");println!("Configuration: {}",config_path.display());println!("Providers: {}",orchestrator.executable_provider_count());println!("Models: {}",orchestrator.executable_model_count());println!("Indexed files: {}",orchestrator.rag.len());Ok(())},
        Some(Commands::Index)=>{let orchestrator=Orchestrator::new(config_path,root)?;println!("Indexed {} files",orchestrator.rag.len());Ok(())},
        Some(Commands::Version)=>{println!("Jev AI Orchestrator v{}",env!("CARGO_PKG_VERSION"));Ok(())},
        Some(Commands::Run{task})=>{let runtime=tokio::runtime::Runtime::new()?;runtime.block_on(async move {let mut orchestrator=Orchestrator::new(config_path,root)?;let result=orchestrator.process(&task.join(" "),Some("cli"),&jev_orchestrator::progress::Pulse::silent()).await;if let Some(response)=result.result{println!("{}",response.response);Ok(())}else{Err(anyhow::anyhow!(result.error.unwrap_or_else(||"task failed".into())))}})},
    }
}
