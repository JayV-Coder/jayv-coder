"""
Execution Graph for Jev AI Orchestrator
Defines the task decomposition and execution flow.
"""

import uuid
import time
from typing import Dict, List, Any, Optional
from dataclasses import dataclass, field
from enum import Enum
from core.memory import MemoryManager
from core.context_engine.context_builder import ContextBuilder
from core.tools import ToolRegistry

class TaskStatus(Enum):
    PENDING = "pending"
    RUNNING = "running"
    COMPLETED = "completed"
    FAILED = "failed"
    RETRYING = "retrying"
    CANCELLED = "cancelled"

class TaskType(Enum):
    CODE_GENERATION = "code_generation"
    CODE_REVIEW = "code_review"
    ARCHITECTURE = "architecture"
    TESTING = "testing"
    INTEGRATION = "integration"
    DOCUMENTATION = "documentation"
    CONFIGURATION = "configuration"
    DEPLOYMENT = "deployment"

@dataclass
class TaskNode:
    """Represents a single task in the execution graph."""
    
    task_type: TaskType
    task_id: str = field(default_factory=lambda: str(uuid.uuid4()))
    dependencies: List[str] = field(default_factory=list)
    required_capabilities: List[str] = field(default_factory=list)
    context: Dict[str, Any] = field(default_factory=dict)
    provider: Optional[str] = None
    model: Optional[str] = None
    budget: Dict[str, Any] = field(default_factory=dict)
    timeout: int = 300  # 5 minutes default
    retry_policy: Dict[str, Any] = field(default_factory=dict)
    status: TaskStatus = TaskStatus.PENDING
    artifacts: List[str] = field(default_factory=list)
    validation: Dict[str, Any] = field(default_factory=dict)
    start_time: Optional[float] = None
    end_time: Optional[float] = None
    result: Optional[Any] = None
    error: Optional[str] = None
    
    def __post_init__(self):
        """Initialize default values."""
        if not self.retry_policy:
            self.retry_policy = {
                'max_retries': 3,
                'delay': 1,
                'backoff_factor': 2
            }
        if not self.budget:
            self.budget = {
                'max_tokens': 10000,
                'max_cost': 0.50,
                'max_time': 300
            }

class ExecutionGraph:
    """Manages the execution graph for complex tasks."""
    
    def __init__(self, memory_manager: MemoryManager):
        self.memory_manager = memory_manager
        self.tasks: Dict[str, TaskNode] = {}
        self.task_queue: List[str] = []
        self.completed_tasks: List[str] = []
        self.failed_tasks: List[str] = []
        self.context_builder = ContextBuilder()
        self.tool_registry = ToolRegistry()
        
    def add_task(self, task: TaskNode) -> str:
        """Add a task to the execution graph."""
        self.tasks[task.task_id] = task
        return task.task_id
        
    def add_dependency(self, task_id: str, dependency_id: str) -> None:
        """Add a dependency relationship between tasks."""
        if task_id in self.tasks and dependency_id in self.tasks:
            self.tasks[task_id].dependencies.append(dependency_id)
            
    def get_ready_tasks(self) -> List[str]:
        """Get tasks that are ready to execute (all dependencies completed)."""
        ready_tasks = []
        for task_id, task in self.tasks.items():
            if task.status == TaskStatus.PENDING:
                # Check if all dependencies are completed
                dependencies_completed = all(
                    dep in self.completed_tasks 
                    for dep in task.dependencies
                )
                if dependencies_completed:
                    ready_tasks.append(task_id)
        return ready_tasks
        
    def mark_task_started(self, task_id: str) -> None:
        """Mark a task as started."""
        if task_id in self.tasks:
            self.tasks[task_id].status = TaskStatus.RUNNING
            self.tasks[task_id].start_time = time.time()
            
    def mark_task_completed(self, task_id: str, result: Any = None) -> None:
        """Mark a task as completed."""
        if task_id in self.tasks:
            self.tasks[task_id].status = TaskStatus.COMPLETED
            self.tasks[task_id].end_time = time.time()
            self.tasks[task_id].result = result
            self.completed_tasks.append(task_id)
            
    def mark_task_failed(self, task_id: str, error: str = None) -> None:
        """Mark a task as failed."""
        if task_id in self.tasks:
            self.tasks[task_id].status = TaskStatus.FAILED
            self.tasks[task_id].error = error
            self.failed_tasks.append(task_id)
            
    def get_task(self, task_id: str) -> Optional[TaskNode]:
        """Get a task by ID."""
        return self.tasks.get(task_id)
        
    def get_all_tasks(self) -> Dict[str, TaskNode]:
        """Get all tasks in the graph."""
        return self.tasks.copy()
        
    def get_task_status(self, task_id: str) -> Optional[TaskStatus]:
        """Get the status of a specific task."""
        task = self.get_task(task_id)
        return task.status if task else None
        
    def validate_execution(self) -> bool:
        """Validate that the execution graph is valid."""
        # Check for circular dependencies
        visited = set()
        rec_stack = set()
        
        def is_cyclic_util(task_id: str) -> bool:
            if task_id not in visited:
                visited.add(task_id)
                rec_stack.add(task_id)
                
                task = self.get_task(task_id)
                if task:
                    for dep_id in task.dependencies:
                        if dep_id not in visited and is_cyclic_util(dep_id):
                            return True
                        elif dep_id in rec_stack:
                            return True
                            
            rec_stack.discard(task_id)
            return False
            
        # Check all tasks
        for task_id in self.tasks:
            if task_id not in visited:
                if is_cyclic_util(task_id):
                    return False
                    
        return True