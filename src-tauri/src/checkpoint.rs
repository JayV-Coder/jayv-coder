use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fs, path::PathBuf};
use uuid::Uuid;

#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct TaskCheckpoint { pub checkpoint_id:String,pub task_id:String,pub state:Value,pub artifacts:Value,pub created_at:DateTime<Utc> }
pub struct CheckpointManager { directory:PathBuf }
impl CheckpointManager {
    pub fn new(path:impl Into<PathBuf>)->Self{Self{directory:path.into()}}
    pub fn create(&self,task_id:&str,state:Value,artifacts:Value)->Result<String>{fs::create_dir_all(&self.directory)?;let item=TaskCheckpoint{checkpoint_id:Uuid::new_v4().to_string(),task_id:task_id.into(),state,artifacts,created_at:Utc::now()};let path=self.directory.join(format!("{}.json",item.checkpoint_id));fs::write(path,serde_json::to_vec_pretty(&item)?)?;Ok(item.checkpoint_id)}
    pub fn restore(&self,id:&str)->Result<TaskCheckpoint>{let path=self.directory.join(format!("{id}.json"));let bytes=fs::read(&path).with_context(||format!("checkpoint not found: {}",path.display()))?;Ok(serde_json::from_slice(&bytes)?)}
    pub fn list(&self,task_id:Option<&str>)->Result<Vec<TaskCheckpoint>>{if !self.directory.exists(){return Ok(vec![]);}let mut items=fs::read_dir(&self.directory)?.filter_map(Result::ok).filter_map(|e|fs::read(e.path()).ok()).filter_map(|v|serde_json::from_slice::<TaskCheckpoint>(&v).ok()).filter(|c|task_id.is_none_or(|id|c.task_id==id)).collect::<Vec<_>>();items.sort_by_key(|c|std::cmp::Reverse(c.created_at));Ok(items)}
}

#[cfg(test)]mod tests{use super::*;#[test]fn round_trip(){let d=tempfile::tempdir().unwrap();let m=CheckpointManager::new(d.path());let id=m.create("t",serde_json::json!({"status":"running"}),Value::Null).unwrap();assert_eq!(m.restore(&id).unwrap().task_id,"t");}}
