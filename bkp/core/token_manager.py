"""
Token Budget Manager
Manages token budgets and consumption tracking.
"""

import logging
from typing import Dict, Any, Optional
from dataclasses import dataclass
from datetime import datetime

logger = logging.getLogger(__name__)

@dataclass
class TokenUsage:
    """Data class to track token usage."""
    input_tokens: int = 0
    output_tokens: int = 0
    total_tokens: int = 0
    timestamp: datetime = None
    
    def __post_init__(self):
        if self.timestamp is None:
            self.timestamp = datetime.now()

class TokenBudgetManager:
    """
    Manages token budgets and consumption tracking for sessions.
    """
    
    def __init__(self, config: Dict[str, Any]):
        """
        Initialize the token budget manager.
        
        Args:
            config: Configuration dictionary
        """
        self.config = config
        self.budgets = config.get('budgets', {})
        self.session_tokens = {
            'input': 0,
            'output': 0,
            'total': 0
        }
        self.session_start_time = datetime.now()
        self.request_count = 0
        self.token_history = []
        
        logger.info("Token Budget Manager initialized")
    
    def estimate_input_tokens(self, text: str) -> int:
        """
        Estimate input token count for text.
        
        Args:
            text: Text to estimate tokens for
            
        Returns:
            Estimated token count
        """
        # Rough estimation: 1 token ≈ 4 characters (average)
        return len(text) // 4
    
    def estimate_output_tokens(self, text: str) -> int:
        """
        Estimate output token count for text.
        
        Args:
            text: Text to estimate tokens for
            
        Returns:
            Estimated token count
        """
        # Rough estimation: 1 token ≈ 4 characters (average)
        return len(text) // 4
    
    def estimate_cost(self, input_tokens: int, output_tokens: int, 
                     model_name: str = None) -> float:
        """
        Estimate cost for token usage.
        
        Args:
            input_tokens: Number of input tokens
            output_tokens: Number of output tokens
            model_name: Name of the model used
            
        Returns:
            Estimated cost in USD
        """
        # Default pricing (these would be model-specific in reality)
        input_price = 0.001  # $0.001 per 1K tokens
        output_price = 0.002  # $0.002 per 1K tokens
        
        # Adjust based on model if specified
        if model_name and 'gpt-4' in model_name:
            input_price = 0.03  # Higher for GPT-4
            output_price = 0.06
        elif model_name and 'claude' in model_name:
            input_price = 0.015  # Claude pricing
            output_price = 0.075
            
        cost = (input_tokens * input_price / 1000) + (output_tokens * output_price / 1000)
        return cost
    
    def check_budget(self, input_tokens: int, output_tokens: int) -> bool:
        """
        Check if request would exceed budget.
        
        Args:
            input_tokens: Expected input tokens
            output_tokens: Expected output tokens
            
        Returns:
            True if within budget, False otherwise
        """
        total_tokens = input_tokens + output_tokens
        current_total = self.session_tokens['total']
        
        # Check against session budget
        session_budget = self.budgets.get('session', 100000)
        if current_total + total_tokens > session_budget:
            logger.warning(f"Would exceed session budget: {current_total + total_tokens} > {session_budget}")
            return False
            
        return True
    
    def add_usage(self, input_tokens: int, output_tokens: int, 
                 model_name: str = None, request_id: str = None):
        """
        Record token usage for a request.
        
        Args:
            input_tokens: Input tokens consumed
            output_tokens: Output tokens consumed
            model_name: Name of model used
            request_id: Request identifier
        """
        total_tokens = input_tokens + output_tokens
        
        self.session_tokens['input'] += input_tokens
        self.session_tokens['output'] += output_tokens
        self.session_tokens['total'] += total_tokens
        self.request_count += 1
        
        # Record in history
        usage_record = TokenUsage(
            input_tokens=input_tokens,
            output_tokens=output_tokens,
            total_tokens=total_tokens,
            timestamp=datetime.now()
        )
        
        self.token_history.append({
            'id': request_id or f"req_{len(self.token_history)}",
            'usage': usage_record,
            'model': model_name,
            'cost': self.estimate_cost(input_tokens, output_tokens, model_name)
        })
        
        logger.debug(f"Added usage: {input_tokens} input, {output_tokens} output")
    
    def get_session_summary(self) -> Dict[str, Any]:
        """
        Get summary of session token usage.
        
        Returns:
            Session summary dictionary
        """
        duration = datetime.now() - self.session_start_time
        duration_seconds = duration.total_seconds()
        
        return {
            'requests': self.request_count,
            'input_tokens': self.session_tokens['input'],
            'output_tokens': self.session_tokens['output'],
            'total_tokens': self.session_tokens['total'],
            'duration_seconds': duration_seconds,
            'tokens_per_second': self.session_tokens['total'] / max(duration_seconds, 1),
            'estimated_cost': self.estimate_cost(
                self.session_tokens['input'],
                self.session_tokens['output']
            ),
            'saved_by_rag': self.session_tokens['total'] * 0.8  # Placeholder for savings
        }
    
    def reset_session(self):
        """Reset session token counters."""
        self.session_tokens = {
            'input': 0,
            'output': 0,
            'total': 0
        }
        self.session_start_time = datetime.now()
        self.request_count = 0
        self.token_history = []
        
        logger.info("Session tokens reset")