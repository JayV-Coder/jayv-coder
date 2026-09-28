# Jev AI Orchestrator - Configuração com Poetry

## Visão Geral

Este projeto foi configurado para ser gerenciado com Poetry, um gerenciador de dependências e ambientes virtuais para Python.

## Pré-requisitos

- Python 3.8+
- Poetry (instalado via `pip install poetry`)

## Configuração Inicial

### 1. Instalar dependências

```bash
# Para instalar as dependências principais
pip install openai anthropic

# Para instalar dependências de desenvolvimento (opcional)
pip install pytest black flake8 mypy
```

### 2. Configurar Poetry (opcional)

Se você desejar usar o Poetry para gerenciar o ambiente:

```bash
# Inicializar o projeto Poetry (já feito)
poetry init

# Instalar dependências
poetry install
```

## Estrutura do Projeto

```
jev-orchestrator/
├── apps/
│   └── cli/
│       └── main.py          # Ponto de entrada
├── core/
│   ├── orchestrator.py      # Motor de orquestração
│   ├── router.py            # Roteamento de modelos
│   ├── context.py           # Construção de contexto
│   ├── memory.py            # Gerenciamento de memória
│   ├── rag.py               # RAG (Retrieval Augmented Generation)
│   └── ...
├── providers/
│   ├── base.py              # Classe base de provedores
│   ├── openai.py            # Provedor OpenAI
│   ├── anthropic.py         # Provedor Anthropic
│   ├── openai_compatible.py # Provedor compatível com OpenAI
│   └── cli.py               # Provedor CLI
├── ui/
│   └── tui.py               # Interface de usuário em terminal
├── config.yaml              # Arquivo de configuração
└── pyproject.toml           # Configuração do Poetry
```

## Execução

### Executar o projeto diretamente:

```bash
python apps/cli/main.py
```

### Executar com o comando `jev` (se configurado):

```bash
jev
```

### Executar testes:

```bash
python test_fixes.py
```

## Scripts Disponíveis

- `run_with_poetry.sh` - Script para executar com Poetry
- `test_project.sh` - Script para testar o projeto

## Dependências Principais

- `openai`: Cliente para API OpenAI
- `anthropic`: Cliente para API Anthropic
- `pytest`: Framework de testes
- `black`: Formatter de código
- `flake8`: Linter de código

## Notas sobre a Implementação

As correções implementadas incluem:

1. **Correção da implementação do RAG**: Indexação real de arquivos do repositório
2. **Tratamento completo de erros**: Todos os métodos agora possuem tratamento de exceções
3. **Melhoria da análise de intenção**: Sistema mais robusto de detecção de intenções
4. **Sistema de cache mais robusto**: Cache com validade temporal e verificação de integridade
5. **Testes unitários**: Implementação de testes para validar as correções

## Comandos Úteis

```bash
# Verificar dependências
poetry show

# Atualizar dependências
poetry update

# Build do pacote
poetry build

# Publicar pacote (se configurado)
poetry publish
```