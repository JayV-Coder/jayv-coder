"""
Conversation Compressor
Handles compression of long conversations to reduce context size.
"""

import logging
from typing import List, Dict, Any
from datetime import datetime

logger = logging.getLogger(__name__)

class ConversationCompressor:
    """
    Compresses long conversations to reduce context size.
    """
    
    def __init__(self, max_history: int = 20):
        """
        Initialize the conversation compressor.
        
        Args:
            max_history: Maximum number of messages to keep in history
        """
        self.max_history = max_history
        self.conversation_history = []
        self.summary_cache = {}
        
        logger.info(f"Conversation Compressor initialized (max history: {max_history})")
    
    def add_message(self, role: str, content: str, timestamp: datetime = None):
        """
        Add a message to conversation history.
        
        Args:
            role: Role of the speaker ('user' or 'assistant')
            content: Message content
            timestamp: Timestamp of the message
        """
        if timestamp is None:
            timestamp = datetime.now()
            
        message = {
            'role': role,
            'content': content,
            'timestamp': timestamp
        }
        
        self.conversation_history.append(message)
        
        # Keep only the most recent messages
        if len(self.conversation_history) > self.max_history:
            self.conversation_history.pop(0)
        
        logger.debug(f"Added message from {role}")
    
    def get_compressed_context(self, current_user_input: str, 
                              max_summary_length: int = 500) -> Dict[str, Any]:
        """
        Get compressed context for the current request.
        
        Args:
            current_user_input: Current user input
            max_summary_length: Maximum length of summary
            
        Returns:
            Dictionary with compressed context
        """
        # Create a summary of the conversation history
        summary = self._create_conversation_summary(max_summary_length)
        
        # Create the current context
        context = {
            'conversation_summary': summary,
            'current_input': current_user_input,
            'recent_messages': self.conversation_history[-5:]  # Last 5 messages
        }
        
        return context
    
    def _create_conversation_summary(self, max_length: int = 500) -> str:
        """
        Create a summary of the conversation history.
        
        Args:
            max_length: Maximum length of the summary
            
        Returns:
            Summary text
        """
        # If we don't have enough messages, return nothing
        if len(self.conversation_history) < 3:
            return ""
        
        # In a real implementation, this would use a model to summarize
        # For now, we'll create a simple summary based on key points
        
        # Extract key points from conversation
        key_points = []
        for msg in self.conversation_history:
            if msg['role'] == 'user':
                # Extract key terms from user messages
                content = msg['content'].lower()
                if 'fix' in content or 'problem' in content or 'issue' in content:
                    key_points.append(f"User reported: {msg['content'][:100]}...")
                elif 'help' in content or 'how' in content:
                    key_points.append(f"User asked for help with: {msg['content'][:100]}...")
                elif 'test' in content or 'unit' in content:
                    key_points.append(f"User requested: {msg['content'][:100]}...")
        
        # If we have key points, create a summary
        if key_points:
            summary = "Recent conversation highlights:\n" + "\n".join(key_points[:3])
            if len(summary) > max_length:
                summary = summary[:max_length-3] + "..."
            return summary
        else:
            # Create a basic summary
            return f"Recent conversation with {len(self.conversation_history)} exchanges"
    
    def get_session_summary(self) -> Dict[str, Any]:
        """
        Get a summary of the current session.
        
        Returns:
            Session summary dictionary
        """
        return {
            'total_messages': len(self.conversation_history),
            'recent_messages': len(self.conversation_history),
            'first_message': self.conversation_history[0]['timestamp'] if self.conversation_history else None,
            'last_message': self.conversation_history[-1]['timestamp'] if self.conversation_history else None
        }
    
    def clear_history(self):
        """Clear conversation history."""
        self.conversation_history.clear()
        self.summary_cache.clear()
        logger.info("Conversation history cleared")