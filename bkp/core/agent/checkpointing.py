"""
Task Checkpointing for Jev AI Orchestrator
Provides state persistence for long-running tasks.
"""

import os
import json
import pickle
import time
from typing import Dict, List, Any, Optional
from dataclasses import dataclass, field
from pathlib import Path
import logging
from datetime import datetime

logger = logging.getLogger(__name__)

@dataclass
class TaskCheckpoint:
    """Represents a task checkpoint."""
    
    task_id: str
    checkpoint_id: str
    timestamp: float
    state: Dict[str, Any]
    artifacts: Dict[str, str] = field(default_factory=dict)
    progress: Dict[str, Any] = field(default_factory=dict)
    dependencies: List[str] = field(default_factory=list)
    metadata: Dict[str, Any] = field(default_factory=dict)
    
    def to_dict(self) -> Dict[str, Any]:
        """Convert checkpoint to dictionary."""
        return {
            'task_id': self.task_id,
            'checkpoint_id': self.checkpoint_id,
            'timestamp': self.timestamp,
            'state': self.state,
            'artifacts': self.artifacts,
            'progress': self.progress,
            'dependencies': self.dependencies,
            'metadata': self.metadata
        }
    
    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> 'TaskCheckpoint':
        """Create checkpoint from dictionary."""
        return cls(**data)

