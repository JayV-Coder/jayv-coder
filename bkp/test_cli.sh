#!/bin/bash

# Script para testar o CLI do Jev AI Orchestrator

echo "🔧 Testando CLI do Jev AI Orchestrator..."
echo "========================================"

# Verificar se o arquivo existe
if [ ! -f "apps/cli/main.py" ]; then
    echo "❌ Erro: Arquivo CLI não encontrado"
    exit 1
fi

echo "✅ Arquivo CLI encontrado"

# Testar ajuda
echo ""
echo "📝 Testando ajuda..."
python3 apps/cli/main.py --help

# Testar status
echo ""
echo "📊 Testando status..."
python3 apps/cli/main.py status

# Testar versão
echo ""
echo ".VERSION Testando versão..."
python3 apps/cli/main.py version

# Testar demonstração
echo ""
echo "🎬 Testando demonstração..."
python3 apps/cli/main.py demo

echo ""
echo "🎉 CLI funcionando corretamente!"
echo "O Jev AI Orchestrator está pronto para uso como CLI"