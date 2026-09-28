#!/usr/bin/env python3
"""
Test script for adaptive routing functionality.
"""

import sys
import os

# Add the project root to the Python path
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from core.adaptive_router import PerformanceTracker, AdaptiveRouter

def test_performance_tracker():
    """Test performance tracking functionality."""
    print("=== Testing Performance Tracker ===")
    
    # Create tracker
    tracker = PerformanceTracker(".test_performance.json")
    
    # Record some fake tasks
    tasks = [
        {
            'task_type': 'code',
            'strategy_used': 'rag_first',
            'model_used': 'local-fast',
            'success': True,
            'response_time': 0.5,
            'tokens_used': {'input': 1000, 'output': 200, 'total': 1200},
            'estimated_cost': 0.0012
        },
        {
            'task_type': 'analysis',
            'strategy_used': 'single_model',
            'model_used': 'local-fast',
            'success': True,
            'response_time': 0.3,
            'tokens_used': {'input': 800, 'output': 150, 'total': 950},
            'estimated_cost': 0.00095
        },
        {
            'task_type': 'security',
            'strategy_used': 'specialist',
            'model_used': 'coding-premium',
            'success': False,
            'response_time': 1.2,
            'tokens_used': {'input': 2000, 'output': 300, 'total': 2300},
            'estimated_cost': 0.005
        }
    ]
    
    for i, task in enumerate(tasks, 1):
        print(f"Recording task {i}: {task['task_type']} with model {task['model_used']}")
        tracker.record_task(task)
    
    # Get performance data
    print("\nOverall Stats:")
    stats = tracker.get_overall_stats()
    for key, value in stats.items():
        print(f"  {key}: {value}")
    
    print("\nModel Performance:")
    model_perf = tracker.performance_data['model_performance']
    for model, perf in model_perf.items():
        print(f"  {model}: {perf}")
    
    print("\nTask Types:")
    task_types = tracker.performance_data['task_types']
    for task_type, perf in task_types.items():
        print(f"  {task_type}: {perf}")
    
    print("\nRecommendations:")
    recs = tracker.get_recommendations()
    for rec in recs:
        print(f"  - {rec}")
    
    print()

def test_adaptive_router():
    """Test adaptive routing functionality."""
    print("=== Testing Adaptive Router ===")
    
    # Create a mock config
    config = {
        'models': {
            'local-fast': {'provider': 'lmstudio', 'capabilities': ['chat', 'code']},
            'coding-premium': {'provider': 'anthropic', 'capabilities': ['code', 'reasoning']}
        }
    }
    
    # Create tracker and router
    tracker = PerformanceTracker(".test_performance.json")
    router = AdaptiveRouter(config, tracker)
    
    # Test model scoring
    print("Testing model scoring...")
    model_scores = {}
    
    # Simulate some performance data
    task_data = {
        'task_type': 'code',
        'strategy_used': 'rag_first',
        'model_used': 'local-fast',
        'success': True,
        'response_time': 0.5,
        'tokens_used': {'input': 1000, 'output': 200, 'total': 1200},
        'estimated_cost': 0.0012
    }
    tracker.record_task(task_data)
    
    # Test scoring for models
    for model_name in ['local-fast', 'coding-premium']:
        score = router.calculate_model_score(model_name, {'intent': 'code'}, {})
        model_scores[model_name] = score
        print(f"  {model_name} score: {score:.3f}")
    
    print("\nPerformance Report:")
    # Just show the structure instead of calling the method that doesn't exist
    stats = tracker.get_overall_stats()
    print(f"  Total requests: {stats['total_requests']}")
    print(f"  Success rate: {stats['successful_requests']/max(stats['total_requests'], 1):.1%}")
    print(f"  Average response time: {stats['avg_response_time']:.2f}s")
    print(f"  Total cost: ${stats['total_cost']:.4f}")
    
    print()

def main():
    """Run all tests."""
    print("=== Jev AI Orchestrator - Adaptive Routing Test ===\n")
    
    test_performance_tracker()
    test_adaptive_router()
    
    print("=== Test Complete ===")
    
    # Clean up test files
    try:
        os.remove(".test_performance.json")
    except:
        pass

if __name__ == "__main__":
    main()