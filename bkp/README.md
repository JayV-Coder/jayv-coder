# Jev AI Orchestrator

Terminal-based AI development environment with intelligent orchestration.

## Overview

Jev AI Orchestrator is a terminal-based AI development environment that intelligently routes user requests to the most appropriate AI models while minimizing token consumption, unnecessary context transmission, and expensive model usage.

## Key Features

- **Intelligent Orchestration**: Automatically decides the best way to process each request
- **Multi-Provider Support**: Works with OpenAI, Anthropic, local LLMs, and CLI tools
- **Context Optimization**: Uses RAG to send only relevant context
- **Cost Awareness**: Considers token costs and model pricing
- **Session Management**: Maintains conversation context efficiently
- **Extensible Architecture**: Easy to add new providers and capabilities

## Architecture

```
                         ┌──────────────────────────┐
                         │          USER            │
                         │        Terminal          │
                         └────────────┬─────────────┘
                                      │
                                      ▼
                         ┌──────────────────────────┐
                         │       Jev AI CLI         │
                         │   TUI / Chat / Commands  │
                         └────────────┬─────────────┘
                                      │
                                      ▼
                    ┌─────────────────────────────────┐
                    │       JEV ORCHESTRATOR          │
                    │                                 │
                    │  Intent Classification          │
                    │  Complexity Analysis            │
                    │  Task Planning                  │
                    │  Context Selection              │
                    │  Provider Selection             │
                    │  Model Selection                │
                    │  Token Budgeting                │
                    │  Tool Selection                 │
                    └───────┬──────────┬──────────────┘
                            │          │
                    ┌───────┘          └────────┐
                    ▼                           ▼
             ┌─────────────┐             ┌─────────────┐
             │     RAG     │             │    Tools    │
             │             │             │             │
             │ Repository  │             │ Filesystem  │
             │ Memory      │             │ Git         │
             │ Docs        │             │ Shell       │
             │ Symbols     │             │ Browser     │
             └──────┬──────┘             └──────┬──────┘
                    │                           │
                    └─────────────┬─────────────┘
                                  │
                                  ▼
                    ┌───────────────────────────┐
                    │      Model Router         │
                    └─────────────┬─────────────┘
                                  │
              ┌───────────────────┼────────────────────┐
              │                   │                    │
              ▼                   ▼                    ▼
        ┌───────────┐       ┌───────────┐       ┌───────────┐
        │ Providers │       │ Local LLM │       │ CLI Agent │
        │           │       │           │       │           │
        │ OpenAI    │       │ LM Studio │       │ Claude    │
        │ Anthropic │       │ Ollama    │       │ Codex     │
        │ OpenRouter│       │ vLLM      │       │ Others    │
        └───────────┘       └───────────┘       └───────────┘
```

## Installation

```bash
# Clone the repository
git clone https://github.com/jev-ai/jev-orchestrator.git
cd jev-orchestrator

# Install dependencies
pip install -r requirements.txt

# Install in development mode
pip install -e .
```

## Quick Start

```bash
# Run the CLI
jev
```

## Configuration

The configuration file `config.yaml` allows you to:

- Configure providers (OpenAI, Anthropic, local LLMs, CLI tools)
- Define models and their capabilities
- Set token budgets for different task complexities
- Control tool permissions

## Usage

Once running, you can:

1. Type natural language prompts in the terminal
2. Jev will automatically determine:
   - What context is needed
   - Which model to use
   - How to route the request
3. View the response in the terminal

## Development

### Project Structure

```
jev-orchestrator/
├── apps/
│   └── cli/
│       └── main.py          # Entry point
├── core/
│   ├── orchestrator.py      # Main orchestrator
│   ├── router.py            # Model routing
│   ├── context.py           # Context building
│   ├── memory.py            # Memory management
│   └── rag.py               # Repository RAG
├── providers/
│   ├── base.py              # Base provider class
│   ├── openai.py            # OpenAI provider
│   ├── anthropic.py         # Anthropic provider
│   ├── openai_compatible.py # OpenAI-compatible provider
│   └── cli.py               # CLI provider
├── infrastructure/
│   └── # Future home for database, vector store, etc.
└── ui/
    └── tui.py               # Terminal User Interface
```

## Contributing

We welcome contributions! Please see our contribution guidelines for more information.

## License

MIT License