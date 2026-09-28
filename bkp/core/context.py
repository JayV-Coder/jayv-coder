"""
Context Builder
Responsible for building the minimal useful context for a model.
"""

import logging
from typing import Dict, Any, List
from pathlib import Path

logger = logging.getLogger(__name__)

class ContextBuilder:
    """
    Builds minimal useful context for models based on task requirements.
    """
    
    def __init__(self):
        """Initialize the context builder."""
        logger.info("Context Builder initialized")
    
    def build_context(self, user_input: str, intent: Dict[str, Any], 
                     complexity: str, context_plan: List[str]) -> Dict[str, Any]:
        """
        Build the context for a model based on task requirements.
        
        Args:
            user_input: Original user input
            intent: Intent analysis results
            complexity: Task complexity
            context_plan: Planned context elements
            
        Returns:
            Dictionary with built context
        """
        logger.info("Building context for model")
        
        # Get RAG context if needed
        rag_context = None
        if 'repository_context' in context_plan:
            # In a real implementation, we'd get this from the RAG system
            rag_context = {
                'files': ['src/main.py', 'src/utils.py'],
                'summary': 'Project contains main application and utility functions'
            }
        
        context = {
            'user_input': user_input,
            'intent': intent,
            'complexity': complexity,
            'context_plan': context_plan,
            'system_instructions': self._get_system_instructions(),
            'project_info': self._get_project_info(),
            'relevant_files': self._get_relevant_files(context_plan),
            'rag_context': rag_context,
            'estimated_tokens': self._estimate_context_tokens(user_input, context_plan)
        }
        
        logger.debug(f"Built context with {len(context.get('relevant_files', []))} relevant files")
        return context
    
    def _get_system_instructions(self) -> str:
        """
        Get system instructions for the model.
        
        Returns:
            System instructions string
        """
        return """
You are an AI assistant helping developers with software development tasks.
Your goal is to provide accurate, helpful responses while minimizing token usage.
Always focus on the specific task requested and use only the context provided.
If you don't have enough information, ask for clarification.
When code is involved, follow best practices and coding standards.
"""
    
    def _get_project_info(self) -> Dict[str, Any]:
        """
        Get basic project information.
        
        Returns:
            Project information dictionary
        """
        # In a real implementation, this would gather actual project info
        return {
            'project_name': 'jev-project',
            'language': 'python',
            'framework': 'none',
            'version': '0.1.0'
        }
    
    def _get_relevant_files(self, context_plan: List[str]) -> List[str]:
        """
        Get list of relevant files based on context plan.
        
        Args:
            context_plan: Planned context elements
            
        Returns:
            List of relevant file paths
        """
        # In a real implementation, this would search for actual files
        # This is a simplified mock implementation
        
        relevant_files = []
        
        if 'code_files' in context_plan:
            relevant_files.extend([
                'src/main.py',
                'src/utils.py',
                'requirements.txt'
            ])
            
        if 'dependencies' in context_plan:
            relevant_files.extend([
                'package.json',
                'pyproject.toml',
                'setup.py'
            ])
            
        if 'architecture' in context_plan:
            relevant_files.extend([
                'docs/architecture.md',
                'README.md',
                'AGENTS.md'
            ])
            
        return list(set(relevant_files))  # Remove duplicates
    
    def _estimate_context_tokens(self, user_input: str, context_plan: List[str]) -> int:
        """
        Estimate the token count for the context.
        
        Args:
            user_input: Original user input
            context_plan: Planned context elements
            
        Returns:
            Estimated token count
        """
        # Rough estimation:
        # - User input: ~1 token per 4 characters
        # - System instructions: ~50 tokens
        # - Project info: ~20 tokens
        # - Files: ~100 tokens per file
        # - RAG context: ~200 tokens per file
        
        input_tokens = len(user_input) // 4
        system_tokens = 50
        project_tokens = 20
        files_tokens = len(self._get_relevant_files(context_plan)) * 100
        
        # Estimate RAG context tokens
        rag_tokens = 0
        if 'repository_context' in context_plan:
            rag_tokens = 200  # Approximate for each RAG file
            
        total = input_tokens + system_tokens + project_tokens + files_tokens + rag_tokens
        
        logger.debug(f"Estimated context tokens: {total}")
        return total