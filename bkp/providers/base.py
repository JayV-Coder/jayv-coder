"""
Base Provider
Base class for all AI providers.
"""

from abc import ABC, abstractmethod
from typing import Dict, Any, Optional, List

class BaseProvider(ABC):
    """
    Abstract base class for all AI providers.
    """
    
    def __init__(self, config: Dict[str, Any]):
        """
        Initialize the provider.
        
        Args:
            config: Provider configuration
        """
        self.config = config
        self.name = config.get('name', 'unnamed')
    
    @abstractmethod
    def chat(self, messages: List[Dict[str, str]], model: str, 
             temperature: float = 0.7, **kwargs) -> Dict[str, Any]:
        """
        Send a chat message to the provider.
        
        Args:
            messages: List of message dictionaries
            model: Model name to use
            temperature: Sampling temperature
            **kwargs: Additional arguments
            
        Returns:
            Response dictionary
        """
        pass
    
    @abstractmethod
    def stream(self, messages: List[Dict[str, str]], model: str, 
               temperature: float = 0.7, **kwargs) -> Any:
        """
        Stream response from the provider.
        
        Args:
            messages: List of message dictionaries
            model: Model name to use
            temperature: Sampling temperature
            **kwargs: Additional arguments
            
        Returns:
            Streaming response
        """
        pass
    
    @abstractmethod
    def supports_tools(self) -> bool:
        """Check if provider supports tools."""
        pass
    
    @abstractmethod
    def supports_vision(self) -> bool:
        """Check if provider supports vision."""
        pass
    
    @abstractmethod
    def supports_reasoning(self) -> bool:
        """Check if provider supports reasoning."""
        pass
    
    @abstractmethod
    def context_window(self) -> int:
        """Get context window size."""
        pass
    
    @abstractmethod
    def estimated_cost(self, input_tokens: int, output_tokens: int) -> float:
        """
        Estimate cost for the given token usage.
        
        Args:
            input_tokens: Number of input tokens
            output_tokens: Number of output tokens
            
        Returns:
            Estimated cost in USD
        """
        pass
    
    @abstractmethod
    def health(self) -> bool:
        """Check provider health."""
        pass