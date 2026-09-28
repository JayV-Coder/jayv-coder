use crate::model::Context;
use chrono::{DateTime,Duration,Utc};
use serde::{Deserialize,Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct ContextFork { pub fork_id:String,pub task_id:String,pub parent_context_id:String,pub context:Context,pub token_limit:usize,pub created_at:DateTime<Utc>,pub expires_at:DateTime<Utc> }
#[derive(Debug,Default)]
pub struct ContextForkManager { forks:HashMap<String,ContextFork> }
impl ContextForkManager {
    pub fn create(&mut self,task_id:&str,parent:&str,mut context:Context,token_limit:usize)->String { context.estimated_tokens=context.estimated_tokens.min(token_limit);let id=Uuid::new_v4().to_string();let now=Utc::now();self.forks.insert(id.clone(),ContextFork{fork_id:id.clone(),task_id:task_id.into(),parent_context_id:parent.into(),context,token_limit,created_at:now,expires_at:now+Duration::hours(1)});id }
    pub fn get(&self,id:&str)->Option<&Context>{self.forks.get(id).filter(|f|f.expires_at>Utc::now()).map(|f|&f.context)}
    pub fn update(&mut self,id:&str,context:Context)->bool{if let Some(f)=self.forks.get_mut(id){f.context=context;f.context.estimated_tokens=f.context.estimated_tokens.min(f.token_limit);true}else{false}}
    pub fn expire(&mut self,id:&str)->bool{self.forks.remove(id).is_some()}
    pub fn active(&self)->Vec<&ContextFork>{let now=Utc::now();self.forks.values().filter(|f|f.expires_at>now).collect()}
}

#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct ContextFragment { pub id:String,pub content:String,pub relevance:f64,pub confidence:f64,pub freshness:f64,pub tokens:usize,pub dependency_importance:f64 }
impl ContextFragment { pub fn value(&self)->f64{(self.relevance*0.35+self.confidence*0.20+self.freshness*0.15+self.dependency_importance*0.30)/(self.tokens.max(1) as f64).sqrt()} }
pub fn rank_fragments(mut fragments:Vec<ContextFragment>)->Vec<ContextFragment>{fragments.sort_by(|a,b|b.value().total_cmp(&a.value()));fragments}
pub fn optimize_for_budget(fragments:Vec<ContextFragment>,budget:usize)->Vec<ContextFragment>{let mut used=0;rank_fragments(fragments).into_iter().filter(|f|{if used+f.tokens<=budget{used+=f.tokens;true}else{false}}).collect()}

#[cfg(test)]mod tests{use super::*;#[test]fn fork_lifecycle(){let mut m=ContextForkManager::default();let id=m.create("t","root",Context::default(),100);assert!(m.get(&id).is_some());assert!(m.expire(&id));}#[test]fn selects_high_value_context(){let a=ContextFragment{id:"a".into(),content:"".into(),relevance:0.9,confidence:0.9,freshness:0.8,tokens:100,dependency_importance:0.9};let b=ContextFragment{id:"b".into(),content:"".into(),relevance:0.1,confidence:0.1,freshness:0.1,tokens:200,dependency_importance:0.1};assert_eq!(optimize_for_budget(vec![b,a],100)[0].id,"a");}}
