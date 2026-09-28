"""
Model Router
Responsible for selecting the most appropriate model based on task requirements.
"""

import logging
from typing import Dict, Any, List
from core.memory import MemoryManager

logger = logging.getLogger(__name__)

class ModelRouter:
    """
    Model Router that selects the best model based on task requirements and available models.
    """
    
    def __init__(self, config: Dict[str, Any]):
        """
        Initialize the model router with configuration.
        
        Args:
            config: Configuration dictionary
        """
        self.config = config
        self.models = config.get('models', {})
        self.budgets = config.get('budgets', {})
        self.memory_manager = MemoryManager()
        
        logger.info("Model Router initialized")
    
    def select_model(self, intent: str, complexity: str, context: Dict[str, Any]) -> Dict[str, Any]:
        """
        Select the most appropriate model for the given task.
        
        Args:
            intent: Intent of the task
            complexity: Complexity level
            context: Context for the task
            
        Returns:
            Dictionary with model selection details
        """
        logger.info(f"Selecting model for intent: {intent}, complexity: {complexity}")
        
        # Determine required capabilities based on intent
        required_capabilities = self._get_required_capabilities(intent)
        
        # Estimate token usage
        estimated_tokens = self._estimate_token_usage(context, complexity)
        
        # Find suitable models
        suitable_models = self._find_suitable_models(required_capabilities, estimated_tokens)
        
        if not suitable_models:
            # Fallback to default model
            logger.warning("No suitable models found, falling back to default")
            fallback_model = self._get_fallback_model()
            return {
                'model_name': fallback_model,
                'provider': self.models[fallback_model]['provider'],
                'capabilities': self.models[fallback_model]['capabilities'],
                'estimated_tokens': estimated_tokens
            }
        
        # Score models and select the best one
        scored_models = self._score_models(suitable_models, complexity, estimated_tokens)
        
        # Return the highest scoring model
        best_model = max(scored_models, key=lambda x: x['score'])
        
        logger.info(f"Selected model: {best_model['model_name']}")
        
        return {
            'model_name': best_model['model_name'],
            'provider': best_model['provider'],
            'capabilities': best_model['capabilities'],
            'estimated_tokens': estimated_tokens,
            'score': best_model['score']
        }
    
    def _get_required_capabilities(self, intent_analysis: Dict[str, Any]) -> List[str]:
        """
        Determine required capabilities based on intent.
        
        Args:
            intent_analysis: Intent analysis dictionary
            
        Returns:
            List of required capabilities
        """
        intent = intent_analysis.get('intent', 'general')
        
        capability_map = {
            'code': ['code', 'tools'],
            'analysis': ['reasoning', 'tools'],
            'test': ['code', 'reasoning'],
            'refactor': ['code', 'reasoning'],
            'security': ['reasoning', 'tools'],
            'frontend': ['frontend', 'ux'],
            'review': ['code-review'],
            'general': ['chat', 'reasoning']
        }
        
        return capability_map.get(intent, ['chat'])
    
    def _estimate_token_usage(self, context: Dict[str, Any], complexity: str) -> int:
        """
        Estimate token usage for the task.
        
        Args:
            context: Context for the task
            complexity: Complexity level
            
        Returns:
            Estimated token count
        """
        # Base estimation based on complexity
        base_tokens = self.budgets.get(complexity, 5000)
        
        # Add context tokens
        context_tokens = context.get('estimated_tokens', 0)
        
        # Add input tokens (approximate)
        input_tokens = len(str(context.get('user_input', ''))) // 4  # Rough estimate
        
        total_tokens = base_tokens + context_tokens + input_tokens
        
        logger.debug(f"Estimated tokens: {total_tokens} (base: {base_tokens}, context: {context_tokens}, input: {input_tokens})")
        
        return total_tokens
    
    def _find_suitable_models(self, required_capabilities: List[str], max_tokens: int) -> List[Dict[str, Any]]:
        """
        Find models that meet the required capabilities and token budget.
        
        Args:
            required_capabilities: Required capabilities
            max_tokens: Maximum token budget
            
        Returns:
            List of suitable models
        """
        suitable_models = []
        
        for model_name, model_config in self.models.items():
            # Check capabilities
            model_capabilities = model_config.get('capabilities', [])
            
            # Check if all required capabilities are met
            if all(cap in model_capabilities for cap in required_capabilities):
                # Check token budget
                context_window = model_config.get('context_window', 4096)
                if max_tokens <= context_window:
                    suitable_models.append({
                        'name': model_name,
                        'provider': model_config['provider'],
                        'capabilities': model_capabilities,
                        'context_window': context_window,
                        'cost_class': model_config.get('cost_class', 'medium'),
                        'speed': model_config.get('speed', 'medium')
                    })
        
        logger.debug(f"Found {len(suitable_models)} suitable models")
        return suitable_models
    
    def _score_models(self, models: List[Dict[str, Any]], complexity: str, 
                      estimated_tokens: int) -> List[Dict[str, Any]]:
        """
        Score models based on various factors.
        
        Args:
            models: List of models to score
            complexity: Task complexity
            estimated_tokens: Estimated token usage
            
        Returns:
            List of scored models
        """
        scored_models = []
        
        # Define weights for scoring factors
        weights = {
            'capability_match': 0.3,
            'cost': 0.2,
            'speed': 0.2,
            'context_penalty': 0.3
        }
        
        for model in models:
            score = 0.0
            
            # Capability match (already verified in find_suitable_models)
            capability_score = 1.0
            score += capability_score * weights['capability_match']
            
            # Cost factor (lower is better)
            cost_factor = {
                'free': 1.0,
                'low': 0.8,
                'medium': 0.6,
                'high': 0.4
            }
            cost_score = cost_factor.get(model['cost_class'], 0.5)
            score += cost_score * weights['cost']
            
            # Speed factor (faster is better)
            speed_factor = {
                'fast': 1.0,
                'medium': 0.7,
                'slow': 0.4
            }
            speed_score = speed_factor.get(model['speed'], 0.5)
            score += speed_score * weights['speed']
            
            # Context penalty (smaller context window is penalized less)
            context_penalty = 1.0 - min(estimated_tokens / model['context_window'], 1.0)
            score += context_penalty * weights['context_penalty']
            
            scored_models.append({
                'model_name': model['name'],
                'provider': model['provider'],
                'capabilities': model['capabilities'],
                'score': score
            })
        
        return sorted(scored_models, key=lambda x: x['score'], reverse=True)
    
    def _get_fallback_model(self) -> str:
        """
        Get a fallback model name.
        
        Returns:
            Name of fallback model
        """
        # Return first model in the configuration as fallback
        if self.models:
            return list(self.models.keys())[0]
        return 'local-fast'