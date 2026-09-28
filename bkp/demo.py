#!/usr/bin/env python3
"""
Demo script showing Jev AI Orchestrator in action.
"""

import sys
import os
import logging

# Add the project root to the Python path
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from core.orchestrator import JevOrchestrator

def main():
    """Run a demo of the Jev orchestrator."""
    print("=== Jev AI Orchestrator Demo ===\n")
    
    # Initialize orchestrator
    print("Initializing Jev Orchestrator...")
    orchestrator = JevOrchestrator("config.yaml")
    print("✓ Orchestrator initialized successfully\n")
    
    # Demonstrate processing a few sample requests
    test_requests = [
        "Fix authentication token refresh issue",
        "Explain how the repository RAG works",
        "Create a Python function to calculate Fibonacci numbers",
        "Review this code for security vulnerabilities",
        "Create a React component for a user profile card",
        "/why"  # Test explanation feature
    ]
    
    for i, request in enumerate(test_requests, 1):
        print(f"Request {i}: {request}")
        print("-" * 50)
        
        # Process the request
        result = orchestrator.process_request(request)
        
        # Check if it's an explanation request
        if result.get('type') == 'explanation':
            print("Explanation:")
            print(result['explanation'])
        else:
            # Display the decision process
            print(f"Intent: {result['intent_analysis']['intent']}")
            print(f"Complexity: {result['complexity']}")
            print(f"Strategy: {result['strategy']}")
            print(f"Selected Model: {result['model_selection']['model_name']}")
            print(f"Provider: {result['model_selection']['provider']}")
            print(f"Estimated Tokens: {result['model_selection']['estimated_tokens']}")
            
            # Show token usage
            if 'tokens_used' in result['result']:
                tokens = result['result']['tokens_used']
                print(f"Tokens Used: {tokens['input']} input, {tokens['output']} output")
            
            # Show a sample response
            print(f"\nSample Response:")
            print(result['result']['response'][:100] + "..." if len(result['result']['response']) > 100 else result['result']['response'])
            
            # Show decision explanation
            if 'decision' in result:
                print(f"\nDecision Explanation:")
                decision = result['decision']
                print(f"  Agent: {decision['agent']}")
                print(f"  Provider: {decision['provider']}")
                print(f"  Model: {decision['model']}")
                print(f"  Reasons: {', '.join(decision['reasons'][:3])}")
            
            # Show agent usage if applicable
            if 'agent_used' in result['result']:
                print(f"Specialist Agent Used: {result['result']['agent_used']}")
        
        print("\n" + "="*60 + "\n")

if __name__ == "__main__":
    main()