"""
Memory Manager
Handles different types of memory for the orchestrator.
"""

import logging
from typing import Dict, Any, Optional
from datetime import datetime

logger = logging.getLogger(__name__)

class MemoryManager:
    """
    Manages different types of memory for the orchestrator.
    """
    
    def __init__(self):
        """Initialize the memory manager."""
        self.working_memory = {}  # Current execution only
        self.session_memory = {}  # Current chat session
        self.project_memory = {}  # Stable project information
        self.semantic_memory = {}  # RAG-indexed knowledge
        
        logger.info("Memory Manager initialized")
    
    def store_working_memory(self, key: str, value: Any):
        """
        Store data in working memory.
        
        Args:
            key: Memory key
            value: Value to store
        """
        self.working_memory[key] = {
            'value': value,
            'timestamp': datetime.now()
        }
        logger.debug(f"Stored in working memory: {key}")
    
    def retrieve_working_memory(self, key: str) -> Optional[Any]:
        """
        Retrieve data from working memory.
        
        Args:
            key: Memory key
            
        Returns:
            Stored value or None if not found
        """
        item = self.working_memory.get(key)
        if item:
            return item['value']
        return None
    
    def store_session_memory(self, key: str, value: Any):
        """
        Store data in session memory.
        
        Args:
            key: Memory key
            value: Value to store
        """
        self.session_memory[key] = {
            'value': value,
            'timestamp': datetime.now()
        }
        logger.debug(f"Stored in session memory: {key}")
    
    def retrieve_session_memory(self, key: str) -> Optional[Any]:
        """
        Retrieve data from session memory.
        
        Args:
            key: Memory key
            
        Returns:
            Stored value or None if not found
        """
        item = self.session_memory.get(key)
        if item:
            return item['value']
        return None
    
    def store_project_memory(self, key: str, value: Any):
        """
        Store data in project memory.
        
        Args:
            key: Memory key
            value: Value to store
        """
        self.project_memory[key] = {
            'value': value,
            'timestamp': datetime.now()
        }
        logger.debug(f"Stored in project memory: {key}")
    
    def retrieve_project_memory(self, key: str) -> Optional[Any]:
        """
        Retrieve data from project memory.
        
        Args:
            key: Memory key
            
        Returns:
            Stored value or None if not found
        """
        item = self.project_memory.get(key)
        if item:
            return item['value']
        return None
    
    def store_semantic_memory(self, key: str, value: Any):
        """
        Store data in semantic memory.
        
        Args:
            key: Memory key
            value: Value to store
        """
        self.semantic_memory[key] = {
            'value': value,
            'timestamp': datetime.now()
        }
        logger.debug(f"Stored in semantic memory: {key}")
    
    def retrieve_semantic_memory(self, key: str) -> Optional[Any]:
        """
        Retrieve data from semantic memory.
        
        Args:
            key: Memory key
            
        Returns:
            Stored value or None if not found
        """
        item = self.semantic_memory.get(key)
        if item:
            return item['value']
        return None
    
    def get_session_summary(self) -> Dict[str, Any]:
        """
        Get a summary of the current session.
        
        Returns:
            Session summary dictionary
        """
        return {
            'working_memory_keys': list(self.working_memory.keys()),
            'session_memory_keys': list(self.session_memory.keys()),
            'project_memory_keys': list(self.project_memory.keys()),
            'semantic_memory_keys': list(self.semantic_memory.keys()),
            'session_started': self.session_memory.get('session_start', datetime.now())
        }
    
    def clear_session_memory(self):
        """Clear session memory."""
        self.session_memory.clear()
        logger.info("Cleared session memory")