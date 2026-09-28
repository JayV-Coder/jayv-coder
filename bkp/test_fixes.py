#!/usr/bin/env python3
"""
Unit tests for Jev AI Orchestrator components.
"""

import sys
import os
import unittest
from unittest.mock import Mock, patch

# Add the project root to the Python path
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from core.orchestrator import JevOrchestrator
from core.router import ModelRouter
from core.rag import RepositoryRAG
from core.execution_graph.graph import ExecutionGraph, TaskNode, TaskType
from core.context_engine.context_fork import ContextForkManager
from core.context_engine.semantic_cache import SemanticContextCache
from core.context_engine.context_firewall import ContextFirewall
from providers.openai import OpenAIProvider
from providers.anthropic import AnthropicProvider


class TestJevOrchestrator(unittest.TestCase):
    """Test cases for JevOrchestrator."""

    def setUp(self):
        """Set up test fixtures before each test method."""
        self.orchestrator = JevOrchestrator("config.yaml")

    def test_orchestrator_initialization(self):
        """Test that orchestrator initializes correctly."""
        self.assertIsNotNone(self.orchestrator)
        self.assertIsInstance(self.orchestrator, JevOrchestrator)

    def test_intent_analysis(self):
        """Test intent analysis functionality."""
        # Test with a code-related query
        result = self.orchestrator._analyze_intent("Write a Python function to sort a list")
        self.assertIn('intent', result)
        self.assertEqual(result['intent'], 'code')
        
        # Test with a general query
        result = self.orchestrator._analyze_intent("What is the weather today?")
        self.assertIn('intent', result)
        self.assertEqual(result['intent'], 'general')

    def test_complexity_analysis(self):
        """Test complexity analysis functionality."""
        # Test simple intent
        intent_simple = {'intent': 'general', 'scores': {}, 'confidence': 0.0}
        complexity = self.orchestrator._analyze_complexity(intent_simple)
        self.assertIn(complexity, ['trivial', 'simple', 'medium', 'complex'])

    def test_context_planning(self):
        """Test context planning functionality."""
        intent_analysis = {'intent': 'code'}
        complexity = 'simple'
        context_plan = self.orchestrator._plan_context(intent_analysis, complexity)
        self.assertIsInstance(context_plan, list)
        # Should include code files and dependencies for code-related tasks
        self.assertIn('code_files', context_plan)
        self.assertIn('dependencies', context_plan)


class TestModelRouter(unittest.TestCase):
    """Test cases for ModelRouter."""

    def setUp(self):
        """Set up test fixtures before each test method."""
        self.config = {
            'models': {
                'test-model': {
                    'provider': 'openai',
                    'model': 'gpt-3.5-turbo',
                    'capabilities': ['chat', 'code'],
                    'cost_class': 'low',
                    'speed': 'fast'
                }
            },
            'budgets': {
                'trivial': 2000,
                'simple': 5000,
                'medium': 12000,
                'complex': 30000
            }
        }
        self.router = ModelRouter(self.config)

    def test_model_router_initialization(self):
        """Test that model router initializes correctly."""
        self.assertIsNotNone(self.router)
        self.assertIsInstance(self.router, ModelRouter)

    def test_get_required_capabilities(self):
        """Test capability mapping."""
        capabilities = self.router._get_required_capabilities('code')
        self.assertIsInstance(capabilities, list)
        self.assertIn('code', capabilities)
        self.assertIn('tools', capabilities)


class TestExecutionGraph(unittest.TestCase):
    """Test cases for ExecutionGraph."""

    def setUp(self):
        """Set up test fixtures before each test method."""
        from core.memory import MemoryManager
        memory_manager = MemoryManager()
        self.graph = ExecutionGraph(memory_manager)

    def test_execution_graph_initialization(self):
        """Test that execution graph initializes correctly."""
        self.assertIsNotNone(self.graph)
        self.assertIsInstance(self.graph, ExecutionGraph)

    def test_task_management(self):
        """Test task creation and management."""
        task = TaskNode(task_type=TaskType.CODE_GENERATION)
        task_id = self.graph.add_task(task)
        self.assertIn(task_id, self.graph.tasks)
        
        # Test getting task
        retrieved_task = self.graph.get_task(task_id)
        self.assertIsNotNone(retrieved_task)
        self.assertEqual(retrieved_task.task_id, task_id)


