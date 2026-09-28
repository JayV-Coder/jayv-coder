"""
CLI Provider
Implementation of a CLI-based provider for Jev.
"""

import subprocess
import logging
import json
from typing import Dict, Any, Optional, List
from providers.base import BaseProvider

logger = logging.getLogger(__name__)

class CLIProvider(BaseProvider):
    """
    CLI provider implementation for local AI tools.
    """
    
    def __init__(self, config: Dict[str, Any]):
        """
        Initialize CLI provider.
        
        Args:
            config: Provider configuration
        """
        super().__init__(config)
        
        self.command = config.get('command', 'claude')
        self.auto_detect = config.get('auto_detect', False)
        
        # Check if CLI is available if auto-detection is enabled
        if self.auto_detect:
            self._detect_cli()
        
        logger.info(f"CLI provider initialized with command: {self.command}")
    
    def _detect_cli(self) -> bool:
        """
        Detect if the CLI tool is available.
        
        Returns:
            True if detected, False otherwise
        """
        try:
            # Try to run the command to see if it exists
            result = subprocess.run([self.command, '--version'], 
                                  capture_output=True, text=True, timeout=5)
            if result.returncode == 0:
                logger.info(f"Detected CLI tool: {self.command}")
                return True
            else:
                logger.warning(f"CLI tool {self.command} not available")
                return False
        except (subprocess.TimeoutExpired, FileNotFoundError):
            logger.warning(f"CLI tool {self.command} not found")
            return False
        except Exception as e:
            logger.warning(f"Error detecting CLI tool {self.command}: {e}")
            return False
    
    def chat(self, messages: List[Dict[str, str]], model: str, 
             temperature: float = 0.7, **kwargs) -> Dict[str, Any]:
        """
        Execute CLI command with the given parameters.
        
        Args:
            messages: List of message dictionaries
            model: Model name to use (passed as argument to CLI)
            temperature: Sampling temperature (passed as argument to CLI)
            **kwargs: Additional arguments
            
        Returns:
            Response from CLI
        """
        try:
            # Prepare CLI arguments
            cmd = [self.command]
            
            # Add model if specified
            if model:
                cmd.extend(['--model', model])
            
            # Add temperature if specified
            cmd.extend(['--temperature', str(temperature)])
            
            # Add system prompt if present
            system_prompt = next((msg['content'] for msg in messages if msg['role'] == 'system'), None)
            if system_prompt:
                cmd.extend(['--system', system_prompt])
            
            # Add user messages
            user_messages = [msg for msg in messages if msg['role'] != 'system']
            if user_messages:
                # For simplicity, we'll join all user messages
                user_content = '\n'.join(msg['content'] for msg in user_messages)
                cmd.extend(['--prompt', user_content])
            
            # Execute CLI command
            result = subprocess.run(cmd, capture_output=True, text=True, timeout=60)
            
            if result.returncode == 0:
                return {
                    'success': True,
                    'response': result.stdout,
                    'stderr': result.stderr
                }
            else:
                return {
                    'success': False,
                    'error': result.stderr,
                    'stdout': result.stdout
                }
        except subprocess.TimeoutExpired:
            logger.error("CLI command timed out")
            return {
                'success': False,
                'error': 'Command timed out'
            }
        except Exception as e:
            logger.error(f"CLI execution error: {e}")
            return {
                'success': False,
                'error': str(e)
            }
    
    def stream(self, messages: List[Dict[str, str]], model: str, 
               temperature: float = 0.7, **kwargs) -> Any:
        """
        Stream response from CLI (not implemented for now).
        
        Args:
            messages: List of message dictionaries
            model: Model name to use
            temperature: Sampling temperature
            **kwargs: Additional arguments
            
        Returns:
            Streaming response (placeholder)
        """
        # For CLI providers, streaming is not supported in this basic implementation
        # This would require more sophisticated handling of streaming output
        return self.chat(messages, model, temperature, **kwargs)
    
    def supports_tools(self) -> bool:
        """Check if provider supports tools."""
        # This depends on the specific CLI tool - for now assume false
        return False
    
    def supports_vision(self) -> bool:
        """Check if provider supports vision."""
        # This depends on the specific CLI tool - for now assume false
        return False
    
    def supports_reasoning(self) -> bool:
        """Check if provider supports reasoning."""
        # This depends on the specific CLI tool - for now assume true
        return True
    
    def context_window(self) -> int:
        """Get context window size."""
        # CLI tools typically don't have strict context windows
        # but we'll set a reasonable limit
        return 32000
    
    def estimated_cost(self, input_tokens: int, output_tokens: int) -> float:
        """
        Estimate cost for the given token usage.
        
        Args:
            input_tokens: Number of input tokens
            output_tokens: Number of output tokens
            
        Returns:
            Estimated cost in USD (typically zero for local tools)
        """
        # Local CLI tools are usually free
        return 0.0
    
    def health(self) -> bool:
        """Check provider health."""
        try:
            if self.auto_detect:
                return self._detect_cli()
            else:
                # If auto-detect is disabled, just check if command is set
                return bool(self.command)
        except Exception:
            return False