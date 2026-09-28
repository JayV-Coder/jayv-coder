#!/usr/bin/env python3
"""
Simplified test for Jev AI Orchestrator advanced features.
"""

import sys
import os
import logging

# Add the project root to the Python path
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s %(name)s %(levelname)s %(message)s'
)

logger = logging.getLogger(__name__)

def demonstrate_context_forking():
    """Demonstrate context forking functionality."""
    print("\n=== Context Forking Demo ===")
    
    try:
        from core.context_engine.context_fork import ContextForkManager
        from core.rag import RepositoryRAG
        from core.context_engine.context_builder import ContextBuilder
        
        # Create components
        rag = RepositoryRAG()
        context_builder = ContextBuilder()
        fork_manager = ContextForkManager(rag, context_builder)
        
        # Create a fork
        fork_id = fork_manager.create_fork(
            task_id="task_001",
            parent_context_id="parent_001",
            context_data={
                "files": ["auth.service.ts", "token.service.ts"],
                "dependencies": ["AGENTS.md#backend"],
                "context": "Authentication service implementation"
            },
            token_limit=8000
        )
        
        print(f"Created fork: {fork_id}")
        print(f"Fork context: {fork_manager.get_fork_context(fork_id)}")
        
        # Update fork context
        fork_manager.update_fork_context(fork_id, {"new_file": "new_auth.ts"})
        print(f"Updated fork context: {fork_manager.get_fork_context(fork_id)}")
        
        print("✓ Context Forking demo completed successfully")
        return True
        
    except Exception as e:
        print(f"✗ Context Forking demo failed: {e}")
        import traceback
        traceback.print_exc()
        return False

def demonstrate_semantic_cache():
    """Demonstrate semantic caching functionality."""
    print("\n=== Semantic Cache Demo ===")
    
    try:
        from core.context_engine.semantic_cache import SemanticContextCache
        
        # Create cache
        cache = SemanticContextCache()
        
        # Create some context data
        context = {
            "files": ["auth.service.ts", "token.service.ts"],
            "content": "Authentication service implementation",
            "related_symbols": ["AuthService", "TokenService"]
        }
        
        # Cache context
        cache_key = cache.cache_context(
            query="Implement authentication",
            repository_hash="abc123",
            git_commit="commit_hash_123",
            semantic_query="authentication implementation",
            retrieved_symbols=["AuthService", "TokenService"],
            file_hashes={"auth.service.ts": "hash1", "token.service.ts": "hash2"},
            context=context,
            semantic_similarity=0.95
        )
        
        print(f"Cached context with key: {cache_key}")
        
        # Retrieve cached context
        cached = cache.get_cached_context(
            query="Implement authentication",
            repository_hash="abc123",
            git_commit="commit_hash_123",
            semantic_query="authentication implementation",
            retrieved_symbols=["AuthService", "TokenService"],
            file_hashes={"auth.service.ts": "hash1", "token.service.ts": "hash2"}
        )
        
        print(f"Retrieved cached context: {cached is not None}")
        
        # Check cache stats
        stats = cache.get_cache_stats()
        print(f"Cache stats: {stats}")
        
        print("✓ Semantic Cache demo completed successfully")
        return True
        
    except Exception as e:
        print(f"✗ Semantic Cache demo failed: {e}")
        import traceback
        traceback.print_exc()
        return False

def demonstrate_context_firewall():
    """Demonstrate context firewall functionality."""
    print("\n=== Context Firewall Demo ===")
    
    try:
        from core.context_engine.context_firewall import ContextFirewall
        
        # Create firewall
        firewall = ContextFirewall()
        
        # Test privacy checking
        sensitive_files = [
            ".env",
            "src/main.py",
            ".ssh/id_rsa",
            "config.json",
            "secret.key"
        ]
        
        for file_path in sensitive_files:
            info = firewall.check_file_privacy(file_path)
            print(f"File {file_path}: sensitive={info.is_sensitive}")
        
        # Test context filtering
        context = {
            "files": [".env", "src/main.py", "app.py"],
            "content": "Main application code"
        }
        
        filtered = firewall.filter_context(context)
        # Fix for the issue: files are strings, not dicts
        if 'files' in filtered and isinstance(filtered['files'], list):
            # If the files are strings, we need to check them differently
            sensitive_count = 0
            for file_item in filtered['files']:
                if isinstance(file_item, str):
                    info = firewall.check_file_privacy(file_item)
                    if info.is_sensitive:
                        sensitive_count += 1
            print(f"Filtered context has sensitive files: {sensitive_count > 0}")
        
        # Test secret redaction
        sensitive_content = "Contact email: test@example.com and IP: 192.168.1.1"
        redacted = firewall.redact_secrets(sensitive_content)
        print(f"Redacted content: {redacted}")
        
        print("✓ Context Firewall demo completed successfully")
        return True
        
    except Exception as e:
        print(f"✗ Context Firewall demo failed: {e}")
        import traceback
        traceback.print_exc()
        return False

