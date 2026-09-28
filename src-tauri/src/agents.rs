use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent { pub name: String, pub capabilities: Vec<String>, pub system_prompt: String }

#[derive(Debug, Clone)]
pub struct AgentRegistry { agents: Vec<Agent> }
impl Default for AgentRegistry {
    fn default() -> Self { Self { agents: vec![
        Agent { name:"developer".into(), capabilities:vec!["coding".into(),"debugging".into(),"testing".into()], system_prompt:"Act as a senior software developer.".into() },
        Agent { name:"frontend".into(), capabilities:vec!["frontend".into(),"ui".into(),"ux".into()], system_prompt:"Act as a frontend and product interface specialist.".into() },
        Agent { name:"security".into(), capabilities:vec!["security".into(),"audit".into()], system_prompt:"Act as a defensive application security specialist.".into() },
        Agent { name:"reviewer".into(), capabilities:vec!["code-review".into(),"quality".into()], system_prompt:"Review correctness, risk, and maintainability.".into() },
    ] } }
}
impl AgentRegistry {
    pub fn list(&self) -> &[Agent] { &self.agents }
    pub fn select(&self, required: &[String]) -> Option<&Agent> { self.agents.iter().max_by_key(|agent| required.iter().filter(|r| agent.capabilities.contains(r)).count()) }
}

#[cfg(test)] mod tests { use super::*; #[test] fn selects_security_agent() { let r=AgentRegistry::default(); assert_eq!(r.select(&["security".into()]).unwrap().name,"security"); } }
