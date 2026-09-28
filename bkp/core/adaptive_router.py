"""
Adaptive Router
Implements adaptive routing based on historical performance data.
"""

import logging
import json
import os
from typing import Dict, Any, List, Optional
from datetime import datetime
from collections import defaultdict, deque

logger = logging.getLogger(__name__)

class PerformanceTracker:
    """
    Tracks performance metrics for routing decisions.
    """
    
    def __init__(self, storage_file: str = ".jev_performance.json"):
        """
        Initialize the performance tracker.
        
        Args:
            storage_file: File to store performance data
        """
        self.storage_file = storage_file
        self.performance_data = self._load_performance_data()
        self.task_history = deque(maxlen=1000)  # Keep last 1000 tasks
        
        logger.info("Performance Tracker initialized")
    
    def _load_performance_data(self) -> Dict[str, Any]:
        """
        Load performance data from storage file.
        
        Returns:
            Performance data dictionary
        """
        try:
            if os.path.exists(self.storage_file):
                with open(self.storage_file, 'r') as f:
                    return json.load(f)
        except Exception as e:
            logger.warning(f"Failed to load performance data: {e}")
        
        # Return default structure
        return {
            'model_performance': {},
            'task_types': {},
            'overall_stats': {
                'total_requests': 0,
                'successful_requests': 0,
                'avg_response_time': 0.0,
                'total_cost': 0.0
            }
        }
    
    def _save_performance_data(self):
        """
        Save performance data to storage file.
        """
        try:
            with open(self.storage_file, 'w') as f:
                json.dump(self.performance_data, f, indent=2)
        except Exception as e:
            logger.error(f"Failed to save performance data: {e}")
    
    def record_task(self, task_data: Dict[str, Any]):
        """
        Record a task execution.
        
        Args:
            task_data: Data about the executed task
        """
        # Add to task history
        self.task_history.append(task_data)
        
        # Update overall stats
        self.performance_data['overall_stats']['total_requests'] += 1
        if task_data.get('success'):
            self.performance_data['overall_stats']['successful_requests'] += 1
        
        # Update model performance
        model_name = task_data.get('model_used', 'unknown')
        if model_name not in self.performance_data['model_performance']:
            self.performance_data['model_performance'][model_name] = {
                'total_executions': 0,
                'successful_executions': 0,
                'total_cost': 0.0,
                'avg_response_time': 0.0,
                'avg_tokens_used': 0.0
            }
        
        model_perf = self.performance_data['model_performance'][model_name]
        model_perf['total_executions'] += 1
        if task_data.get('success'):
            model_perf['successful_executions'] += 1
        
        model_perf['total_cost'] += task_data.get('estimated_cost', 0.0)
        model_perf['avg_response_time'] = (
            (model_perf['avg_response_time'] * (model_perf['total_executions'] - 1) + 
             task_data.get('response_time', 0.0)) / model_perf['total_executions']
        )
        
        # Update token usage average
        model_perf['avg_tokens_used'] = (
            (model_perf['avg_tokens_used'] * (model_perf['total_executions'] - 1) + 
             task_data.get('tokens_used', {}).get('total', 0)) / model_perf['total_executions']
        )
        
        # Update task type stats
        task_type = task_data.get('task_type', 'unknown')
        if task_type not in self.performance_data['task_types']:
            self.performance_data['task_types'][task_type] = {
                'total_executions': 0,
                'successful_executions': 0,
                'avg_cost': 0.0,
                'avg_response_time': 0.0
            }
        
        task_perf = self.performance_data['task_types'][task_type]
        task_perf['total_executions'] += 1
        if task_data.get('success'):
            task_perf['successful_executions'] += 1
        
        task_perf['avg_cost'] = (
            (task_perf['avg_cost'] * (task_perf['total_executions'] - 1) + 
             task_data.get('estimated_cost', 0.0)) / task_perf['total_executions']
        )
        
        task_perf['avg_response_time'] = (
            (task_perf['avg_response_time'] * (task_perf['total_executions'] - 1) + 
             task_data.get('response_time', 0.0)) / task_perf['total_executions']
        )
        
        # Update overall stats
        total_req = self.performance_data['overall_stats']['total_requests']
        success_req = self.performance_data['overall_stats']['successful_requests']
        self.performance_data['overall_stats']['avg_response_time'] = (
            (self.performance_data['overall_stats']['avg_response_time'] * (total_req - 1) + 
             task_data.get('response_time', 0.0)) / total_req
        )
        self.performance_data['overall_stats']['total_cost'] += task_data.get('estimated_cost', 0.0)
        
        # Save updated data
        self._save_performance_data()
    
    def get_model_performance(self, model_name: str) -> Dict[str, Any]:
        """
        Get performance data for a specific model.
        
        Args:
            model_name: Name of the model
            
        Returns:
            Performance data for the model
        """
        return self.performance_data['model_performance'].get(model_name, {})
    
    def get_task_performance(self, task_type: str) -> Dict[str, Any]:
        """
        Get performance data for a specific task type.
        
        Args:
            task_type: Type of task
            
        Returns:
            Performance data for the task type
        """
        return self.performance_data['task_types'].get(task_type, {})
    
    def get_overall_stats(self) -> Dict[str, Any]:
        """
        Get overall performance statistics.
        
        Returns:
            Overall statistics
        """
        return self.performance_data['overall_stats']
    
    def get_recommendations(self) -> List[str]:
        """
        Get recommendations based on performance data.
        
        Returns:
            List of recommendations
        """
        recommendations = []
        
        # Check for underperforming models
        for model_name, perf_data in self.performance_data['model_performance'].items():
            if perf_data['total_executions'] > 10:  # Only consider models with enough data
                success_rate = perf_data['successful_executions'] / perf_data['total_executions']
                if success_rate < 0.7:  # Less than 70% success rate
                    recommendations.append(
                        f"Model '{model_name}' has low success rate ({success_rate:.1%})"
                    )
        
        # Check for high-cost models
        total_cost = self.performance_data['overall_stats']['total_cost']
        if total_cost > 100.0:  # More than $100 spent
            recommendations.append(
                f"High cost usage detected ($%.2f total)" % total_cost
            )
        
        return recommendations

