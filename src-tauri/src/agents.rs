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
    pub fn find(&self, name: &str) -> Option<&Agent> { self.agents.iter().find(|agent| agent.name == name) }
    /// O papel que atende a intenção lida pelo Jev. Conversa geral não ganha
    /// papel: o pedido vai sem persona.
    pub fn for_intent(&self, intent: &str) -> Option<&Agent> {
        self.find(match intent { "code"|"refactor"|"test"=>"developer", "frontend"=>"frontend", "security"=>"security", "review"|"analysis"=>"reviewer", _=>return None })
    }
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn each_intent_gets_its_own_agent() {
        let registry=AgentRegistry::default();
        let named=|intent:&str|registry.for_intent(intent).map(|agent|agent.name.as_str());
        assert_eq!(named("security"),Some("security"));
        assert_eq!(named("code"),Some("developer"));
        assert_eq!(named("refactor"),Some("developer"));
        assert_eq!(named("test"),Some("developer"));
        assert_eq!(named("frontend"),Some("frontend"));
        assert_eq!(named("review"),Some("reviewer"));
        assert_eq!(named("analysis"),Some("reviewer"));
        assert_eq!(named("general"),None);
    }
}
