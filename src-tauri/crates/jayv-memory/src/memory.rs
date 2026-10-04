use crate::model::ChatMessage;
use serde_json::Value;
use std::collections::HashMap;

/// A sessão do agente que atende um chat. O pedido seguinte a retoma enquanto
/// for o mesmo agente, o mesmo modelo e a mesma pasta, e até `turns` chegar ao
/// teto: uma sessão longa demais volta a ficar cara, mesmo lida do cache.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentSession { pub provider: String, pub model: String, pub root: String, pub id: String, pub turns: usize }

#[derive(Debug, Default)]
pub struct MemoryManager {
    working: HashMap<String, Value>,
    session: HashMap<String, Value>,
    project: HashMap<String, Value>,
    semantic: HashMap<String, Value>,
    conversations: HashMap<String, Vec<ChatMessage>>,
    agent_sessions: HashMap<String, AgentSession>,
}

impl MemoryManager {
    pub fn store_working(&mut self, key: impl Into<String>, value: Value) { self.working.insert(key.into(), value); }
    pub fn retrieve_working(&self, key: &str) -> Option<&Value> { self.working.get(key) }
    pub fn store_session(&mut self, key: impl Into<String>, value: Value) { self.session.insert(key.into(), value); }
    pub fn retrieve_session(&self, key: &str) -> Option<&Value> { self.session.get(key) }
    pub fn store_project(&mut self, key: impl Into<String>, value: Value) { self.project.insert(key.into(), value); }
    pub fn retrieve_project(&self, key: &str) -> Option<&Value> { self.project.get(key) }
    pub fn store_semantic(&mut self, key: impl Into<String>, value: Value) { self.semantic.insert(key.into(), value); }
    pub fn retrieve_semantic(&self, key: &str) -> Option<&Value> { self.semantic.get(key) }
    pub fn add_message(&mut self, session_id: &str, role: impl Into<String>, content: impl Into<String>) {
        let conversation=self.conversations.entry(session_id.to_string()).or_default();
        conversation.push(ChatMessage { role: role.into(), content: content.into() });
        if conversation.len() > 40 { conversation.remove(0); }
    }
    pub fn conversation(&self, session_id: &str) -> &[ChatMessage] {
        self.conversations.get(session_id).map(Vec::as_slice).unwrap_or(&[])
    }
    pub fn set_conversation(&mut self, session_id: impl Into<String>, messages: Vec<ChatMessage>) {
        self.conversations.insert(session_id.into(),messages.into_iter().rev().take(40).collect::<Vec<_>>().into_iter().rev().collect());
    }
    pub fn clear_session(&mut self, session_id: &str) {
        self.working.clear();
        self.session.clear();
        self.conversations.remove(session_id);
        self.agent_sessions.remove(session_id);
    }
    pub fn agent_session(&self, session_id: &str) -> Option<&AgentSession> { self.agent_sessions.get(session_id) }
    pub fn keep_agent_session(&mut self, session_id: &str, session: AgentSession) { self.agent_sessions.insert(session_id.to_string(), session); }
    pub fn forget_agent_session(&mut self, session_id: &str) { self.agent_sessions.remove(session_id); }
    pub fn session_messages(&self) -> usize { self.conversations.values().map(Vec::len).sum() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversations_are_isolated_by_chat() {
        let mut memory=MemoryManager::default();
        memory.add_message("chat-a","user","A");
        memory.add_message("chat-b","user","B");

        assert_eq!(memory.conversation("chat-a")[0].content,"A");
        assert_eq!(memory.conversation("chat-b")[0].content,"B");
        memory.keep_agent_session("chat-a",AgentSession{provider:"claude".into(),model:"sonnet".into(),root:"/repo".into(),id:"s-1".into(),turns:1});
        memory.clear_session("chat-a");
        assert!(memory.agent_session("chat-a").is_none(),"limpar o chat esquece a sessão do agente");
        assert!(memory.conversation("chat-a").is_empty());
        assert_eq!(memory.session_messages(),1);
    }
}
