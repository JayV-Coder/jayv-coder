use crate::config::PermissionsConfig;
use anyhow::{anyhow,Context,Result};
use std::{fs,path::{Path,PathBuf},process::Stdio};
use tokio::{process::Command,time::{timeout,Duration}};

pub struct ToolRegistry { root:PathBuf, permissions:PermissionsConfig }
impl ToolRegistry {
    pub fn new(root:PathBuf,permissions:PermissionsConfig)->Self{Self{root,permissions}}
    fn resolve(&self,path:impl AsRef<Path>)->Result<PathBuf>{let path=path.as_ref();let joined=if path.is_absolute(){path.into()}else{self.root.join(path)};let parent=joined.parent().unwrap_or(&joined).canonicalize().with_context(||format!("invalid path: {}",joined.display()))?;let root=self.root.canonicalize()?;if !parent.starts_with(root){return Err(anyhow!("path escapes workspace"));}Ok(joined)}
    pub fn read(&self,path:impl AsRef<Path>)->Result<String>{if self.permissions.read!="allow"{return Err(anyhow!("read permission is {}",self.permissions.read));}Ok(fs::read_to_string(self.resolve(path)?)?)}
    pub fn write(&self,path:impl AsRef<Path>,content:&str,approved:bool)->Result<()> {if self.permissions.write=="deny"||self.permissions.write=="ask"&&!approved{return Err(anyhow!("write requires approval"));}let path=self.resolve(path)?;if let Some(parent)=path.parent(){fs::create_dir_all(parent)?;}fs::write(path,content)?;Ok(())}
    pub async fn command(&self,program:&str,args:&[String],approved:bool,seconds:u64)->Result<String>{if self.permissions.shell=="deny"||self.permissions.shell=="ask"&&!approved{return Err(anyhow!("shell execution requires approval"));}let child=Command::new(program).args(args).current_dir(&self.root).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true).spawn()?;let output=timeout(Duration::from_secs(seconds),child.wait_with_output()).await.map_err(|_|anyhow!("command timed out"))??;if !output.status.success(){return Err(anyhow!("command failed: {}",String::from_utf8_lossy(&output.stderr)));}Ok(String::from_utf8_lossy(&output.stdout).into_owned())}
}
