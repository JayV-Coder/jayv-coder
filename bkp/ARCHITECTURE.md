# Jev AI Orchestrator - Arquitetura Avançada

## Visão Geral

Este projeto implementa uma arquitetura avançada para o Jev AI Orchestrator com base nas sugestões de evolução propostas. A implementação segue a evolução sugerida em etapas lógicas, começando pelas funcionalidades mais fundamentais e evoluindo para sistemas mais complexos e inteligentes.

## Estrutura de Componentes

```
Jev AI Orchestrator
├── core/
│   ├── execution_graph/          # Execution Graph
│   │   └── graph.py              # Estrutura do grafo de execução
│   ├── agent/                    # Agentes e executores
│   ├── context_engine/           # Motores de contexto
│   │   ├── context_fork.py       # Forking de contexto
│   │   ├── semantic_cache.py     # Cache semântico
│   │   └── context_firewall.py   # Firewall de contexto
│   ├── optimizer/                # Otimizadores e routers adaptativos
│   │   └── adaptive_router.py    # Router adaptativo
│   ├── orchestrator.py           # Motor principal de orquestração
│   └── ...
├── providers/                    # Provedores de IA
├── ui/                           # Interface de usuário
└── apps/                         # Aplicações
```

## Funcionalidades Implementadas

### 1. Execution Graph (Grafo de Execução)

Implementa a decomposição de tarefas complexas em um grafo direcionado acíclico (DAG) de tarefas independentes.

**Características:**
- Tarefas representadas como nós com dependências
- Suporte para execução paralela de tarefas independentes
- Gestão completa do ciclo de vida das tarefas (pendente, executando, concluída, falha)
- Validação de dependências e ciclos

### 2. Context Forking (Forking de Contexto)

Sistema que cria contextos isolados para cada tarefa, reduzindo o consumo de tokens.

**Características:**
- Contextos isolados para cada tarefa
- Limite de tokens configurável por fork
- Expiração automática de forks
- Sistema de cache para evitar contextos redundantes

### 3. Semantic Context Cache (Cache Semântico)

Cache que evita operações de RAG redundantes através de comparação semântica.

**Características:**
- Chave de cache baseada em múltiplos fatores (query, repo hash, commit, símbolos)
- TTL configurável
- Invalidação automática de entradas expiradas
- Estatísticas de cache

### 4. Context Firewall (Firewall de Contexto)

Sistema de proteção que previne informações sensíveis de serem enviadas para provedores externos.

**Características:**
- Regras de privacidade configuráveis
- Filtragem de arquivos sensíveis
- Redação de conteúdo potencialmente sensível
- Avaliação de segurança do contexto

### 5. Adaptive Model Router (Router Adaptativo)

Sistema que aprende com execuções anteriores para melhorar as decisões de roteamento.

**Características:**
- Aprendizado baseado em histórico de execuções
- Métricas de desempenho por modelo e provedor
- Confiança nas decisões de roteamento
- Recomendações de modelos baseadas em desempenho histórico

## Arquitetura de Evolução

A implementação segue a evolução proposta:

### Jev v0.1 - Fundamentos
- Provider Abstraction
- Provedores OpenAI, Anthropic, CLI
- Router básico
- RAG
- Telemetria de tokens

### Jev v0.2 - Contexto e Isolamento
- Builder de contexto
- Context Forking
- Cache semântico
- Skills e ferramentas sob demanda
- Firewall de contexto

### Jev v0.3 - Execução Paralela
- Execution Graph
- Planejador de tarefas
- Agentes paralelos
- Protocolo estruturado de agentes
- Armazenamento de artefatos
- Worktrees Git

### Jev v0.4 - Verificação e Segurança
- Validador de resultados
- Portaria de verificação
- Agente revisor
- Rollback automático
- Sandbox
- Motor de políticas

### Jev v0.5 - Aprendizado e Otimização
- Router adaptativo
- Memória de falhas
- Roteamento em sombra
- Testes A/B
- Benchmark
- Telemetria de roteamento

## Configuração Avançada

O projeto suporta configurações avançadas:

```yaml
jev:
  adaptive_routing:
    enabled: true
    learning_rate: 0.1
    confidence_threshold: 0.7

  context:
    fork_limit: 8000
    cache_ttl: 3600
    privacy_filtering: true

  execution_graph:
    max_parallel_tasks: 4
    timeout_seconds: 300

privacy:
  deny:
    - ".env"
    - "*.pem"
    - ".ssh/**"
  local_only:
    - "internal/**"
  redact_secrets: true
```

## Como Usar

### Executar Demonstração

```bash
python demo_advanced.py
```

### Executar Testes

```bash
python test_fixes.py
```

### Executar o Orquestrador

```bash
python apps/cli/main.py
```

## Benefícios das Novas Funcionalidades

1. **Redução de Tokens:** Context Forking e Semantic Cache reduzem significativamente o uso de tokens
2. **Segurança:** Context Firewall protege informações sensíveis
3. **Eficiência:** Adaptive Router aprende e melhora continuamente
4. **Escalabilidade:** Execution Graph permite execução paralela
5. **Manutenibilidade:** Estrutura modular e bem documentada
6. **Flexibilidade:** Configurações avançadas para diferentes cenários

## Próximos Passos

1. Implementar Agent Sandboxing
2. Integrar MCP Gateway
3. Adicionar suporte a Workflows como Código
4. Implementar sistema de autorização humano-em-loop
5. Desenvolver dashboard de estatísticas
6. Adicionar suporte a tarefas de fundo
7. Implementar sistema de checkpointing

Esta implementação transforma o Jev AI Orchestrator de um simples "router" de LLMs para uma **plataforma de execução de IA** que otimiza não apenas o custo e tempo, mas também a qualidade e segurança das execuções.