def demonstrate_adaptive_router():
    """Demonstrate adaptive routing functionality."""
    print("\n=== Adaptive Router Demo ===")
    
    try:
        from core.optimizer.adaptive_router import AdaptiveModelRouter
        from core.router import ModelRouter
        from core.memory import MemoryManager
        
        # Create components
        model_router = ModelRouter({})
        memory_manager = MemoryManager()
        adaptive_router = AdaptiveModelRouter(model_router, memory_manager)
        
        # Record some decisions for learning
        adaptive_router.record_decision(
            task_type="code_generation",
            model="qwen-local",
            provider="lmstudio",
            tokens_used=2000,
            cost=0.0,
            latency=5.0,
            success=True,
            tests_passed=True,
            retry_count=0,
            escalation=False,
            confidence=0.9,
            context_size=1000,
            capabilities_matched=["code", "chat"]
        )
        
        adaptive_router.record_decision(
            task_type="code_generation",
            model="claude-3-opus",
            provider="anthropic",
            tokens_used=3000,
            cost=0.03,
            latency=10.0,
            success=True,
            tests_passed=True,
            retry_count=0,
            escalation=False,
            confidence=0.85,
            context_size=1500,
            capabilities_matched=["code", "reasoning", "tools"]
        )
        
        # Get model recommendations
        recommendations = adaptive_router.get_model_recommendations("code_generation")
        print(f"Model recommendations: {recommendations['recommendations']}")
        
        # Get performance report
        report = adaptive_router.get_performance_report("code_generation")
        print(f"Performance report: {report['success_rate']:.2f} success rate")
        
        print("✓ Adaptive Router demo completed successfully")
        return True
        
    except Exception as e:
        print(f"✗ Adaptive Router demo failed: {e}")
        import traceback
        traceback.print_exc()
        return False

def demonstrate_sandbox():
    """Demonstrate agent sandbox functionality."""
    print("\n=== Agent Sandbox Demo ===")
    
    try:
        from core.agent.sandbox import AgentSandbox, SandboxConfig
        
        # Create sandbox with basic configuration
        config = SandboxConfig(
            max_memory_mb=256,
            max_time_seconds=60,
            network_access=False,
            allow_spawn_processes=False,
            temp_directory="/tmp/jev_sandbox_demo"
        )
        
        sandbox = AgentSandbox(config)
        
        # Create workspace
        workspace = sandbox.create_isolated_workspace("demo_task")
        print(f"Created workspace: {workspace}")
        
        # Test execution
        result = sandbox.execute_in_sandbox(
            ['echo', 'Hello from sandbox!'],
            workspace=workspace,
            timeout=10
        )
        
        print(f"Execution result: {result['success']}")
        
        # Cleanup
        sandbox.destroy_workspace(workspace)
        sandbox.cleanup()
        
        print("✓ Agent Sandbox demo completed successfully")
        return True
        
    except Exception as e:
        print(f"✗ Agent Sandbox demo failed: {e}")
        import traceback
        traceback.print_exc()
        return False

def demonstrate_checkpointing():
    """Demonstrate task checkpointing functionality."""
    print("\n=== Task Checkpointing Demo ===")
    
    try:
        from core.agent.checkpointing import TaskCheckpointManager
        
        # Create checkpoint manager
        manager = TaskCheckpointManager("/tmp/jev_checkpoints_demo")
        
        # Create sample state
        task_state = {
            'status': 'running',
            'current_step': 'implementation',
            'progress': 0.65,
            'files_processed': ['auth.ts', 'token.ts', 'user.ts'],
            'errors': []
        }
        
        # Create checkpoint
        checkpoint_id = manager.create_checkpoint(
            task_id="task_001",
            state=task_state,
            artifacts={
                'source_code': 'src/auth.ts',
                'test_results': 'tests/auth.test'
            },
            progress={'step': 'implementation', 'percentage': 65},
            dependencies=['task_000'],
            metadata={'description': 'Authentication implementation'}
        )
        
        print(f"Created checkpoint: {checkpoint_id}")
        
        # List checkpoints
        checkpoints = manager.list_checkpoints()
        print(f"Checkpoints: {len(checkpoints)}")
        
        # Restore checkpoint
        restored = manager.restore_checkpoint(checkpoint_id)
        if restored:
            print(f"Restored checkpoint state: {restored.state}")
        
        # Stats
        stats = manager.get_checkpoint_stats()
        print(f"Checkpoint stats: {stats['total_checkpoints']} total checkpoints")
        
        # Cleanup
        manager.cleanup_old_checkpoints(0)  # Delete all
        
        print("✓ Task Checkpointing demo completed successfully")
        return True
        
    except Exception as e:
        print(f"✗ Task Checkpointing demo failed: {e}")
        import traceback
        traceback.print_exc()
        return False

def main():
    """Run all demonstrations."""
    print("Starting Jev AI Orchestrator Advanced Features Demo")
    print("="*50)
    
    demos = [
        demonstrate_context_forking,
        demonstrate_semantic_cache,
        demonstrate_context_firewall,
        demonstrate_adaptive_router,
        demonstrate_sandbox,
        demonstrate_checkpointing
    ]
    
    results = []
    for demo_func in demos:
        try:
            result = demo_func()
            results.append(result)
        except Exception as e:
            print(f"Demo {demo_func.__name__} failed with exception: {e}")
            import traceback
            traceback.print_exc()
            results.append(False)
    
    print("\n" + "="*50)
    print("Demo Results Summary:")
    for i, (demo_func, result) in enumerate(zip(demos, results)):
        status = "✓ PASS" if result else "✗ FAIL"
        print(f"  {i+1}. {demo_func.__name__}: {status}")
    
    passed = sum(results)
    total = len(results)
    print(f"\nOverall: {passed}/{total} demos passed")
    
    return 0 if all(results) else 1

if __name__ == "__main__":
    sys.exit(main())