"""
OpenAI-Compatible Provider
Implementation of an OpenAI-compatible provider for Jev.
"""

import openai
import logging
from typing import Dict, Any, Optional, List
from providers.base import BaseProvider

logger = logging.getLogger(__name__)

class OpenAICompatibleProvider(BaseProvider):
    """
    OpenAI-compatible provider implementation.
    """
    
    def __init__(self, config: Dict[str, Any]):
        """
        Initialize OpenAI-compatible provider.
        
        Args:
            config: Provider configuration
        """
        super().__init__(config)
        
        self.base_url = config.get('base_url')
        self.api_key = config.get('api_key', 'placeholder')  # Some servers don't require API keys
        
        # Initialize OpenAI client with custom base URL
        self.client = openai.OpenAI(
            api_key=self.api_key,
            base_url=self.base_url
        )
        
        logger.info(f"OpenAI-compatible provider initialized with base URL: {self.base_url}")
    
    def chat(self, messages: List[Dict[str, str]], model: str, 
             temperature: float = 0.7, **kwargs) -> Dict[str, Any]:
        """
        Send a chat message to the compatible API.
        
        Args:
            messages: List of message dictionaries
            model: Model name to use
            temperature: Sampling temperature
            **kwargs: Additional arguments
            
        Returns:
            Response from the compatible API
        """
        try:
            response = self.client.chat.completions.create(
                model=model,
                messages=messages,
                temperature=temperature,
                **kwargs
            )
            
            return {
                'success': True,
                'response': response.choices[0].message.content,
                'usage': {
                    'prompt_tokens': response.usage.prompt_tokens,
                    'completion_tokens': response.usage.completion_tokens,
                    'total_tokens': response.usage.total_tokens
                }
            }
        except Exception as e:
            logger.error(f"OpenAI-compatible chat error: {e}")
            return {
                'success': False,
                'error': str(e)
            }
    
    def stream(self, messages: List[Dict[str, str]], model: str, 
               temperature: float = 0.7, **kwargs) -> Any:
        """
        Stream response from the compatible API.
        
        Args:
            messages: List of message dictionaries
            model: Model name to use
            temperature: Sampling temperature
            **kwargs: Additional arguments
            
        Returns:
            Streaming response
        """
        try:
            stream = self.client.chat.completions.create(
                model=model,
                messages=messages,
                temperature=temperature,
                stream=True,
                **kwargs
            )
            
            return stream
        except Exception as e:
            logger.error(f"OpenAI-compatible stream error: {e}")
            raise
    
    def supports_tools(self) -> bool:
        """Check if provider supports tools."""
        return True
    
    def supports_vision(self) -> bool:
        """Check if provider supports vision."""
        return True
    
    def supports_reasoning(self) -> bool:
        """Check if provider supports reasoning."""
        return True
    
    def context_window(self) -> int:
        """Get context window size."""
        # Default to 128k for most models
        return 128000
    
    def estimated_cost(self, input_tokens: int, output_tokens: int) -> float:
        """
        Estimate cost for the given token usage.
        
        Args:
            input_tokens: Number of input tokens
            output_tokens: Number of output tokens
            
        Returns:
            Estimated cost in USD (placeholder)
        """
        # Placeholder pricing - actual implementation would depend on the specific server
        input_price = 0.00  # Free for local servers
        output_price = 0.00  # Free for local servers
        
        return (input_tokens * input_price) + (output_tokens * output_price)
    
    def health(self) -> bool:
        """Check provider health."""
        try:
            # Simple health check
            self.client.models.list()
            return True
        except Exception:
            return False