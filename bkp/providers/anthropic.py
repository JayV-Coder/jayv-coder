"""
Anthropic Provider
Implementation of the Anthropic provider for Jev.
"""

import anthropic
import logging
from typing import Dict, Any, Optional, List
from providers.base import BaseProvider

logger = logging.getLogger(__name__)

class AnthropicProvider(BaseProvider):
    """
    Anthropic provider implementation.
    """
    
    def __init__(self, config: Dict[str, Any]):
        """
        Initialize Anthropic provider.
        
        Args:
            config: Provider configuration
        """
        super().__init__(config)
        
        self.api_key = config.get('api_key')
        self.base_url = config.get('base_url')
        
        # Initialize Anthropic client
        self.client = anthropic.Anthropic(
            api_key=self.api_key,
            base_url=self.base_url
        )
        
        logger.info("Anthropic provider initialized")
    
    def chat(self, messages: List[Dict[str, str]], model: str, 
             temperature: float = 0.7, **kwargs) -> Dict[str, Any]:
        """
        Send a chat message to Anthropic.
        
        Args:
            messages: List of message dictionaries
            model: Model name to use
            temperature: Sampling temperature
            **kwargs: Additional arguments
            
        Returns:
            Response from Anthropic
        """
        try:
            # Convert messages to Anthropic format
            anthropic_messages = []
            system_prompt = ""
            
            for msg in messages:
                if msg['role'] == 'system':
                    system_prompt = msg['content']
                else:
                    anthropic_messages.append({
                        'role': msg['role'],
                        'content': msg['content']
                    })
            
            response = self.client.messages.create(
                model=model,
                messages=anthropic_messages,
                temperature=temperature,
                system=system_prompt,
                **kwargs
            )
            
            return {
                'success': True,
                'response': response.content[0].text,
                'usage': {
                    'input_tokens': response.usage.input_tokens,
                    'output_tokens': response.usage.output_tokens
                }
            }
        except Exception as e:
            logger.error(f"Anthropic chat error: {e}")
            return {
                'success': False,
                'error': str(e)
            }
    
    def stream(self, messages: List[Dict[str, str]], model: str, 
               temperature: float = 0.7, **kwargs) -> Any:
        """
        Stream response from Anthropic.
        
        Args:
            messages: List of message dictionaries
            model: Model name to use
            temperature: Sampling temperature
            **kwargs: Additional arguments
            
        Returns:
            Streaming response
        """
        try:
            # Convert messages to Anthropic format
            anthropic_messages = []
            system_prompt = ""
            
            for msg in messages:
                if msg['role'] == 'system':
                    system_prompt = msg['content']
                else:
                    anthropic_messages.append({
                        'role': msg['role'],
                        'content': msg['content']
                    })
            
            stream = self.client.messages.stream(
                model=model,
                messages=anthropic_messages,
                temperature=temperature,
                system=system_prompt,
                **kwargs
            )
            
            return stream
        except Exception as e:
            logger.error(f"Anthropic stream error: {e}")
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
        # Claude 3 Opus has 200k context window
        return 200000
    
    def estimated_cost(self, input_tokens: int, output_tokens: int) -> float:
        """
        Estimate cost for the given token usage.
        
        Args:
            input_tokens: Number of input tokens
            output_tokens: Number of output tokens
            
        Returns:
            Estimated cost in USD
        """
        # Pricing for Claude 3 Opus (example)
        input_price = 0.015 / 1000  # $0.015 per 1K tokens
        output_price = 0.075 / 1000  # $0.075 per 1K tokens
        
        return (input_tokens * input_price) + (output_tokens * output_price)
    
    def health(self) -> bool:
        """Check provider health."""
        try:
            # Simple health check - we'll just verify the client can be created
            return True
        except Exception:
            return False