#!/bin/bash

# Script para executar o Jev AI Orchestrator sem problemas de Poetry

echo "🔧 Iniciando execução do Jev AI Orchestrator..."
echo "================================================"

# Verificar se estamos no diretório certo
if [ ! -f "demo_simple.py" ]; then
    echo "❌ Erro: Não encontrei o arquivo demo_simple.py"
    echo "Por favor, execute este script no diretório correto do projeto"
    exit 1
fi

echo "✅ Diretório correto detectado"

# Verificar se o Python está disponível
if ! command -v python3 &> /dev/null; then
    echo "❌ Erro: Python3 não encontrado"
    exit 1
fi

echo "✅ Python3 disponível"

# Executar testes
echo ""
echo "🧪 Executando testes unitários..."
echo "------------------------------------------------"
python3 test_advanced.py

TEST_RESULT=$?
if [ $TEST_RESULT -eq 0 ]; then
    echo "✅ Todos os testes passaram!"
else
    echo "❌ Alguns testes falharam!"
    exit 1
fi

# Executar demonstração
echo ""
echo "🚀 Executando demonstração..."
echo "------------------------------------------------"
python3 demo_simple.py

DEMO_RESULT=$?
if [ $DEMO_RESULT -eq 0 ]; then
    echo "✅ Demonstração executada com sucesso!"
    echo ""
    echo "🎉 PARABÉNS! O Jev AI Orchestrator está funcionando perfeitamente!"
    echo "Todas as funcionalidades avançadas foram implementadas e testadas."
else
    echo "❌ Demonstração falhou!"
    exit 1
fi

echo ""
echo "📋 RESUMO DAS FUNCIONALIDADES IMPLEMENTADAS:"
echo "============================================"
echo "1. ✅ Execution Graph - Decomposição de tarefas complexas"
echo "2. ✅ Context Forking - Isolamento de contextos"
echo "3. ✅ Semantic Context Cache - Cache semântico"
echo "4. ✅ Context Firewall - Proteção de informações"
echo "5. ✅ Adaptive Model Router - Roteamento adaptativo"
echo "6. ✅ Agent Sandbox - Isolamento seguro"
echo "7. ✅ Task Checkpointing - Persistência de estado"
echo "8. ✅ Context Value Scoring - Avaliação de valor"
echo ""
echo "O projeto foi transformado de um simples \"router\" de LLMs"
echo "para uma plataforma completa de execução de IA."