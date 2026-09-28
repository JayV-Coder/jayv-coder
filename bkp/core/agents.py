"""
Agents Framework
Framework for managing and executing specialized agents.
"""

import logging
from typing import Dict, Any, List, Optional
from abc import ABC, abstractmethod

logger = logging.getLogger(__name__)

class Agent(ABC):
    """
    Abstract base class for all agents.
    """
    
    def __init__(self, name: str, capabilities: List[str]):
        """
        Initialize an agent.
        
        Args:
            name: Agent name
            capabilities: List of capabilities this agent has
        """
        self.name = name
        self.capabilities = capabilities
        
        logger.debug(f"Created agent: {name} with capabilities: {capabilities}")
    
    @abstractmethod
    def process_task(self, task: str, context: Dict[str, Any]) -> Dict[str, Any]:
        """
        Process a task with this agent.
        
        Args:
            task: Task description
            context: Context for the task
            
        Returns:
            Result of task processing
        """
        pass
    
    def has_capability(self, capability: str) -> bool:
        """
        Check if agent has a specific capability.
        
        Args:
            capability: Capability to check
            
        Returns:
            True if agent has capability, False otherwise
        """
        return capability in self.capabilities

class AgentRegistry:
    """
    Registry for managing available agents.
    """
    
    def __init__(self):
        """Initialize the agent registry."""
        self.agents = {}
        self._register_builtin_agents()
        
        logger.info("Agent Registry initialized")
    
    def _register_builtin_agents(self):
        """Register built-in agents."""
        # Developer agent
        self.register_agent('developer', DeveloperAgent(), 
                          ['coding', 'reasoning', 'tools'])
        
        # Frontend agent
        self.register_agent('frontend', FrontendAgent(), 
                          ['frontend', 'ux', 'browser'])
        
        # Security agent
        self.register_agent('security', SecurityAgent(), 
                          ['security', 'reasoning'])
        
        # Reviewer agent
        self.register_agent('reviewer', CodeReviewerAgent(), 
                          ['code-review'])
        
        logger.info("Registered builtin agents")
    
    def register_agent(self, name: str, agent: Agent, capabilities: List[str] = None):
        """
        Register a new agent.
        
        Args:
            name: Agent name
            agent: Agent instance
            capabilities: List of capabilities
        """
        if capabilities:
            agent.capabilities = capabilities
            
        self.agents[name] = agent
        logger.debug(f"Registered agent: {name}")
    
    def get_agent(self, name: str) -> Optional[Agent]:
        """
        Get an agent by name.
        
        Args:
            name: Agent name
            
        Returns:
            Agent instance or None if not found
        """
        return self.agents.get(name)
    
    def get_agents_by_capability(self, capability: str) -> List[Agent]:
        """
        Get all agents that have a specific capability.
        
        Args:
            capability: Capability to search for
            
        Returns:
            List of agents with the capability
        """
        return [agent for agent in self.agents.values() 
                if agent.has_capability(capability)]
    
    def list_agents(self) -> List[str]:
        """
        List all registered agents.
        
        Returns:
            List of agent names
        """
        return list(self.agents.keys())

class DeveloperAgent(Agent):
    """
    Specialized developer agent.
    """
    
    def __init__(self):
        """Initialize the developer agent."""
        super().__init__('developer', ['coding', 'reasoning', 'tools'])
    
    def process_task(self, task: str, context: Dict[str, Any]) -> Dict[str, Any]:
        """
        Process a development task.
        
        Args:
            task: Task description
            context: Context for the task
            
        Returns:
            Result of task processing
        """
        # Simulate processing
        return {
            'success': True,
            'agent': self.name,
            'task': task,
            'action': 'develop',
            'result': f'Developer agent processed: {task}',
            'context_used': context.get('relevant_files', [])
        }

class FrontendAgent(Agent):
    """
    Specialized frontend agent.
    """
    
    def __init__(self):
        """Initialize the frontend agent."""
        super().__init__('frontend', ['frontend', 'ux', 'browser'])
    
    def process_task(self, task: str, context: Dict[str, Any]) -> Dict[str, Any]:
        """
        Process a frontend task.
        
        Args:
            task: Task description
            context: Context for the task
            
        Returns:
            Result of task processing
        """
        # Simulate processing
        return {
            'success': True,
            'agent': self.name,
            'task': task,
            'action': 'frontend',
            'result': f'Frontend agent processed: {task}',
            'context_used': context.get('relevant_files', [])
        }

class SecurityAgent(Agent):
    """
    Specialized security agent.
    """
    
    def __init__(self):
        """Initialize the security agent."""
        super().__init__('security', ['security', 'reasoning'])
    
    def process_task(self, task: str, context: Dict[str, Any]) -> Dict[str, Any]:
        """
        Process a security task.
        
        Args:
            task: Task description
            context: Context for the task
            
        Returns:
            Result of task processing
        """
        # Simulate processing
        return {
            'success': True,
            'agent': self.name,
            'task': task,
            'action': 'security',
            'result': f'Security agent processed: {task}',
            'context_used': context.get('relevant_files', [])
        }

class CodeReviewerAgent(Agent):
    """
    Specialized code reviewer agent.
    """
    
    def __init__(self):
        """Initialize the code reviewer agent."""
        super().__init__('reviewer', ['code-review'])
    
    def process_task(self, task: str, context: Dict[str, Any]) -> Dict[str, Any]:
        """
        Process a code review task.
        
        Args:
            task: Task description
            context: Context for the task
            
        Returns:
            Result of task processing
        """
        # Simulate processing
        return {
            'success': True,
            'agent': self.name,
            'task': task,
            'action': 'review',
            'result': f'Reviewer agent processed: {task}',
            'context_used': context.get('relevant_files', [])
        }

class AgentSelector:
    """
    Selects the appropriate agent for a given task.
    """
    
    def __init__(self, agent_registry: AgentRegistry):
        """
        Initialize the agent selector.
        
        Args:
            agent_registry: Agent registry instance
        """
        self.agent_registry = agent_registry
        logger.info("Agent Selector initialized")
    
    def select_agent(self, task: str, required_capabilities: List[str]) -> Optional[Agent]:
        """
        Select the best agent for a task based on required capabilities.
        
        Args:
            task: Task description
            required_capabilities: List of required capabilities
            
        Returns:
            Best matching agent or None if none found
        """
        # Find agents that have all required capabilities
        suitable_agents = []
        
        for agent in self.agent_registry.agents.values():
            # Check if agent has all required capabilities
            if all(cap in agent.capabilities for cap in required_capabilities):
                suitable_agents.append(agent)
        
        # If we found suitable agents, return the first one
        if suitable_agents:
            return suitable_agents[0]
        
        # If no specific agent found, return the developer agent as default
        return self.agent_registry.get_agent('developer')