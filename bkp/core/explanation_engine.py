"""
Explanation Engine
Provides explanations for Jev's decisions and routing.
"""

import logging
from typing import Dict, Any, List
from datetime import datetime

logger = logging.getLogger(__name__)

class ExplanationEngine:
    """
    Engine that provides explanations for Jev's decisions.
    """
    
    def __init__(self):
        """Initialize the explanation engine."""
        self.decision_log = []
        logger.info("Explanation Engine initialized")
    
    def log_decision(self, task: str, decision: Dict[str, Any]):
        """
        Log a decision made by Jev.
        
        Args:
            task: The task being processed
            decision: The decision made
        """
        log_entry = {
            'task': task,
            'decision': decision,
            'timestamp': datetime.now()
        }
        self.decision_log.append(log_entry)
        
        logger.debug(f"Logged decision for task: {task}")
    
    def explain_last_decision(self) -> str:
        """
        Generate explanation for the last decision.
        
        Returns:
            Formatted explanation string
        """
        if not self.decision_log:
            return "No decisions logged yet."
        
        # Get the most recent decision
        last_decision = self.decision_log[-1]
        decision = last_decision['decision']
        
        explanation = f"Task: {last_decision['task']}\n"
        explanation += f"Complexity: {decision.get('complexity', 'unknown')}\n\n"
        
        explanation += "Selected:\n"
        explanation += f"  Agent: {decision.get('agent', 'unknown')}\n"
        explanation += f"  Provider: {decision.get('provider', 'unknown')}\n"
        explanation += f"  Model: {decision.get('model', 'unknown')}\n\n"
        
        explanation += "Reasons:\n"
        reasons = decision.get('reasons', [])
        if reasons:
            for reason in reasons:
                explanation += f"  ✓ {reason}\n"
        else:
            explanation += "  No specific reasons provided\n\n"
        
        explanation += "Estimated:\n"
        estimated = decision.get('estimated', {})
        explanation += f"  Input: {estimated.get('input_tokens', 'unknown')}\n"
        explanation += f"  Output: {estimated.get('output_tokens', 'unknown')}\n"
        
        return explanation
    
    def get_decision_history(self, limit: int = 5) -> List[Dict[str, Any]]:
        """
        Get recent decision history.
        
        Args:
            limit: Maximum number of decisions to return
            
        Returns:
            List of recent decisions
        """
        return self.decision_log[-limit:]
    
    def clear_history(self):
        """Clear decision history."""
        self.decision_log.clear()
        logger.info("Decision history cleared")