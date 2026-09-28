use anyhow::{anyhow,Context,Result};
use std::{collections::HashSet,fs,path::{Path,PathBuf},process::Stdio};
use tokio::{process::Command,time::{timeout,Duration}};
use uuid::Uuid;

#[derive(Debug,Clone)]
pub struct SandboxConfig { pub directory:PathBuf,pub timeout_seconds:u64,pub allowed_commands:HashSet<String>,pub network_access:bool }
impl Default for SandboxConfig { fn default()->Self{Self{directory:std::env::temp_dir().join("jev-sandbox"),timeout_seconds:30,allowed_commands:["cargo","npm","git","rg","echo"].into_iter().map(str::to_string).collect(),network_access:false}} }
pub struct AgentSandbox { config:SandboxConfig }
impl AgentSandbox {
    pub fn new(config:SandboxConfig)->Self{Self{config}}
    pub fn create_workspace(&self,task_id:&str)->Result<PathBuf>{let safe=task_id.chars().filter(|c|c.is_ascii_alphanumeric()||*c=='-'||*c=='_').collect::<String>();let path=self.config.directory.join(format!("{}-{}",safe,Uuid::new_v4()));fs::create_dir_all(&path)?;Ok(path)}
    pub fn destroy_workspace(&self,path:&Path)->Result<()> {let root=self.config.directory.canonicalize().context("sandbox root does not exist")?;let target=path.canonicalize().context("sandbox workspace does not exist")?;if !target.starts_with(&root)||target==root{return Err(anyhow!("refusing to remove path outside sandbox"));}fs::remove_dir_all(target)?;Ok(())}
    pub async fn execute(&self,command:&str,args:&[String],workspace:&Path)->Result<String>{if !self.config.allowed_commands.contains(command){return Err(anyhow!("command is not allowed in sandbox"));}if !workspace.starts_with(&self.config.directory){return Err(anyhow!("workspace is outside sandbox"));}let child=Command::new(command).args(args).current_dir(workspace).env("JEV_NETWORK_ACCESS",if self.config.network_access{"1"}else{"0"}).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true).spawn()?;let output=timeout(Duration::from_secs(self.config.timeout_seconds),child.wait_with_output()).await.map_err(|_|anyhow!("sandbox command timed out"))??;if !output.status.success(){return Err(anyhow!("sandbox command failed: {}",String::from_utf8_lossy(&output.stderr)));}Ok(String::from_utf8_lossy(&output.stdout).into_owned())}
}

#[cfg(test)]mod tests{use super::*;#[tokio::test]async fn runs_allowed_command(){let d=tempfile::tempdir().unwrap();let s=AgentSandbox::new(SandboxConfig{directory:d.path().into(),..Default::default()});let w=s.create_workspace("test").unwrap();assert_eq!(s.execute("echo",&["ok".into()],&w).await.unwrap().trim(),"ok");s.destroy_workspace(&w).unwrap();}}
