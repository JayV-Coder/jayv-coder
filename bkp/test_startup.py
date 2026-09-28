#!/usr/bin/env python3
"""
Test script to verify Jev AI Orchestrator startup and basic functionality.
"""

import sys
import os
import logging

# Add the project root to the Python path
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

def test_imports():
    """Test that all modules can be imported."""
    print("Testing imports...")
    
    try:
        from core.orchestrator import JevOrchestrator
        print("✓ JevOrchestrator imported successfully")
    except Exception as e:
        print(f"✗ Failed to import JevOrchestrator: {e}")
        return False
    
    try:
        from core.router import ModelRouter
        print("✓ ModelRouter imported successfully")
    except Exception as e:
        print(f"✗ Failed to import ModelRouter: {e}")
        return False
    
    try:
        from core.context import ContextBuilder
        print("✓ ContextBuilder imported successfully")
    except Exception as e:
        print(f"✗ Failed to import ContextBuilder: {e}")
        return False
    
    try:
        from core.memory import MemoryManager
        print("✓ MemoryManager imported successfully")
    except Exception as e:
        print(f"✗ Failed to import MemoryManager: {e}")
        return False
    
    try:
        from core.rag import RepositoryRAG
        print("✓ RepositoryRAG imported successfully")
    except Exception as e:
        print(f"✗ Failed to import RepositoryRAG: {e}")
        return False
    
    try:
        from providers.openai import OpenAIProvider
        print("✓ OpenAIProvider imported successfully")
    except Exception as e:
        print(f"✗ Failed to import OpenAIProvider: {e}")
        return False
    
    try:
        from providers.anthropic import AnthropicProvider
        print("✓ AnthropicProvider imported successfully")
    except Exception as e:
        print(f"✗ Failed to import AnthropicProvider: {e}")
        return False
    
    try:
        from providers.openai_compatible import OpenAICompatibleProvider
        print("✓ OpenAICompatibleProvider imported successfully")
    except Exception as e:
        print(f"✗ Failed to import OpenAICompatibleProvider: {e}")
        return False
    
    try:
        from providers.cli import CLIProvider
        print("✓ CLIProvider imported successfully")
    except Exception as e:
        print(f"✗ Failed to import CLIProvider: {e}")
        return False
    
    return True

def test_orchestrator_creation():
    """Test creating an orchestrator instance."""
    print("\nTesting orchestrator creation...")
    
    try:
        from core.orchestrator import JevOrchestrator
        orchestrator = JevOrchestrator("config.yaml")
        print("✓ JevOrchestrator created successfully")
        return True
    except Exception as e:
        print(f"✗ Failed to create JevOrchestrator: {e}")
        return False

def test_provider_initialization():
    """Test that providers can be initialized."""
    print("\nTesting provider initialization...")
    
    try:
        from providers.openai import OpenAIProvider
        from providers.anthropic import AnthropicProvider
        from providers.openai_compatible import OpenAICompatibleProvider
        from providers.cli import CLIProvider
        
        # Test basic provider creation (won't actually connect)
        openai_provider = OpenAIProvider({'type': 'openai', 'api_key': 'test-key'})
        print("✓ OpenAIProvider initialized")
        
        anthropic_provider = AnthropicProvider({'type': 'anthropic', 'api_key': 'test-key'})
        print("✓ AnthropicProvider initialized")
        
        compatible_provider = OpenAICompatibleProvider({'type': 'openai-compatible', 'base_url': 'http://localhost:1234'})
        print("✓ OpenAICompatibleProvider initialized")
        
        cli_provider = CLIProvider({'type': 'cli', 'command': 'test-cli'})
        print("✓ CLIProvider initialized")
        
        return True
    except Exception as e:
        print(f"✗ Failed to initialize providers: {e}")
        return False

def test_core_functionality():
    """Test core functionality of components."""
    print("\nTesting core functionality...")
    
    try:
        from core.orchestrator import JevOrchestrator
        from core.router import ModelRouter
        from core.rag import RepositoryRAG
        from core.context import ContextBuilder
        
        # Test orchestrator
        orchestrator = JevOrchestrator("config.yaml")
        
        # Test router
        router = ModelRouter({})
        capabilities = router._get_required_capabilities('code')
        print(f"✓ ModelRouter capabilities: {capabilities}")
        
        # Test RAG
        rag = RepositoryRAG()
        print("✓ RepositoryRAG initialized")
        
        # Test context builder
        context_builder = ContextBuilder()
        print("✓ ContextBuilder initialized")
        
        return True
    except Exception as e:
        print(f"✗ Core functionality test failed: {e}")
        return False

def main():
    """Run all tests."""
    print("=== Jev AI Orchestrator Startup Test ===\n")
    
    success = True
    
    success &= test_imports()
    success &= test_orchestrator_creation()
    success &= test_provider_initialization()
    success &= test_core_functionality()
    
    print("\n" + "="*50)
    if success:
        print("✓ All tests passed! Jev AI Orchestrator is ready.")
        return 0
    else:
        print("✗ Some tests failed!")
        return 1

if __name__ == "__main__":
    sys.exit(main())