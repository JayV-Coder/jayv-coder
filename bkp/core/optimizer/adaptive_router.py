"""
Adaptive Model Router for Jev AI Orchestrator
Learn from historical executions to improve routing decisions.
"""

import time
import json
import os
from typing import Dict, List, Any, Optional, Tuple
from dataclasses import dataclass, field
from collections import defaultdict, deque
from core.router import ModelRouter
from core.memory import MemoryManager

@dataclass
class RoutingDecision:
    """Represents a routing decision with metadata."""
    
    task_type: str
    model: str
    provider: str
    tokens_used: int
    cost: float
    latency: float
    success: bool
    tests_passed: bool
    retry_count: int
    escalation: bool
    user_feedback: str = ""
    timestamp: float = 0.0
    confidence: float = 0.0
    context_size: int = 0
    capabilities_matched: List[str] = field(default_factory=list)

class AdaptiveModelRouter:
    """Adaptive router that learns from historical execution data."""
    
    def __init__(self, model_router: ModelRouter, memory_manager: MemoryManager, 
                 cache_dir: str = ".jev_cache"):
        self.model_router = model_router
        self.memory_manager = memory_manager
        self.cache_dir = cache_dir
        self.decision_history = deque(maxlen=1000)  # Keep last 1000 decisions
        self.task_profiles = defaultdict(list)  # Store profiles per task type
        self.model_performance = defaultdict(dict)  # Store performance per model
        self.provider_performance = defaultdict(dict)  # Store performance per provider
        
        # Load historical data
        self._load_historical_data()
        
    def _load_historical_data(self) -> None:
        """Load historical routing decisions from cache."""
        try:
            cache_file = os.path.join(self.cache_dir, "routing_history.json")
            if os.path.exists(cache_file):
                with open(cache_file, 'r') as f:
                    data = json.load(f)
                    for item in data:
                        decision = RoutingDecision(**item)
                        self.decision_history.append(decision)
                        self.task_profiles[decision.task_type].append(decision)
        except Exception as e:
            print(f"Failed to load historical data: {e}")
            
    def _save_historical_data(self) -> None:
        """Save routing decisions to cache."""
        try:
            cache_file = os.path.join(self.cache_dir, "routing_history.json")
            data = [vars(decision) for decision in self.decision_history]
            with open(cache_file, 'w') as f:
                json.dump(data, f, indent=2)
        except Exception as e:
            print(f"Failed to save historical data: {e}")
            
    def record_decision(self, task_type: str, model: str, provider: str,
                       tokens_used: int, cost: float, latency: float,
                       success: bool, tests_passed: bool, retry_count: int,
                       escalation: bool, user_feedback: str = "",
                       confidence: float = 0.0, context_size: int = 0,
                       capabilities_matched: List[str] = None) -> None:
        """Record a routing decision for learning."""
        if capabilities_matched is None:
            capabilities_matched = []
            
        decision = RoutingDecision(
            task_type=task_type,
            model=model,
            provider=provider,
            tokens_used=tokens_used,
            cost=cost,
            latency=latency,
            success=success,
            tests_passed=tests_passed,
            retry_count=retry_count,
            escalation=escalation,
            user_feedback=user_feedback,
            timestamp=time.time(),
            confidence=confidence,
            context_size=context_size,
            capabilities_matched=capabilities_matched
        )
        
        self.decision_history.append(decision)
        self.task_profiles[task_type].append(decision)
        
        # Update performance metrics
        self._update_performance_metrics(decision)
        
        # Save to cache
        self._save_historical_data()
        
    def _update_performance_metrics(self, decision: RoutingDecision) -> None:
        """Update performance metrics for models and providers."""
        # Model performance
        if decision.model not in self.model_performance[decision.task_type]:
            self.model_performance[decision.task_type][decision.model] = {
                'total_executions': 0,
                'successful_executions': 0,
                'total_cost': 0.0,
                'total_latency': 0.0,
                'average_cost': 0.0,
                'average_latency': 0.0,
                'success_rate': 0.0
            }
            
        model_perf = self.model_performance[decision.task_type][decision.model]
        model_perf['total_executions'] += 1
        if decision.success:
            model_perf['successful_executions'] += 1
        model_perf['total_cost'] += decision.cost
        model_perf['total_latency'] += decision.latency
        
        # Calculate averages
        model_perf['average_cost'] = model_perf['total_cost'] / model_perf['total_executions']
        model_perf['average_latency'] = model_perf['total_latency'] / model_perf['total_executions']
        model_perf['success_rate'] = model_perf['successful_executions'] / model_perf['total_executions']
        
        # Provider performance
        if decision.provider not in self.provider_performance[decision.task_type]:
            self.provider_performance[decision.task_type][decision.provider] = {
                'total_executions': 0,
                'successful_executions': 0,
                'total_cost': 0.0,
                'total_latency': 0.0,
                'average_cost': 0.0,
                'average_latency': 0.0,
                'success_rate': 0.0
            }
            
        provider_perf = self.provider_performance[decision.task_type][decision.provider]
        provider_perf['total_executions'] += 1
        if decision.success:
            provider_perf['successful_executions'] += 1
        provider_perf['total_cost'] += decision.cost
        provider_perf['total_latency'] += decision.latency
        
        # Calculate averages
        provider_perf['average_cost'] = provider_perf['total_cost'] / provider_perf['total_executions']
        provider_perf['average_latency'] = provider_perf['total_latency'] / provider_perf['total_executions']
        provider_perf['success_rate'] = provider_perf['successful_executions'] / provider_perf['total_executions']
        
    def calculate_routing_confidence(self, task_type: str, context: Dict[str, Any],
                                  required_capabilities: List[str]) -> float:
        """Calculate confidence in routing decision."""
        # Simple confidence calculation based on historical data
        task_decisions = self.task_profiles.get(task_type, [])
        
        if not task_decisions:
            return 0.5  # Default confidence
            
        # Calculate recent success rate
        recent_decisions = [d for d in task_decisions if time.time() - d.timestamp < 86400]  # Last 24 hours
        
        if not recent_decisions:
            return 0.5
            
        success_rate = sum(1 for d in recent_decisions if d.success) / len(recent_decisions)
        
        # Calculate capability match
        avg_capabilities = sum(len(d.capabilities_matched) for d in recent_decisions) / len(recent_decisions)
        capability_match = min(avg_capabilities / len(required_capabilities), 1.0) if required_capabilities else 1.0
        
        # Combine factors
        confidence = (success_rate * 0.6) + (capability_match * 0.4)
        
        return min(confidence, 1.0)
        
    def select_model_adaptive(self, task_type: str, complexity: str, 
                            context: Dict[str, Any], 
                            required_capabilities: List[str]) -> Dict[str, Any]:
        """Select the best model based on adaptive learning."""
        # First get the base model selection
        base_selection = self.model_router.select_model(task_type, complexity, context)
        
        # Check if we have historical data for this task type
        task_decisions = self.task_profiles.get(task_type, [])
        
        if not task_decisions:
            # No historical data, use base selection
            return base_selection
            
        # Calculate confidence in base selection
        confidence = self.calculate_routing_confidence(task_type, context, required_capabilities)
        
        # If confidence is low, adjust based on historical performance
        if confidence < 0.7:
            # Find better performing models for this task type
            best_model = self._find_best_model_for_task(task_type, required_capabilities)
            if best_model:
                # Override with better performing model
                base_selection['model_name'] = best_model
                base_selection['provider'] = self._find_provider_for_model(best_model)
                
        # Add confidence to the selection
        base_selection['confidence'] = confidence
        
        return base_selection
        
    def _find_best_model_for_task(self, task_type: str, required_capabilities: List[str]) -> Optional[str]:
        """Find the best performing model for a specific task type."""
        task_decisions = self.task_profiles.get(task_type, [])
        
        if not task_decisions:
            return None
            
        # Group decisions by model
        model_decisions = defaultdict(list)
        for decision in task_decisions:
            model_decisions[decision.model].append(decision)
            
        # Calculate performance score for each model
        model_scores = {}
        for model, decisions in model_decisions.items():
            # Success rate
            success_rate = sum(1 for d in decisions if d.success) / len(decisions)
            
            # Average cost efficiency
            avg_cost = sum(d.cost for d in decisions) / len(decisions) if decisions else 0
            
            # Average latency
            avg_latency = sum(d.latency for d in decisions) / len(decisions) if decisions else 0
            
            # Calculate score (higher is better)
            # Weighted combination: success rate (40%), cost efficiency (30%), latency (30%)
            score = (
                success_rate * 0.4 +
                (1 / (avg_cost + 1)) * 0.3 +  # Lower cost = higher score
                (1 / (avg_latency + 1)) * 0.3  # Lower latency = higher score
            )
            
            model_scores[model] = score
            
        # Return model with highest score
        if model_scores:
            return max(model_scores, key=model_scores.get)
            
        return None
        
    def _find_provider_for_model(self, model_name: str) -> str:
        """Find the provider for a specific model."""
        # This would need to be enhanced with actual model-provider mapping
        # For now, return a default provider
        return "default"
        
    def get_model_recommendations(self, task_type: str) -> Dict[str, Any]:
        """Get recommendations for models for a specific task type."""
        task_decisions = self.task_profiles.get(task_type, [])
        
        if not task_decisions:
            return {
                'recommendations': [],
                'confidence': 0.0
            }
            
        # Group by model
        model_decisions = defaultdict(list)
        for decision in task_decisions:
            model_decisions[decision.model].append(decision)
            
        # Calculate recommendations
        recommendations = []
        for model, decisions in model_decisions.items():
            success_rate = sum(1 for d in decisions if d.success) / len(decisions)
            avg_cost = sum(d.cost for d in decisions) / len(decisions) if decisions else 0
            avg_latency = sum(d.latency for d in decisions) / len(decisions) if decisions else 0
            
            recommendations.append({
                'model': model,
                'success_rate': success_rate,
                'avg_cost': avg_cost,
                'avg_latency': avg_latency,
                'total_executions': len(decisions)
            })
            
        # Sort by success rate
        recommendations.sort(key=lambda x: x['success_rate'], reverse=True)
        
        return {
            'recommendations': recommendations[:5],  # Top 5
            'confidence': min(len(task_decisions) / 100, 1.0)  # Confidence based on sample size
        }
        
    def get_performance_report(self, task_type: str) -> Dict[str, Any]:
        """Get performance report for a task type."""
        task_decisions = self.task_profiles.get(task_type, [])
        
        if not task_decisions:
            return {
                'task_type': task_type,
                'total_executions': 0,
                'success_rate': 0.0,
                'average_cost': 0.0,
                'average_latency': 0.0,
                'models': [],
                'providers': []
            }
            
        # Aggregate statistics
        total_executions = len(task_decisions)
        success_count = sum(1 for d in task_decisions if d.success)
        success_rate = success_count / total_executions
        
        total_cost = sum(d.cost for d in task_decisions)
        average_cost = total_cost / total_executions
        
        total_latency = sum(d.latency for d in task_decisions)
        average_latency = total_latency / total_executions
        
        # Group by model and provider
        model_stats = defaultdict(lambda: {'executions': 0, 'successes': 0, 'cost': 0, 'latency': 0})
        provider_stats = defaultdict(lambda: {'executions': 0, 'successes': 0, 'cost': 0, 'latency': 0})
        
        for decision in task_decisions:
            # Model stats
            model_stats[decision.model]['executions'] += 1
            if decision.success:
                model_stats[decision.model]['successes'] += 1
            model_stats[decision.model]['cost'] += decision.cost
            model_stats[decision.model]['latency'] += decision.latency
            
            # Provider stats
            provider_stats[decision.provider]['executions'] += 1
            if decision.success:
                provider_stats[decision.provider]['successes'] += 1
            provider_stats[decision.provider]['cost'] += decision.cost
            provider_stats[decision.provider]['latency'] += decision.latency
            
        # Calculate averages for each
        model_details = []
        for model, stats in model_stats.items():
            success_rate = stats['successes'] / stats['executions'] if stats['executions'] > 0 else 0
            avg_cost = stats['cost'] / stats['executions'] if stats['executions'] > 0 else 0
            avg_latency = stats['latency'] / stats['executions'] if stats['executions'] > 0 else 0
            
            model_details.append({
                'model': model,
                'executions': stats['executions'],
                'success_rate': success_rate,
                'avg_cost': avg_cost,
                'avg_latency': avg_latency
            })
            
        provider_details = []
        for provider, stats in provider_stats.items():
            success_rate = stats['successes'] / stats['executions'] if stats['executions'] > 0 else 0
            avg_cost = stats['cost'] / stats['executions'] if stats['executions'] > 0 else 0
            avg_latency = stats['latency'] / stats['executions'] if stats['executions'] > 0 else 0
            
            provider_details.append({
                'provider': provider,
                'executions': stats['executions'],
                'success_rate': success_rate,
                'avg_cost': avg_cost,
                'avg_latency': avg_latency
            })
            
        return {
            'task_type': task_type,
            'total_executions': total_executions,
            'success_rate': success_rate,
            'average_cost': average_cost,
            'average_latency': average_latency,
            'models': model_details,
            'providers': provider_details
        }