class AdaptiveRouter:
    """
    Adaptive router that learns from past performance to improve future routing decisions.
    """
    
    def __init__(self, config: Dict[str, Any], performance_tracker: PerformanceTracker):
        """
        Initialize the adaptive router.
        
        Args:
            config: Configuration dictionary
            performance_tracker: Performance tracker instance
        """
        self.config = config
        self.performance_tracker = performance_tracker
        self.model_scores = {}  # Cache for model scores
        
        logger.info("Adaptive Router initialized")
    
    def calculate_model_score(self, model_name: str, task_requirements: Dict[str, Any], 
                            context: Dict[str, Any]) -> float:
        """
        Calculate a score for a model based on historical performance and task requirements.
        
        Args:
            model_name: Name of the model
            task_requirements: Requirements for the task
            context: Context for the task
            
        Returns:
            Model score (higher is better)
        """
        # Check cache first
        cache_key = f"{model_name}_{hash(str(task_requirements))}"
        if cache_key in self.model_scores:
            return self.model_scores[cache_key]
        
        # Get historical performance for this model
        model_perf = self.performance_tracker.get_model_performance(model_name)
        
        # Base score calculation
        base_score = 1.0  # Start with perfect score
        
        # Adjust based on success rate
        if model_perf.get('total_executions', 0) > 0:
            success_rate = model_perf.get('successful_executions', 0) / model_perf.get('total_executions', 1)
            base_score *= success_rate
        
        # Adjust based on cost efficiency (lower is better)
        avg_cost = model_perf.get('total_cost', 0) / max(model_perf.get('total_executions', 1), 1)
        if avg_cost > 0:
            # Normalize cost (assuming $0.01 per 1K tokens)
            cost_efficiency = 1.0 / (1.0 + avg_cost)  # Lower cost = higher efficiency
            base_score *= cost_efficiency
        
        # Adjust based on response time
        avg_response_time = model_perf.get('avg_response_time', 1.0)
        if avg_response_time > 0:
            # Lower response time = higher score
            response_efficiency = 1.0 / (1.0 + avg_response_time / 10.0)  # Normalize
            base_score *= response_efficiency
        
        # Store in cache
        self.model_scores[cache_key] = base_score
        
        return base_score
    
    def select_model_adaptive(self, intent: str, complexity: str, context: Dict[str, Any]) -> Dict[str, Any]:
        """
        Select the best model considering adaptive learning.
        
        Args:
            intent: Task intent
            complexity: Task complexity
            context: Context for the task
            
        Returns:
            Model selection details
        """
        # In a real implementation, this would be more sophisticated
        # For now, we'll just use the original router logic but incorporate adaptive scoring
        
        # This is a placeholder - in a real implementation, this would be integrated
        # with the existing ModelRouter to use adaptive scoring
        return {
            'model_name': 'adaptive-model',
            'provider': 'adaptive-provider',
            'score': 0.85
        }
    
    def get_performance_report(self) -> Dict[str, Any]:
        """
        Generate a performance report.
        
        Returns:
            Performance report dictionary
        """
        return {
            'overall_stats': self.performance_tracker.get_overall_stats(),
            'model_performance': self.performance_tracker.performance_data['model_performance'],
            'task_types': self.performance_tracker.performance_data['task_types'],
            'recommendations': self.performance_tracker.get_recommendations()
        }