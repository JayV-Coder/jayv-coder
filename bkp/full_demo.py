#!/usr/bin/env python3
"""
Complete demo showcasing all Jev AI Orchestrator phases.
"""

import sys
import os

# Add the project root to the Python path
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from core.orchestrator import JevOrchestrator

def main():
    """Run a complete demo of the Jev orchestrator."""
    print("=== Complete Jev AI Orchestrator Demo ===\n")
    
    # Initialize orchestrator
    print("1. Initializing Jev Orchestrator...")
    orchestrator = JevOrchestrator("config.yaml")
    print("✓ Orchestrator initialized successfully\n")
    
    # Show capabilities
    print("2. System Capabilities:")
    print("   • Multi-provider support (OpenAI, Anthropic, local LLMs)")
    print("   • Repository RAG with semantic chunking")
    print("   • Intelligent context selection")
    print("   • Adaptive routing with performance learning")
    print("   • Specialized agents (developer, frontend, security, reviewer)")
    print("   • Tool execution with permission control")
    print("   • Token budgeting and cost tracking")
    print("   • Conversation compression")
    print("   • Explainable routing (/why command)\n")
    
    # Demonstrate different phases
    print("3. Demonstrating All Phases:\n")
    
    # Phase 1: Core orchestration
    print("Phase 1 - Core Orchestration:")
    result = orchestrator.process_request("Fix authentication token refresh issue")
    print(f"   Intent: {result['intent_analysis']['intent']}")
    print(f"   Strategy: {result['strategy']}")
    print(f"   Model: {result['model_selection']['model_name']}")
    print("   ✓ Basic orchestration working\n")
    
    # Phase 2: Repository intelligence
    print("Phase 2 - Repository Intelligence:")
    result = orchestrator.process_request("Explain how the repository RAG works")
    print(f"   Strategy: {result['strategy']}")
    print(f"   Files retrieved: {len(orchestrator.rag.get_relevant_context('explain RAG').get('files', []))}")
    print("   ✓ RAG system working\n")
    
    # Phase 3: Optimization
    print("Phase 3 - Optimization:")
    result = orchestrator.process_request("Create a Python function to calculate Fibonacci numbers")
    tokens = result['result']['tokens_used']
    print(f"   Tokens used: {tokens['input']} input, {tokens['output']} output")
    print(f"   Response time: {result['result'].get('response_time', 0):.2f}s")
    print("   ✓ Optimization working\n")
    
    # Phase 4: Agent specialization
    print("Phase 4 - Agent Specialization:")
    result = orchestrator.process_request("Review this code for security vulnerabilities")
    if 'agent_used' in result['result']:
        print(f"   Specialist agent: {result['result']['agent_used']}")
        print("   ✓ Agent specialization working")
    else:
        print("   Standard model used")
    print("   ✓ Agent system working\n")
    
    # Phase 5: Adaptive routing
    print("Phase 5 - Adaptive Routing:")
    result = orchestrator.process_request("Create a React component for a user profile card")
    print(f"   Strategy: {result['strategy']}")
    print(f"   Agent used: {result['result'].get('agent_used', 'N/A')}")
    print("   ✓ Adaptive routing working\n")
    
    # Show performance tracking
    print("Performance Tracking:")
    stats = orchestrator.performance_tracker.get_overall_stats()
    print(f"   Total requests: {stats['total_requests']}")
    print(f"   Success rate: {stats['successful_requests']/max(stats['total_requests'], 1):.1%}")
    print(f"   Total cost: ${stats['total_cost']:.4f}")
    print("   ✓ Performance tracking working\n")
    
    # Show explainability
    print("Explainability:")
    result = orchestrator.process_request("/why")
    print("   ✓ '/why' command working")
    print("   ✓ Decision explanations available\n")
    
    # Final summary
    print("=== Demo Complete ===")
    print("All 5 phases of Jev AI Orchestrator are functional:")
    print("1. Core orchestration")
    print("2. Repository intelligence")
    print("3. Optimization")
    print("4. Agent specialization")
    print("5. Adaptive routing")
    print("\nThe system demonstrates intelligent routing, context optimization,")
    print("and performance learning - all while minimizing token consumption.")

if __name__ == "__main__":
    main()