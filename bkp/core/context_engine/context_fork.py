"""
Context Fork Manager for Jev AI Orchestrator
Manages isolated context forks for tasks to reduce token consumption.
"""

import time
import uuid
from typing import Dict, List, Any, Optional
from dataclasses import dataclass
from core.context_engine.context_builder import ContextBuilder

@dataclass
class ForkContext:
    """Represents the context for a fork."""
    
    fork_id: str
    parent_context_id: str
    task_id: str
    context_data: Dict[str, Any]
    created_at: float
    expires_at: float
    token_limit: int
    is_active: bool = True

class ContextForkManager:
    """Manages isolated context forks for tasks."""
    
    def __init__(self, rag, context_builder: ContextBuilder, 
                 ttl_seconds: int = 3600):
        self.rag = rag
        self.context_builder = context_builder
        self.ttl_seconds = ttl_seconds
        self.forks: Dict[str, ForkContext] = {}
        
    def create_fork(self, task_id: str, parent_context_id: str, 
                   context_data: Dict[str, Any], 
                   token_limit: int = 8000) -> str:
        """
        Create a new context fork for a task.
        
        Returns:
            Fork ID
        """
        fork_id = f"fork_{uuid.uuid4().hex[:8]}"
        
        fork_context = ForkContext(
            fork_id=fork_id,
            parent_context_id=parent_context_id,
            task_id=task_id,
            context_data=context_data,
            created_at=time.time(),
            expires_at=time.time() + self.ttl_seconds,
            token_limit=token_limit,
            is_active=True
        )
        
        self.forks[fork_id] = fork_context
        return fork_id
    
    def get_fork_context(self, fork_id: str) -> Optional[Dict[str, Any]]:
        """
        Get the context data for a fork.
        
        Returns:
            Context data or None if fork doesn't exist
        """
        if fork_id not in self.forks:
            return None
            
        fork = self.forks[fork_id]
        if time.time() > fork.expires_at:
            # Expire the fork
            fork.is_active = False
            return None
            
        return fork.context_data
    
    def update_fork_context(self, fork_id: str, 
                           updates: Dict[str, Any]) -> bool:
        """
        Update context data for a fork.
        
        Returns:
            True if successful, False otherwise
        """
        if fork_id not in self.forks:
            return False
            
        fork = self.forks[fork_id]
        if time.time() > fork.expires_at:
            # Expire the fork
            fork.is_active = False
            return False
            
        # Update context data
        fork.context_data.update(updates)
        return True
    
    def expire_fork(self, fork_id: str) -> bool:
        """
        Manually expire a fork.
        
        Returns:
            True if successful, False otherwise
        """
        if fork_id not in self.forks:
            return False
            
        self.forks[fork_id].is_active = False
        return True
    
    def get_active_forks(self) -> List[str]:
        """
        Get list of active forks.
        
        Returns:
            List of active fork IDs
        """
        active_forks = []
        current_time = time.time()
        
        for fork_id, fork in self.forks.items():
            if fork.is_active and current_time <= fork.expires_at:
                active_forks.append(fork_id)
                
        return active_forks

# Example usage function
def demo_context_fork():
    """Demonstrate context forking functionality."""
    
    from core.rag import RepositoryRAG
    
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

if __name__ == "__main__":
    demo_context_fork()