class TestContextForkManager(unittest.TestCase):
    """Test cases for ContextForkManager."""

    def setUp(self):
        """Set up test fixtures before each test method."""
        from core.rag import RepositoryRAG
        from core.context import ContextBuilder
        rag = RepositoryRAG()
        context_builder = ContextBuilder()
        self.fork_manager = ContextForkManager(rag, context_builder)

    def test_context_fork_initialization(self):
        """Test that context fork manager initializes correctly."""
        self.assertIsNotNone(self.fork_manager)
        self.assertIsInstance(self.fork_manager, ContextForkManager)

    def test_create_fork(self):
        """Test creating a context fork."""
        context_data = {"files": ["test.py"]}
        fork_id = self.fork_manager.create_fork(
            task_id="test_task",
            parent_context_id="parent_001",
            context_data=context_data
        )
        self.assertIsNotNone(fork_id)
        self.assertIn(fork_id, self.fork_manager.forks)


class TestSemanticContextCache(unittest.TestCase):
    """Test cases for SemanticContextCache."""

    def setUp(self):
        """Set up test fixtures before each test method."""
        self.cache = SemanticContextCache()

    def test_semantic_cache_initialization(self):
        """Test that semantic cache initializes correctly."""
        self.assertIsNotNone(self.cache)
        self.assertIsInstance(self.cache, SemanticContextCache)

    def test_cache_operations(self):
        """Test cache operations."""
        context = {"files": ["test.py"], "content": "test content"}
        cache_key = self.cache.cache_context(
            query="test query",
            repository_hash="abc123",
            git_commit="commit123",
            semantic_query="test semantic",
            retrieved_symbols=["test"],
            file_hashes={"test.py": "hash1"},
            context=context
        )
        self.assertIsNotNone(cache_key)
        
        # Test retrieval
        cached = self.cache.get_cached_context(
            query="test query",
            repository_hash="abc123",
            git_commit="commit123",
            semantic_query="test semantic",
            retrieved_symbols=["test"],
            file_hashes={"test.py": "hash1"}
        )
        self.assertIsNotNone(cached)


class TestContextFirewall(unittest.TestCase):
    """Test cases for ContextFirewall."""

    def setUp(self):
        """Set up test fixtures before each test method."""
        self.firewall = ContextFirewall()

    def test_context_firewall_initialization(self):
        """Test that context firewall initializes correctly."""
        self.assertIsNotNone(self.firewall)
        self.assertIsInstance(self.firewall, ContextFirewall)

    def test_file_privacy_checking(self):
        """Test privacy checking of files."""
        sensitive_file = ".env"
        info = self.firewall.check_file_privacy(sensitive_file)
        self.assertTrue(info['is_sensitive'])


class TestProviders(unittest.TestCase):
    """Test cases for provider implementations."""

    def test_openai_provider_initialization(self):
        """Test OpenAI provider initialization."""
        config = {
            'type': 'openai',
            'api_key': 'test-key'
        }
        provider = OpenAIProvider(config)
        self.assertIsNotNone(provider)
        self.assertIsInstance(provider, OpenAIProvider)

    def test_anthropic_provider_initialization(self):
        """Test Anthropic provider initialization."""
        config = {
            'type': 'anthropic',
            'api_key': 'test-key'
        }
        provider = AnthropicProvider(config)
        self.assertIsNotNone(provider)
        self.assertIsInstance(provider, AnthropicProvider)


if __name__ == '__main__':
    unittest.main()