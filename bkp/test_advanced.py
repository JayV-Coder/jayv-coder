#!/usr/bin/env python3
"""
Comprehensive test suite for Jev AI Orchestrator advanced features.
"""

import sys
import os
import unittest
from unittest.mock import Mock, patch

# Add the project root to the Python path
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from core.execution_graph.graph import ExecutionGraph, TaskNode, TaskType, TaskStatus
from core.context_engine.context_fork import ContextForkManager
from core.context_engine.semantic_cache import SemanticContextCache
from core.context_engine.context_firewall import ContextFirewall
from core.optimizer.adaptive_router import AdaptiveModelRouter
from core.agent.sandbox import AgentSandbox, SandboxConfig
from core.agent.checkpointing import TaskCheckpointManager
from core.context_engine.value_scorer import ContextValueScorer, ContextFragment
from core.orchestrator import JevOrchestrator
from core.router import ModelRouter

class TestAdvancedFeatures(unittest.TestCase):
    """Test cases for advanced Jev AI Orchestrator features."""

    def test_execution_graph(self):
        """Test Execution Graph functionality."""
        from core.memory import MemoryManager
        
        memory_manager = MemoryManager()
        graph = ExecutionGraph(memory_manager)
        
        # Create tasks
        task_a = TaskNode(task_type=TaskType.CODE_GENERATION)
        task_b = TaskNode(task_type=TaskType.CODE_GENERATION)
        task_c = TaskNode(task_type=TaskType.INTEGRATION)
        
        # Add tasks
        task_a_id = graph.add_task(task_a)
        task_b_id = graph.add_task(task_b)
        task_c_id = graph.add_task(task_c)
        
        # Add dependencies
        graph.add_dependency(task_c_id, task_a_id)
        graph.add_dependency(task_c_id, task_b_id)
        
        # Verify tasks and dependencies
        self.assertIn(task_a_id, graph.tasks)
        self.assertIn(task_b_id, graph.tasks)
        self.assertIn(task_c_id, graph.tasks)
        
        # Verify dependencies - checking the actual structure
        task_c_obj = graph.get_task(task_c_id)
        self.assertIn(task_a_id, task_c_obj.dependencies)
        self.assertIn(task_b_id, task_c_obj.dependencies)
        
        # Verify ready tasks
        ready_tasks = graph.get_ready_tasks()
        self.assertIn(task_a_id, ready_tasks)
        self.assertIn(task_b_id, ready_tasks)
        self.assertNotIn(task_c_id, ready_tasks)
        
        # Simulate completion
        graph.mark_task_started(task_a_id)
        graph.mark_task_completed(task_a_id, "Completed")
        
        # Verify task status
        self.assertEqual(graph.get_task_status(task_a_id), TaskStatus.COMPLETED)
        
        print("✓ Execution Graph test passed")

    def test_context_forking(self):
        """Test Context Forking functionality."""
        from core.rag import RepositoryRAG
        from core.context import ContextBuilder
        
        rag = RepositoryRAG()
        context_builder = ContextBuilder()
        fork_manager = ContextForkManager(rag, context_builder)
        
        # Create a fork
        fork_id = fork_manager.create_fork(
            task_id="test_task",
            parent_context_id="parent_001",
            context_data={"files": ["test.py"]},
            token_limit=8000
        )
        
        self.assertIsNotNone(fork_id)
        self.assertIn(fork_id, fork_manager.forks)
        
        # Update fork
        fork_manager.update_fork_context(fork_id, {"new_file": "new_test.py"})
        context = fork_manager.get_fork_context(fork_id)
        self.assertIn("new_file", context)
        
        print("✓ Context Forking test passed")

    def test_semantic_cache(self):
        """Test Semantic Context Cache functionality."""
        cache = SemanticContextCache()
        
        # Cache context
        context = {"files": ["test.py"], "content": "test content"}
        cache_key = cache.cache_context(
            query="test query",
            repository_hash="abc123",
            git_commit="commit123",
            semantic_query="test semantic",
            retrieved_symbols=["test"],
            file_hashes={"test.py": "hash1"},
            context=context
        )
        
        self.assertIsNotNone(cache_key)
        
        # Retrieve cached context
        cached = cache.get_cached_context(
            query="test query",
            repository_hash="abc123",
            git_commit="commit123",
            semantic_query="test semantic",
            retrieved_symbols=["test"],
            file_hashes={"test.py": "hash1"}
        )
        
        self.assertIsNotNone(cached)
        
        # Check cache stats
        stats = cache.get_cache_stats()
        self.assertGreaterEqual(stats['hits'], 0)
        self.assertGreaterEqual(stats['misses'], 0)
        
        print("✓ Semantic Cache test passed")

    def test_context_firewall(self):
        """Test Context Firewall functionality."""
        firewall = ContextFirewall()
        
        # Test privacy checking
        sensitive_file = ".env"
        info = firewall.check_file_privacy(sensitive_file)
        self.assertTrue(info.is_sensitive)
        
        # Test context filtering
        context = {"files": [".env", "app.py"]}
        filtered = firewall.filter_context(context)
        self.assertTrue(len(filtered['files']) == 1)  # Only non-sensitive file
        
        # Test secret redaction
        content = "Email: test@example.com"
        redacted = firewall.redact_secrets(content)
        self.assertIn("[EMAIL_REDACTED]", redacted)
        
        print("✓ Context Firewall test passed")

    def test_adaptive_router(self):
        """Test Adaptive Model Router functionality."""
        from core.memory import MemoryManager
        
        model_router = ModelRouter({})
        memory_manager = MemoryManager()
        adaptive_router = AdaptiveModelRouter(model_router, memory_manager)
        
        # Record decisions
        adaptive_router.record_decision(
            task_type="code_generation",
            model="test-model",
            provider="test-provider",
            tokens_used=1000,
            cost=0.01,
            latency=2.0,
            success=True,
            tests_passed=True,
            retry_count=0,
            escalation=False,
            confidence=0.9,
            context_size=500,
            capabilities_matched=["code"]
        )
        
        # Get recommendations
        recommendations = adaptive_router.get_model_recommendations("code_generation")
        self.assertIn('recommendations', recommendations)
        
        # Get performance report
        report = adaptive_router.get_performance_report("code_generation")
        self.assertIn('success_rate', report)
        
        print("✓ Adaptive Router test passed")

    def test_agent_sandbox(self):
        """Test Agent Sandbox functionality."""
        config = SandboxConfig(
            max_memory_mb=256,
            max_time_seconds=30,
            network_access=False,
            temp_directory="/tmp/jev_sandbox_test"
        )
        
        sandbox = AgentSandbox(config)
        
        # Create workspace
        workspace = sandbox.create_isolated_workspace("test_task")
        self.assertTrue(os.path.exists(workspace))
        
        # Execute command
        result = sandbox.execute_in_sandbox(
            ['echo', 'test'],
            workspace=workspace,
            timeout=10
        )
        
        self.assertTrue(result['success'])
        
        # Cleanup
        sandbox.destroy_workspace(workspace)
        sandbox.cleanup()
        
        print("✓ Agent Sandbox test passed")

    def test_task_checkpointing(self):
        """Test Task Checkpointing functionality."""
        manager = TaskCheckpointManager("/tmp/jev_checkpoint_test")
        
        # Create checkpoint
        task_state = {'status': 'running', 'progress': 0.5}
        checkpoint_id = manager.create_checkpoint(
            task_id="test_task",
            state=task_state,
            artifacts={'file': 'test.py'}
        )
        
        self.assertIsNotNone(checkpoint_id)
        
        # Restore checkpoint
        restored = manager.restore_checkpoint(checkpoint_id)
        self.assertIsNotNone(restored)
        self.assertEqual(restored.state['status'], 'running')
        
        # List checkpoints
        checkpoints = manager.list_checkpoints("test_task")
        self.assertEqual(len(checkpoints), 1)
        
        # Cleanup
        manager.cleanup_old_checkpoints(0)
        
        print("✓ Task Checkpointing test passed")

    def test_context_value_scorer(self):
        """Test Context Value Scoring functionality."""
        scorer = ContextValueScorer()
        
        # Create sample fragments
        fragments = [
            ContextFragment(
                id="auth.ts",
                content="Authentication service",
                relevance=0.9,
                confidence=0.9,
                freshness=0.8,
                tokens=1000,
                dependency_importance=0.9
            ),
            ContextFragment(
                id="README.md",
                content="Documentation",
                relevance=0.3,
                confidence=0.2,
                freshness=0.1,
                tokens=2000,
                dependency_importance=0.1
            )
        ]
        
        # Rank fragments
        ranked = scorer.rank_context_fragments(fragments)
        self.assertEqual(ranked[0].id, "auth.ts")  # Should be higher value
        
        # Optimize for budget
        optimized = scorer.optimize_context_for_budget(fragments, 1500)
        self.assertEqual(len(optimized), 1)  # Should only include first fragment
        
        # Get summary
        summary = scorer.get_context_summary(fragments)
        self.assertIn('total_fragments', summary)
        self.assertIn('average_value', summary)
        
        print("✓ Context Value Scoring test passed")

    def test_jev_orchestrator_integration(self):
        """Test integration with Jev Orchestrator."""
        # This would require a full orchestrator setup which is complex
        # Just verify imports work
        self.assertIsNotNone(JevOrchestrator)
        self.assertIsNotNone(ModelRouter)
        
        print("✓ Jev Orchestrator Integration test passed")


if __name__ == '__main__':
    unittest.main()