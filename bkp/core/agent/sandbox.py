"""
Agent Sandbox for Jev AI Orchestrator
Provides isolation and security for agent execution.
"""

import os
import tempfile
import shutil
import subprocess
import signal
import time
from typing import Dict, List, Any, Optional, Callable
from dataclasses import dataclass, field
from pathlib import Path
import logging

logger = logging.getLogger(__name__)

@dataclass
class SandboxConfig:
    """Configuration for agent sandbox."""
    
    # Resource limits
    max_memory_mb: int = 512
    max_cpu_percent: float = 50.0
    max_time_seconds: int = 300
    
    # File system restrictions
    allowed_paths: List[str] = field(default_factory=list)
    denied_paths: List[str] = field(default_factory=list)
    temp_directory: str = "/tmp/jev_sandbox"
    
    # Network restrictions
    network_access: bool = False
    allowed_hosts: List[str] = field(default_factory=list)
    
    # Process restrictions
    allow_spawn_processes: bool = False
    max_processes: int = 10
    
    # Environment restrictions
    restrict_environment: bool = True
    allowed_env_vars: List[str] = field(default_factory=list)
    
    # Security settings
    enable_logging: bool = True
    enable_monitoring: bool = True

class AgentSandbox:
    """Provides isolated execution environment for agents."""
    
    def __init__(self, config: SandboxConfig = None):
        self.config = config or SandboxConfig()
        self.temp_dir = Path(self.config.temp_directory)
        self.temp_dir.mkdir(parents=True, exist_ok=True)
        self.running_processes = {}
        self.process_counter = 0
        
    def create_isolated_workspace(self, task_id: str) -> str:
        """Create isolated workspace for task."""
        workspace_dir = self.temp_dir / f"workspace_{task_id}"
        workspace_dir.mkdir(parents=True, exist_ok=True)
        return str(workspace_dir)
        
    def destroy_workspace(self, workspace_path: str) -> bool:
        """Destroy isolated workspace."""
        try:
            if os.path.exists(workspace_path):
                shutil.rmtree(workspace_path)
                return True
        except Exception as e:
            logger.error(f"Failed to destroy workspace {workspace_path}: {e}")
        return False
        
    def execute_in_sandbox(self, command: List[str], 
                          workspace: str = None,
                          timeout: int = None,
                          env_vars: Dict[str, str] = None) -> Dict[str, Any]:
        """
        Execute command in sandboxed environment.
        
        Returns:
            Dict with execution results:
            {
                'success': bool,
                'stdout': str,
                'stderr': str,
                'return_code': int,
                'execution_time': float,
                'memory_used': int,
                'error': str
            }
        """
        start_time = time.time()
        
        try:
            # Prepare environment
            env = os.environ.copy()
            if env_vars:
                env.update(env_vars)
                
            # Set resource limits
            if self.config.restrict_environment:
                # Restrict environment variables
                if self.config.allowed_env_vars:
                    restricted_env = {}
                    for var in self.config.allowed_env_vars:
                        if var in env:
                            restricted_env[var] = env[var]
                    env = restricted_env
                else:
                    # Clear most environment variables for security
                    env = {'PATH': env.get('PATH', '/usr/bin:/bin')}
            
            # Set working directory
            cwd = workspace if workspace else str(self.temp_dir)
            
            # Execute command
            process = subprocess.Popen(
                command,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                cwd=cwd,
                env=env,
                text=True,
                preexec_fn=self._setup_process_limits if timeout else None
            )
            
            # Wait for completion or timeout
            stdout, stderr = process.communicate(timeout=timeout if timeout else 300)
            
            execution_time = time.time() - start_time
            
            return {
                'success': process.returncode == 0,
                'stdout': stdout,
                'stderr': stderr,
                'return_code': process.returncode,
                'execution_time': execution_time,
                'memory_used': 0,  # Would require additional monitoring
                'error': None
            }
            
        except subprocess.TimeoutExpired:
            try:
                process.kill()
                stdout, stderr = process.communicate()
            except Exception as e:
                logger.error(f"Error killing timed-out process: {e}")
                stdout, stderr = "", str(e)
                
            execution_time = time.time() - start_time
            return {
                'success': False,
                'stdout': stdout,
                'stderr': stderr,
                'return_code': -1,
                'execution_time': execution_time,
                'memory_used': 0,
                'error': 'Command timed out'
            }
        except Exception as e:
            execution_time = time.time() - start_time
            return {
                'success': False,
                'stdout': '',
                'stderr': str(e),
                'return_code': -1,
                'execution_time': execution_time,
                'memory_used': 0,
                'error': str(e)
            }
            
    def _setup_process_limits(self):
        """Setup process limits for sandbox execution."""
        # This is a simplified implementation
        # In production, you'd use cgroups or similar
        pass
        
    def create_process_monitor(self, process_id: str) -> Dict[str, Any]:
        """Create monitoring entry for process."""
        monitor = {
            'process_id': process_id,
            'start_time': time.time(),
            'resource_usage': {
                'cpu_percent': 0.0,
                'memory_mb': 0.0,
                'disk_usage': 0.0
            },
            'status': 'running'
        }
        self.running_processes[process_id] = monitor
        return monitor
        
    def update_process_monitor(self, process_id: str, **kwargs) -> bool:
        """Update monitoring information for process."""
        if process_id in self.running_processes:
            for key, value in kwargs.items():
                if key in self.running_processes[process_id]['resource_usage']:
                    self.running_processes[process_id]['resource_usage'][key] = value
            return True
        return False
        
    def get_process_status(self, process_id: str) -> Optional[Dict[str, Any]]:
        """Get status of monitored process."""
        return self.running_processes.get(process_id)
        
    def cleanup(self) -> None:
        """Cleanup sandbox resources."""
        try:
            # Kill any remaining processes
            for process_id in list(self.running_processes.keys()):
                if process_id in self.running_processes:
                    del self.running_processes[process_id]
                    
            # Cleanup temp directory (but not the base directory)
            if self.temp_dir.exists():
                for item in self.temp_dir.iterdir():
                    if item.is_dir() and item.name.startswith('workspace_'):
                        try:
                            shutil.rmtree(item)
                        except Exception as e:
                            logger.warning(f"Failed to cleanup workspace {item}: {e}")
                            
        except Exception as e:
            logger.error(f"Error during sandbox cleanup: {e}")

# Example usage function
def demo_sandbox():
    """Demonstrate sandbox functionality."""
    
    # Create sandbox with basic configuration
    config = SandboxConfig(
        max_memory_mb=256,
        max_time_seconds=60,
        network_access=False,
        allow_spawn_processes=False,
        temp_directory="/tmp/jev_sandbox_demo"
    )
    
    sandbox = AgentSandbox(config)
    
    print("=== Agent Sandbox Demo ===")
    
    # Create workspace
    workspace = sandbox.create_isolated_workspace("demo_task")
    print(f"Created workspace: {workspace}")
    
    # Test execution
    result = sandbox.execute_in_sandbox(
        ['echo', 'Hello from sandbox!'],
        workspace=workspace,
        timeout=10
    )
    
    print(f"Execution result: {result}")
    
    # Test with error
    result = sandbox.execute_in_sandbox(
        ['ls', '/nonexistent/path'],
        workspace=workspace,
        timeout=10
    )
    
    print(f"Error execution result: {result}")
    
    # Cleanup
    sandbox.destroy_workspace(workspace)
    print("Workspace destroyed")
    
    # Cleanup sandbox
    sandbox.cleanup()
    print("Sandbox cleaned up")

if __name__ == "__main__":
    demo_sandbox()