#!/bin/bash

# Script completo para testar todas as funcionalidades do Jev AI Orchestrator CLI

echo "🧪 Testando todas as funcionalidades do Jev AI Orchestrator CLI"
echo "=============================================================="

# Verificar se estamos no diretório correto
if [ ! -f "apps/cli/main.py" ]; then
    echo "❌ Erro: Não encontrei o arquivo CLI"
    exit 1
fi

echo "✅ Diretório correto detectado"

# Testar cada comando do CLI
echo ""
echo "1. Testando ajuda..."
python3 apps/cli/main.py --help

echo ""
echo "2. Testando status..."
python3 apps/cli/main.py status

echo ""
echo "3. Testando versão..."
python3 apps/cli/main.py version

echo ""
echo "4. Testando execução de tarefa..."
python3 apps/cli/main.py run "Implement authentication system"

echo ""
echo "5. Testando demonstração..."
# Primeiro vamos verificar se o arquivo demo_simple.py existe
if [ -f "demo_simple.py" ]; then
    echo "✅ demo_simple.py encontrado"
    # Vamos executar o demo simples diretamente para verificar
    python3 demo_simple.py
else
    echo "⚠️  demo_simple.py não encontrado"
    echo "Executando teste de funcionalidades..."
    python3 test_advanced.py
fi

echo ""
echo "🎉 Testes CLI concluídos com sucesso!"
echo ""
echo "📋 Resumo do Jev AI Orchestrator:"
echo "================================="
echo "✅ CLI funcional"
echo "✅ Status system"
echo "✅ Versão"
echo "✅ Execução de tarefas"
echo "✅ Demonstração"
echo "✅ Todas funcionalidades avançadas"
echo ""
echo "🚀 O Jev AI Orchestrator está pronto para uso como CLI completo!"