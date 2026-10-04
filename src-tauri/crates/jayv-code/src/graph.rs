use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all="snake_case")]
pub enum TaskStatus { Pending, Running, Completed, Failed, Skipped }
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all="snake_case")]
pub enum TaskType { Analysis, CodeGeneration, Testing, Review, Integration, Documentation, Custom }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskNode {
    pub task_id: String, pub task_type: TaskType, pub description: String, pub status: TaskStatus,
    pub dependencies: Vec<String>, pub result: Option<Value>, pub error: Option<String>,
}
impl TaskNode { pub fn new(task_type: TaskType, description: impl Into<String>) -> Self { Self { task_id: Uuid::new_v4().to_string(), task_type, description: description.into(), status: TaskStatus::Pending, dependencies: vec![], result: None, error: None } } }

#[derive(Debug, Default)]
pub struct ExecutionGraph { pub tasks: HashMap<String, TaskNode> }
impl ExecutionGraph {
    pub fn add_task(&mut self, task: TaskNode) -> String { let id = task.task_id.clone(); self.tasks.insert(id.clone(), task); id }
    pub fn add_dependency(&mut self, task_id: &str, dependency_id: &str) -> Result<(), String> {
        if !self.tasks.contains_key(dependency_id) { return Err(format!("unknown dependency: {dependency_id}")); }
        self.tasks.get_mut(task_id).ok_or_else(|| format!("unknown task: {task_id}"))?.dependencies.push(dependency_id.into());
        if !self.validate() { self.tasks.get_mut(task_id).unwrap().dependencies.retain(|id| id != dependency_id); return Err("dependency creates a cycle".into()); }
        Ok(())
    }
    pub fn ready_tasks(&self) -> Vec<&TaskNode> { self.tasks.values().filter(|task| task.status == TaskStatus::Pending && task.dependencies.iter().all(|id| self.tasks.get(id).is_some_and(|d| d.status == TaskStatus::Completed))).collect() }
    pub fn mark_started(&mut self, id: &str) -> Result<(), String> { self.set_status(id, TaskStatus::Running, None, None) }
    pub fn mark_completed(&mut self, id: &str, result: Value) -> Result<(), String> { self.set_status(id, TaskStatus::Completed, Some(result), None) }
    pub fn mark_failed(&mut self, id: &str, error: String) -> Result<(), String> { self.set_status(id, TaskStatus::Failed, None, Some(error)) }
    fn set_status(&mut self, id: &str, status: TaskStatus, result: Option<Value>, error: Option<String>) -> Result<(), String> { let task = self.tasks.get_mut(id).ok_or_else(|| format!("unknown task: {id}"))?; task.status=status; task.result=result; task.error=error; Ok(()) }
    pub fn validate(&self) -> bool { self.tasks.keys().all(|id| !self.cyclic(id, &mut HashSet::new(), &mut HashSet::new())) }
    fn cyclic(&self, id: &str, visiting: &mut HashSet<String>, visited: &mut HashSet<String>) -> bool { if visiting.contains(id) { return true; } if visited.contains(id) { return false; } visiting.insert(id.into()); if let Some(t) = self.tasks.get(id) { for d in &t.dependencies { if self.cyclic(d, visiting, visited) { return true; } } } visiting.remove(id); visited.insert(id.into()); false }
}

#[cfg(test)] mod tests { use super::*; #[test] fn respects_dependencies() { let mut g=ExecutionGraph::default(); let a=g.add_task(TaskNode::new(TaskType::Analysis,"a")); let b=g.add_task(TaskNode::new(TaskType::Testing,"b")); g.add_dependency(&b,&a).unwrap(); assert_eq!(g.ready_tasks().len(),1); g.mark_started(&a).unwrap(); g.mark_completed(&a,Value::Null).unwrap(); assert_eq!(g.ready_tasks()[0].task_id,b); } }
