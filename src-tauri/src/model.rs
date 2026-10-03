use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentAnalysis {
    pub intent: String,
    pub scores: HashMap<String, usize>,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RoutingSignals {
    pub source: String,
    pub intent_confidence: f64,
    pub complexity_confidence: f64,
    pub confident: bool,
    pub needs_repository_context: Option<f64>,
    pub needs_tools: Option<f64>,
    pub is_destructive: Option<f64>,
    pub complexity_before_widening: Option<String>,
    pub jev_model: Option<String>,
    pub jev_input_tokens: u64,
    pub jev_output_tokens: u64,
    pub notice: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Context {
    pub system_instructions: String,
    pub project: ProjectInfo,
    pub relevant_files: Vec<String>,
    pub snippets: Vec<ContextSnippet>,
    pub estimated_tokens: usize,
    #[serde(default)] pub repository_context_skipped: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectInfo {
    pub root: String,
    pub name: String,
    pub languages: Vec<String>,
    /// Os repositórios dentro da raiz, quando ela junta vários (a pasta da
    /// organização): `api/ (github.com/acme/api)`.
    #[serde(default)]
    pub repositories: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSnippet {
    pub path: String,
    pub content: String,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelSelection {
    pub model_name: String,
    pub provider: String,
    pub estimated_tokens: usize,
    pub score: f64,
    pub reason: String,
    /// `plan` (o agente só lê e devolve um plano) ou `build` (o agente faz a
    /// mudança). Vazio quando nenhum agente foi chamado.
    #[serde(default)] pub mode: String,
    /// O papel que o Jev deu ao agente (`developer`, `reviewer`…), se algum.
    #[serde(default)] pub agent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderResponse {
    pub response: String,
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub model: String,
    pub provider: String,
    pub latency_ms: u128,
    /// A sessão do agente que atendeu, quando ele diz qual é: é ela que o
    /// pedido seguinte do mesmo chat retoma.
    #[serde(default)] pub session: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub model_provider: String,
    pub model_name: String,
    pub estimated_tokens: usize,
    pub context_files_count: usize,
    pub rag_files_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessResult {
    pub user_input: String,
    pub normalized_input: String,
    pub intent_analysis: IntentAnalysis,
    pub complexity: String,
    pub context_plan: Vec<String>,
    pub context: Context,
    pub strategy: String,
    pub model_selection: ModelSelection,
    pub result: Option<ProviderResponse>,
    pub validation: bool,
    pub decision: Decision,
    pub routing: RoutingSignals,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceRecord {
    pub task_type: String,
    pub strategy_used: String,
    pub model_used: String,
    pub success: bool,
    pub response_time_ms: u128,
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub estimated_cost: f64,
    pub timestamp: DateTime<Utc>,
}

pub type JsonMap = HashMap<String, Value>;
