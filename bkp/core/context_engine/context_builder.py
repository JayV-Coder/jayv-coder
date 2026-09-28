"""
Context Builder for Jev AI Orchestrator
Builds context objects for tasks and agents.
"""

from typing import Dict, List, Any, Optional
from dataclasses import dataclass
from pathlib import Path

@dataclass
class ContextData:
    """Represents structured context data."""
    
    files: List[str]
    dependencies: List[str]
    content: str
    metadata: Dict[str, Any]

class ContextBuilder:
    """Builds context objects for tasks and agents."""
    
    def __init__(self):
        self.context_templates = {}
        
    def build_context(self, task_data: Dict[str, Any], 
                     repository_context: Dict[str, Any] = None) -> ContextData:
        """
        Build context for a task.
        
        Args:
            task_data: Data about the task
            repository_context: Context from repository
            
        Returns:
            ContextData object
        """
        # Extract files and dependencies from task data
        files = task_data.get('files', [])
        dependencies = task_data.get('dependencies', [])
        content = task_data.get('content', '')
        
        # Merge with repository context if provided
        if repository_context:
            files.extend(repository_context.get('files', []))
            dependencies.extend(repository_context.get('dependencies', []))
            content += "\n" + repository_context.get('content', '')
        
        # Remove duplicates while preserving order
        seen_files = set()
        unique_files = []
        for file_path in files:
            if file_path not in seen_files:
                seen_files.add(file_path)
                unique_files.append(file_path)
        
        seen_deps = set()
        unique_dependencies = []
        for dep in dependencies:
            if dep not in seen_deps:
                seen_deps.add(dep)
                unique_dependencies.append(dep)
        
        return ContextData(
            files=unique_files,
            dependencies=unique_dependencies,
            content=content,
            metadata=task_data.get('metadata', {})
        )
        
    def build_context_from_files(self, files: List[str], 
                                base_path: str = ".") -> ContextData:
        """
        Build context from a list of file paths.
        
        Args:
            files: List of file paths
            base_path: Base path for resolving relative paths
            
        Returns:
            ContextData object
        """
        # In a real implementation, this would read file contents
        # For now, we'll simulate it
        content_parts = []
        for file_path in files:
            # This is a placeholder - in real implementation, you'd read the file
            content_parts.append(f"# Content from {file_path}\n")
            content_parts.append(f"// This is a simulated file content for {file_path}\n\n")
            
        content = "".join(content_parts)
        
        return ContextData(
            files=files,
            dependencies=[],
            content=content,
            metadata={'base_path': base_path}
        )

# Example usage function
def demo_context_builder():
    """Demonstrate context builder functionality."""
    
    builder = ContextBuilder()
    
    # Build context from task data
    task_data = {
        'files': ['auth.service.ts', 'token.service.ts'],
        'dependencies': ['AGENTS.md#backend'],
        'content': 'Authentication service implementation',
        'metadata': {'task_type': 'auth'}
    }
    
    context = builder.build_context(task_data)
    print(f"Built context with {len(context.files)} files")
    print(f"Content preview: {context.content[:50]}...")
    
    # Build context from files
    files = ['src/main.py', 'src/utils.py']
    file_context = builder.build_context_from_files(files)
    print(f"File context with {len(file_context.files)} files")
    print(f"Content preview: {file_context.content[:50]}...")

if __name__ == "__main__":
    demo_context_builder()