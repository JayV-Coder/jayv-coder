#!/bin/bash

# Script para executar o Jev AI Orchestrator com Poetry

# Verifica se o Poetry está instalado
if ! command -v poetry &> /dev/null; then
    echo "Poetry não encontrado. Por favor, instale o Poetry primeiro."
    exit 1
fi

# Verifica se estamos no diretório correto
if [ ! -f "pyproject.toml" ]; then
    echo "Este script deve ser executado no diretório raiz do projeto."
    exit 1
fi

# Ativa o ambiente virtual do Poetry
poetry env use python3.14

# Instala as dependências
echo "Instalando dependências..."
poetry install

# Executa o projeto
echo "Executando o Jev AI Orchestrator..."
poetry run jev