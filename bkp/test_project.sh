#!/bin/bash

# Script para testar o Jev AI Orchestrator após as correções

echo "=== Testando Jev AI Orchestrator ==="

# Verifica se o Python está instalado
if ! command -v python3 &> /dev/null; then
    echo "Python não encontrado"
    exit 1
fi

# Testa a importação dos módulos principais
echo "Testando importação dos módulos..."

python3 -c "
import sys
sys.path.insert(0, '.')

# Testa importação do orchestrator
try:
    from core.orchestrator import JevOrchestrator
    print('✓ JevOrchestrator importado com sucesso')
except Exception as e:
    print(f'✗ Falha ao importar JevOrchestrator: {e}')

# Testa importação do RAG
try:
    from core.rag import RepositoryRAG
    print('✓ RepositoryRAG importado com sucesso')
except Exception as e:
    print(f'✗ Falha ao importar RepositoryRAG: {e}')

# Testa importação dos provedores
try:
    from providers.openai import OpenAIProvider
    from providers.anthropic import AnthropicProvider
    print('✓ Provedores importados com sucesso')
except Exception as e:
    print(f'✗ Falha ao importar provedores: {e}')

print('Teste de importação concluído!')
"

# Executa os testes unitários
echo ""
echo "Executando testes unitários..."
python3 test_fixes.py

echo ""
echo "=== Teste concluído ==="