class TaskCheckpointManager:
    """Manages task checkpoints and state persistence."""
    
    def __init__(self, checkpoint_dir: str = ".jev/checkpoints"):
        self.checkpoint_dir = Path(checkpoint_dir)
        self.checkpoint_dir.mkdir(parents=True, exist_ok=True)
        self.checkpoint_files = {}
        
    def create_checkpoint(self, task_id: str, state: Dict[str, Any], 
                         artifacts: Dict[str, str] = None,
                         progress: Dict[str, Any] = None,
                         dependencies: List[str] = None,
                         metadata: Dict[str, Any] = None) -> str:
        """
        Create a new checkpoint for a task.
        
        Returns:
            Checkpoint ID
        """
        checkpoint_id = f"chk_{int(time.time())}_{len(self.checkpoint_files)}"
        
        checkpoint = TaskCheckpoint(
            task_id=task_id,
            checkpoint_id=checkpoint_id,
            timestamp=time.time(),
            state=state,
            artifacts=artifacts or {},
            progress=progress or {},
            dependencies=dependencies or [],
            metadata=metadata or {}
        )
        
        # Save checkpoint to file
        checkpoint_file = self.checkpoint_dir / f"{checkpoint_id}.json"
        try:
            with open(checkpoint_file, 'w') as f:
                json.dump(checkpoint.to_dict(), f, indent=2)
            
            # Track checkpoint file
            self.checkpoint_files[checkpoint_id] = str(checkpoint_file)
            
            logger.info(f"Created checkpoint {checkpoint_id} for task {task_id}")
            return checkpoint_id
            
        except Exception as e:
            logger.error(f"Failed to create checkpoint {checkpoint_id}: {e}")
            raise
            
    def restore_checkpoint(self, checkpoint_id: str) -> Optional[TaskCheckpoint]:
        """
        Restore a task from checkpoint.
        
        Returns:
            TaskCheckpoint object or None if not found
        """
        if checkpoint_id not in self.checkpoint_files:
            logger.warning(f"Checkpoint {checkpoint_id} not found")
            return None
            
        try:
            checkpoint_file = Path(self.checkpoint_files[checkpoint_id])
            with open(checkpoint_file, 'r') as f:
                data = json.load(f)
            
            checkpoint = TaskCheckpoint.from_dict(data)
            logger.info(f"Restored checkpoint {checkpoint_id}")
            return checkpoint
            
        except Exception as e:
            logger.error(f"Failed to restore checkpoint {checkpoint_id}: {e}")
            return None
            
    def get_latest_checkpoint(self, task_id: str) -> Optional[TaskCheckpoint]:
        """
        Get the latest checkpoint for a task.
        
        Returns:
            Latest TaskCheckpoint or None if no checkpoints
        """
        # Find all checkpoints for this task
        task_checkpoints = []
        for chk_id, chk_file in self.checkpoint_files.items():
            try:
                with open(chk_file, 'r') as f:
                    data = json.load(f)
                    if data.get('task_id') == task_id:
                        task_checkpoints.append((chk_id, data['timestamp']))
            except Exception:
                continue
                
        if not task_checkpoints:
            return None
            
        # Get the latest checkpoint
        latest_chk = max(task_checkpoints, key=lambda x: x[1])
        return self.restore_checkpoint(latest_chk[0])
        
    def list_checkpoints(self, task_id: str = None) -> List[Dict[str, Any]]:
        """
        List all checkpoints, optionally filtered by task ID.
        
        Returns:
            List of checkpoint information
        """
        checkpoints = []
        
        for chk_id, chk_file in self.checkpoint_files.items():
            try:
                with open(chk_file, 'r') as f:
                    data = json.load(f)
                    
                if task_id is None or data.get('task_id') == task_id:
                    checkpoints.append({
                        'id': chk_id,
                        'task_id': data.get('task_id'),
                        'timestamp': data.get('timestamp'),
                        'size': os.path.getsize(chk_file),
                        'metadata': data.get('metadata', {})
                    })
            except Exception as e:
                logger.warning(f"Failed to read checkpoint {chk_id}: {e}")
                continue
                
        # Sort by timestamp descending
        checkpoints.sort(key=lambda x: x['timestamp'], reverse=True)
        return checkpoints
        
    def delete_checkpoint(self, checkpoint_id: str) -> bool:
        """
        Delete a specific checkpoint.
        
        Returns:
            True if deleted successfully, False otherwise
        """
        if checkpoint_id not in self.checkpoint_files:
            return False
            
        try:
            checkpoint_file = Path(self.checkpoint_files[checkpoint_id])
            if checkpoint_file.exists():
                checkpoint_file.unlink()
                
            # Remove from tracking
            del self.checkpoint_files[checkpoint_id]
            
            logger.info(f"Deleted checkpoint {checkpoint_id}")
            return True
            
        except Exception as e:
            logger.error(f"Failed to delete checkpoint {checkpoint_id}: {e}")
            return False
            
    def cleanup_old_checkpoints(self, max_age_days: int = 7) -> int:
        """
        Clean up old checkpoints.
        
        Returns:
            Number of checkpoints deleted
        """
        cutoff_time = time.time() - (max_age_days * 24 * 60 * 60)
        deleted_count = 0
        
        # Find old checkpoints
        old_checkpoints = []
        for chk_id, chk_file in self.checkpoint_files.items():
            try:
                file_time = os.path.getctime(chk_file)
                if file_time < cutoff_time:
                    old_checkpoints.append(chk_id)
            except Exception:
                continue
                
        # Delete old checkpoints
        for chk_id in old_checkpoints:
            if self.delete_checkpoint(chk_id):
                deleted_count += 1
                
        logger.info(f"Cleaned up {deleted_count} old checkpoints")
        return deleted_count
        
    def get_checkpoint_stats(self) -> Dict[str, Any]:
        """Get statistics about checkpoints."""
        total_checkpoints = len(self.checkpoint_files)
        total_size = sum(os.path.getsize(f) for f in self.checkpoint_files.values() if os.path.exists(f))
        
        # Group by task
        task_counts = {}
        for chk_id, chk_file in self.checkpoint_files.items():
            try:
                with open(chk_file, 'r') as f:
                    data = json.load(f)
                task_id = data.get('task_id', 'unknown')
                task_counts[task_id] = task_counts.get(task_id, 0) + 1
            except Exception:
                continue
                
        return {
            'total_checkpoints': total_checkpoints,
            'total_size_bytes': total_size,
            'average_size_bytes': total_size / max(total_checkpoints, 1),
            'tasks_with_checkpoints': len(task_counts),
            'checkpoints_per_task': task_counts
        }

# Example usage function
def demo_checkpointing():
    """Demonstrate checkpointing functionality."""
    
    # Create checkpoint manager
    manager = TaskCheckpointManager("/tmp/jev_checkpoints_demo")
    
    print("=== Task Checkpointing Demo ===")
    
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
    for chk in checkpoints:
        print(f"  {chk['id']} - {datetime.fromtimestamp(chk['timestamp'])}")
    
    # Restore checkpoint
    restored = manager.restore_checkpoint(checkpoint_id)
    if restored:
        print(f"Restored checkpoint state: {restored.state}")
    
    # Get latest checkpoint for task
    latest = manager.get_latest_checkpoint("task_001")
    if latest:
        print(f"Latest checkpoint: {latest.checkpoint_id}")
    
    # Stats
    stats = manager.get_checkpoint_stats()
    print(f"Checkpoint stats: {stats}")
    
    # Cleanup
    manager.cleanup_old_checkpoints(0)  # Delete all
    print("Cleanup completed")

if __name__ == "__main__":
    demo_checkpointing()