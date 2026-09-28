"""
OpenAI Provider
Implementation of the OpenAI provider for Jev.
"""

import openai
import logging
from typing import Dict, Any, Optional, List
from providers.base import BaseProvider

logger = logging.getLogger(__name__)

class OpenAIProvider(BaseProvider):
    """
    OpenAI provider implementation.
    """
    
    def __init__(self, config: Dict[str, Any]):
        """
        Initialize OpenAI provider.
        
        Args:
            config: Provider configuration
        """
        super().__init__(config)
        
        self.api_key = config.get('api_key')
        self.base_url = config.get('base_url', 'https://api.openai.com/v1')
        
        # Initialize OpenAI client
        self.client = openai.OpenAI(
            api_key=self.api_key,
            base_url=self.base_url
        )
        
        logger.info("OpenAI provider initialized")
    
    def chat(self, messages: List[Dict[str, str]], model: str, 
             temperature: float = 0.7, **kwargs) -> Dict[str, Any]:
        """
        Send a chat message to OpenAI.
        
        Args:
            messages: List of message dictionaries
            model: Model name to use
            temperature: Sampling temperature
            **kwargs: Additional arguments
            
        Returns:
            Response from OpenAI
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
            logger.error(f"OpenAI chat error: {e}")
            return {
                'success': False,
                'error': str(e)
            }
    
    def stream(self, messages: List[Dict[str, str]], model: str, 
               temperature: float = 0.7, **kwargs) -> Any:
        """
        Stream response from OpenAI.
        
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
            logger.error(f"OpenAI stream error: {e}")
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
            Estimated cost in USD
        """
        # Pricing for gpt-4-turbo (example)
        input_price = 0.01 / 1000  # $0.01 per 1K tokens
        output_price = 0.03 / 1000  # $0.03 per 1K tokens
        
        return (input_tokens * input_price) + (output_tokens * output_price)
    
    def health(self) -> bool:
        """Check provider health."""
        try:
            # Simple health check
            self.client.models.list()
            return True
        except Exception:
            return False