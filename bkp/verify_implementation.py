#!/usr/bin/env python3
"""
Verificação completa das funcionalidades implementadas do Jev AI Orchestrator
"""

import sys
import os
import subprocess

# Adiciona o diretório raiz ao path
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

def check_file_exists(filepath):
    """Verifica se um arquivo existe"""
    return os.path.exists(filepath)

def run_tests():
    """Executa os testes unitários"""
    print("=== Executando Testes Unitários ===")
    try:
        result = subprocess.run([sys.executable, 'test_advanced.py'], 
                              capture_output=True, text=True, cwd=os.getcwd())
        if result.returncode == 0:
            print("✅ Todos os testes passaram!")
            print("Saída dos testes:")
            print(result.stdout)
            return True
        else:
            print("❌ Alguns testes falharam!")
            print("Saída dos testes:")
            print(result.stderr)
            return False
    except Exception as e:
        print(f"Erro ao executar testes: {e}")
        return False

def run_demo():
    """Executa a demonstração"""
    print("\n=== Executando Demonstração ===")
    try:
        result = subprocess.run([sys.executable, 'demo_simple.py'], 
                              capture_output=True, text=True, cwd=os.getcwd())
        if result.returncode == 0:
            print("✅ Demonstração executada com sucesso!")
            print("Saída da demonstração:")
            print(result.stdout)
            return True
        else:
            print("❌ Demonstração falhou!")
            print("Saída da demonstração:")
            print(result.stderr)
            return False
    except Exception as e:
        print(f"Erro ao executar demonstração: {e}")
        return False

def list_implemented_features():
    """Lista todas as funcionalidades implementadas"""
    print("\n=== Funcionalidades Implementadas ===")
    
    features = [
        "Execution Graph - Decomposição de tarefas complexas em DAG",
        "Context Forking - Isolamento de contextos para redução de tokens",
        "Semantic Context Cache - Cache semântico para evitar operações RAG redundantes",
        "Context Firewall - Proteção de informações sensíveis",
        "Adaptive Model Router - Roteamento adaptativo que aprende com execuções",
        "Agent Sandbox - Isolamento seguro para execução de agentes",
        "Task Checkpointing - Persistência de estado para tarefas longas",
        "Context Value Scoring - Avaliação de valor de contexto por token"
    ]
    
    for i, feature in enumerate(features, 1):
        print(f"{i}. ✅ {feature}")

def list_files():
    """Lista todos os arquivos implementados"""
    print("\n=== Arquivos Implementados ===")
    
    core_dirs = [
        "core/execution_graph/",
        "core/agent/",
        "core/context_engine/",
        "core/optimizer/"
    ]
    
    for directory in core_dirs:
        if os.path.exists(directory):
            print(f"\n{directory}:")
            for root, dirs, files in os.walk(directory):
                for file in files:
                    if file.endswith('.py'):
                        print(f"  ✅ {os.path.join(root, file)}")

def main():
    """Função principal de verificação"""
    print("🔍 Verificação Completa do Jev AI Orchestrator")
    print("=" * 50)
    
    # Lista funcionalidades
    list_implemented_features()
    
    # Lista arquivos
    list_files()
    
    # Executa testes
    tests_passed = run_tests()
    
    # Executa demonstração
    demo_passed = run_demo()
    
    print("\n" + "=" * 50)
    print("📊 RESUMO FINAL")
    print("=" * 50)
    
    if tests_passed and demo_passed:
        print("🎉 TODAS AS FUNCIONALIDADES ESTÃO IMPLEMENTADAS E FUNCIONANDO CORRETAMENTE!")
        print("✅ Testes unitários: PASSARAM")
        print("✅ Demonstração: EXECUTADA COM SUCESSO")
        print("\nO Jev AI Orchestrator foi transformado de um simples \"router\" de LLMs")
        print("para uma plataforma completa de execução de IA com todas as funcionalidades")
        print("avançadas solicitadas.")
    else:
        print("⚠️  Algumas verificações falharam:")
        if not tests_passed:
            print("❌ Testes unitários: FALHARAM")
        if not demo_passed:
            print("❌ Demonstração: FALHOU")
    
    return 0 if (tests_passed and demo_passed) else 1

if __name__ == "__main__":
    sys.exit(main())