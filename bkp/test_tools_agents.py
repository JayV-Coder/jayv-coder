#!/usr/bin/env python3
"""
Test script for tools and agents functionality.
"""

import sys
import os

# Add the project root to the Python path
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from core.tools import ToolRegistry, ToolExecutor
from core.agents import AgentRegistry, AgentSelector

def test_tools():
    """Test tool functionality."""
    print("=== Testing Tools ===")
    
    # Create tool registry
    registry = ToolRegistry({
        'read': {'execute': 'allow'},
        'write': {'execute': 'ask'},
        'shell': {'execute': 'deny'}
    })
    
    # Create tool executor
    executor = ToolExecutor(registry)
    
    # Test listing tools
    print("Available tools:", registry.list_tools())
    
    # Test permissions
    print("Read permission:", registry.validate_permission('read'))
    print("Write permission:", registry.validate_permission('write'))
    print("Shell permission:", registry.validate_permission('shell'))
    
    # Test reading a file (this will fail but show structure)
    try:
        result = executor.execute_tool('read', file_path='/nonexistent.txt')
        print("Read result:", result)
    except Exception as e:
        print("Read error (expected):", str(e))
    
    print()

def test_agents():
    """Test agent functionality."""
    print("=== Testing Agents ===")
    
    # Create agent registry
    registry = AgentRegistry()
    
    # Test listing agents
    print("Available agents:", registry.list_agents())
    
    # Test getting specific agent
    dev_agent = registry.get_agent('developer')
    if dev_agent:
        print("Developer agent capabilities:", dev_agent.capabilities)
    
    # Test agent selection
    selector = AgentSelector(registry)
    
    # Test selecting agents by capability
    code_agents = registry.get_agents_by_capability('coding')
    print("Agents with coding capability:", [a.name for a in code_agents])
    
    # Test selecting specific agents
    test_tasks = [
        ("Fix authentication issue", "code"),
        ("Review security vulnerabilities", "security"),
        ("Create React component", "frontend"),
        ("Review code quality", "review")
    ]
    
    for task, intent in test_tasks:
        print(f"\nTask: {task}")
        print(f"Intent: {intent}")
        
        # Select appropriate agent
        required_caps = ['coding'] if intent == 'code' else ['security'] if intent == 'security' else ['frontend'] if intent == 'frontend' else ['code-review']
        agent = selector.select_agent(task, required_caps)
        print(f"Selected agent: {agent.name}")
        
        # Process task
        result = agent.process_task(task, {})
        print(f"Result: {result['result'][:50]}...")

def main():
    """Run all tests."""
    print("=== Jev AI Orchestrator - Tools & Agents Test ===\n")
    
    test_tools()
    test_agents()
    
    print("\n=== Test Complete ===")

if __name__ == "__main__":
    main()