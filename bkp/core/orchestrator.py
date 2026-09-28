""" 
Jev AI Orchestrator
Main orchestration engine that makes decisions about how to process user requests.
"""

import yaml
import os
import logging
import time
from typing import Dict, Any, Optional, List
from pathlib import Path

# Import core modules
from core.router import ModelRouter
from core.context import ContextBuilder
from core.memory import MemoryManager
from core.rag import RepositoryRAG
from core.token_manager import TokenBudgetManager
from core.conversation_compressor import ConversationCompressor
from core.explanation_engine import ExplanationEngine
from core.tools import ToolRegistry, ToolExecutor
from core.agents import AgentRegistry, AgentSelector
from core.adaptive_router import PerformanceTracker, AdaptiveRouter
from core.execution_graph.graph import ExecutionGraph
from core.context_engine.context_fork import ContextForkManager
from core.context_engine.semantic_cache import SemanticContextCache
from core.context_engine.context_firewall import ContextFirewall

# Import provider modules
from providers.openai import OpenAIProvider
from providers.anthropic import AnthropicProvider
from providers.openai_compatible import OpenAICompatibleProvider
from providers.cli import CLIProvider

logger = logging.getLogger(__name__)

class JevOrchestrator:
    """ 
    Main orchestrator that makes decisions about how to process user requests.
    """

    def __init__(self, config_path: str = "config.yaml"):
        """ 
        Initialize the Jev orchestrator with configuration.
        
        Args:
            config_path: Path to configuration file
        """
        try:
            self.config_path = config_path
            self.config = self._load_config()
            self.model_router = ModelRouter(self.config)
            self.context_builder = ContextBuilder()
            self.memory_manager = MemoryManager()
            self.rag = RepositoryRAG()
            self.token_manager = TokenBudgetManager(self.config)
            self.conversation_compressor = ConversationCompressor()
            self.explanation_engine = ExplanationEngine()
            self.tool_registry = ToolRegistry(self.config.get('permissions', {}))
            self.tool_executor = ToolExecutor(self.tool_registry)
            self.agent_registry = AgentRegistry()
            self.agent_selector = AgentSelector(self.agent_registry)
            self.performance_tracker = PerformanceTracker()
            self.adaptive_router = AdaptiveRouter(self.config, self.performance_tracker)
            
            # Initialize new components
            self.execution_graph = ExecutionGraph(self.memory_manager)
            self.context_fork_manager = ContextForkManager(self.rag, self.context_builder)
            self.semantic_cache = SemanticContextCache()
            self.context_firewall = ContextFirewall()
            
            # Initialize providers
            self.providers = {}
            self._initialize_providers()
            
            # Index repository on startup
            if not self.rag.index_repository():
                logger.warning("Failed to index repository for RAG")
            
            logger.info("Jev Orchestrator initialized successfully")
            
        except Exception as e:
            logger.error(f"Failed to initialize Jev orchestrator: {e}")
            raise

    def _load_config(self) -> Dict[str, Any]:
        """ 
        Load configuration from YAML file.
        
        Returns:
            Dictionary containing configuration
        """
        try:
            with open(self.config_path, 'r') as f:
                config = yaml.safe_load(f)
                logger.debug(f"Loaded configuration from {self.config_path}")
                return config or {}
        except FileNotFoundError:
            logger.warning(f"Configuration file {self.config_path} not found, using defaults")
            return {}
        except Exception as e:
            logger.error(f"Error loading configuration: {e}")
            raise

    def _initialize_providers(self):
        """ 
        Initialize all configured providers.
        """
        try:
            providers_config = self.config.get('providers', {})
            
            for provider_name, provider_config in providers_config.items():
                provider_type = provider_config.get('type')
                
                if not provider_type:
                    logger.warning(f"Provider {provider_name} has no type specified, skipping")
                    continue

                try:
                    if provider_type == 'openai':
                        self.providers[provider_name] = OpenAIProvider(provider_config)
                    elif provider_type == 'anthropic':
                        self.providers[provider_name] = AnthropicProvider(provider_config)
                    elif provider_type == 'openai-compatible':
                        self.providers[provider_name] = OpenAICompatibleProvider(provider_config)
                    elif provider_type == 'cli':
                        self.providers[provider_name] = CLIProvider(provider_config)
                    else:
                        logger.warning(f"Unknown provider type: {provider_type}")
                        continue
                    
                    # Verify provider health
                    if hasattr(self.providers[provider_name], 'health'):
                        if not self.providers[provider_name].health():
                            logger.warning(f"Provider {provider_name} health check failed")
                        else:
                            logger.info(f"Initialized provider: {provider_name} ({provider_type})")
                    else:
                        logger.info(f"Initialized provider: {provider_name} ({provider_type})")
                    
                except Exception as e:
                    logger.error(f"Failed to initialize provider {provider_name}: {e}")
                    continue
                    
        except Exception as e:
            logger.error(f"Error initializing providers: {e}")
            raise

    def process_request(self, user_input: str, session_id: Optional[str] = None) -> Dict[str, Any]:
        """ 
        Process user request through orchestration pipeline.
        
        Args:
            user_input: user's input/query
            session_id: Optional session identifier
            
        Returns:
            Dictionary with processing results and decisions
        """
        try:
            logger.info(f"Processing user request: {user_input[:50]}...")
            
            # Special commands
            if user_input.startswith('/why'):
                return {
                    'type': 'explanation',
                    'explanation': self.explanation_engine.explain_last_decision()
                }

            # Step 1: Input normalization
            normalized_input = self._normalize_input(user_input)

            # Step 2: Intent analysis
            intent_analysis = self._analyze_intent(normalized_input)

            # Step 3: Complexity analysis
            complexity = self._analyze_complexity(intent_analysis)

            # Step 4: Context planning
            context_plan = self._plan_context(intent_analysis, complexity)

            # Step 5: Context building with semantic caching
            context = self._build_context_with_cache(
                normalized_input,
                intent_analysis,
                complexity,
                context_plan
            )

            # Step 6: Strategy selection
            strategy = self._select_strategy(intent_analysis, complexity)

            # Step 7: Model selection with adaptive learning
            model_selection = self._select_model_adaptive(
                intent_analysis.get('intent', 'general'),
                complexity,
                context
            )

            # Step 8: Request execution
            result = self._execute_request(
                normalized_input,
                context,
                strategy,
                model_selection
            )

            # Step 9: Result validation
            validation_result = self._validate_result(result)

            # Final decision structure
            decision = {
                'model_provider': model_selection.get('provider', 'unknown'),
                'model_name': model_selection.get('model_name', 'unknown'),
                'estimated_tokens': model_selection.get('estimated_tokens', 0),
                'context_files_count': len(context.get('relevant_files', [])),
                'rag_files_count': len(self.rag.get_relevant_context(user_input).get('files', []))
            }

            self.explanation_engine.log_decision(user_input, decision)

            return {
                'user_input': user_input,
                'normalized_input': normalized_input,
                'intent_analysis': intent_analysis,
                'complexity': complexity,
                'context_plan': context_plan,
                'context': context,
                'strategy': strategy,
                'model_selection': model_selection,
                'result': result,
                'validation': validation_result,
                'decision': decision
            }
            
        except Exception as e:
            logger.error(f"Error processing request: {e}")
            return {
                'user_input': user_input,
                'error': str(e),
                'status': 'error'
            }

    def _normalize_input(self, user_input: str) -> str:
        """ 
        Normalize user input for consistent processing.
        
        Args:
            user_input: Raw user input
            
        Returns:
            Normalized input string
        """
        try:
            # Basic normalization
            normalized = user_input.strip()
            
            # Add conversation history compression
            self.conversation_compressor.add_message('user', normalized)
            
            return normalized
        except Exception as e:
            logger.error(f"Error normalizing input: {e}")
            return user_input.strip()

    def _analyze_intent(self, normalized_input: str) -> Dict[str, Any]:
        """ 
        Analyze the intent of the user request.
        
        Args:
            normalized_input: Normalized user input
            
        Returns:
            Dictionary with intent analysis
        """
        try:
            # More robust intent detection based on keywords
            intent_keywords = {
                'code': ['code', 'fix', 'implement', 'write', 'create', 'change', 'refactor', 'develop', 'build'],
                'analysis': ['analyze', 'review', 'explain', 'understand', 'explain', 'investigate', 'study'],
                'test': ['test', 'unit', 'integration', 'assert', 'pytest', 'unittest', 'testing', 'coverage'],
                'refactor': ['refactor', 'rewrite', 'improve', 'optimize', 'cleanup', 'restructure'],
                'security': ['security', 'vulnerability', 'hack', 'exploit', 'secure', 'vulnerabilities', 'penetration'],
                'frontend': ['react', 'vue', 'angular', 'html', 'css', 'javascript', 'frontend', 'ui', 'ux'],
                'review': ['review', 'check', 'approve', 'inspect', 'audit', 'critique'],
                'general': ['help', 'what', 'how', 'why', 'when', 'where', 'who', 'tell', 'show']
            }

            intent_scores = {}
            for intent, keywords in intent_keywords.items():
                score = 0
                for keyword in keywords:
                    if keyword.lower() in normalized_input.lower():
                        score += 1
                intent_scores[intent] = score

            # Return intent with highest score
            best_intent = max(intent_scores, key=intent_scores.get)
            
            # Only return intent if we have confidence (at least one match)
            if intent_scores[best_intent] > 0:
                return {
                    'intent': best_intent,
                    'scores': intent_scores,
                    'confidence': intent_scores[best_intent] / max(len(kw) for kw in intent_keywords.values())
                }
            else:
                return {
                    'intent': 'general',
                    'scores': intent_scores,
                    'confidence': 0.0
                }
                
        except Exception as e:
            logger.error(f"Error analyzing intent: {e}")
            return {
                'intent': 'general',
                'scores': {},
                'confidence': 0.0
            }

    def _build_context_with_cache(self, user_input: str, intent_analysis: Dict[str, Any], 
                                complexity: str, context_plan: List[str]) -> Dict[str, Any]:
        """Build context with semantic caching."""
        try:
            # Check if we have cached context for this query
            # For simplicity, we'll use the basic approach for now
            context = self.context_builder.build_context(
                user_input,
                intent_analysis,
                complexity,
                context_plan
            )
            
            # Apply context firewall to protect sensitive data
            context = self.context_firewall.filter_context(context)
            
            return context
        except Exception as e:
            logger.error(f"Error building context with cache: {e}")
            # Fallback to basic context building
            return self.context_builder.build_context(
                user_input,
                intent_analysis,
                complexity,
                context_plan
            )

    def _select_model_adaptive(self, intent: str, complexity: str, 
                              context: Dict[str, Any]) -> Dict[str, Any]:
        """Select model using adaptive learning."""
        try:
            # Get required capabilities based on intent
            required_capabilities = self.model_router._get_required_capabilities(intent)
            
            # Use adaptive router for better selection
            model_selection = self.adaptive_router.select_model_adaptive(
                intent, complexity, context, required_capabilities
            )
            
            return model_selection
        except Exception as e:
            logger.error(f"Error in adaptive model selection: {e}")
            # Fallback to basic selection
            return self.model_router.select_model(intent, complexity, context)

    def _plan_context(self, intent_analysis: Dict[str, Any], complexity: str) -> List[str]:
        """ 
        Plan context elements needed for request.
        
        Args:
            intent_analysis: Results from intent analysis
            complexity: Complexity level
            
        Returns:
            List of context elements needed
        """
        try:
            # Based on intent and complexity
            context_elements = []

            if intent_analysis.get('intent') == 'code':
                context_elements.extend(['code_files', 'dependencies'])

            if complexity == 'complex':
                context_elements.append('architecture')

            # Code-related tasks, need repository context
            if intent_analysis.get('intent') in ['code', 'refactor']:
                context_elements.append('repository_context')

            # Remove duplicates
            return list(set(context_elements))
            
        except Exception as e:
            logger.error(f"Error planning context: {e}")
            return []

    def _select_strategy(self, intent_analysis: Dict[str, Any], complexity: str) -> str:
        """ 
        Select strategy for processing request.
        
        Args:
            intent_analysis: Results from intent analysis
            complexity: Complexity level
            
        Returns:
            Strategy name
        """
        try:
            intent = intent_analysis.get('intent', 'general')

            if complexity == 'trivial':
                return 'direct'
            elif intent == 'code' or intent == 'refactor':
                return 'rag_first'
            elif intent == 'frontend':
                return 'specialist'
            elif intent == 'security':
                return 'specialist'
            elif intent == 'review':
                return 'specialist'
            else:
                return 'single_model'
                
        except Exception as e:
            logger.error(f"Error selecting strategy: {e}")
            return 'single_model'

    def _execute_request(self, user_input: str, context: Dict[str, Any], 
                        strategy: str, model_selection: Dict[str, Any]) -> Dict[str, Any]:
        """ 
        Execute request with selected strategy and model.
        
        Args:
            user_input: Normalized user input
            context: Context built
            strategy: Selected strategy
            model_selection: Model selection details
            
        Returns:
            Execution result
        """
        try:
            start_time = time.time()
            logger.info(f"Executing request with strategy: {strategy}")
            logger.info(f"Using {model_selection.get('model_name', 'unknown')} model")

            # Handle different strategies
            if strategy == 'direct':
                # Direct execution
                return self._execute_direct(user_input, model_selection)

            elif strategy == 'rag_first':
                # RAG-first approach
                rag_context = self.rag.get_relevant_context(user_input)
                logger.debug(f"Retrieved RAG context for {len(rag_context.get('files', []))} files")

                # Check if we have agents available
                intent = self._analyze_intent(user_input)['intent']
                required_capabilities = self.model_router._get_required_capabilities(intent)

                # Try to use agent if available
                agent = self.agent_selector.select_agent(user_input, required_capabilities)
                if agent:
                    # Process with agent
                    agent_result = agent.process_task(user_input, context)
                    # Simulate token usage
                    input_tokens = self.token_manager.estimate_input_tokens(user_input)
                    output_tokens = self.token_manager.estimate_output_tokens(agent_result.get('result', ''))

                    # Record usage
                    self.token_manager.add_usage(
                        input_tokens=input_tokens,
                        output_tokens=output_tokens,
                        model_name=f"agent-{agent.name}"
                    )

                    # Calculate response time
                    response_time = time.time() - start_time

                    # Record task performance
                    task_data = {
                        'task_type': intent,
                        'strategy_used': strategy,
                        'model_used': f"agent-{agent.name}",
                        'success': True,
                        'response_time': response_time,
                        'tokens_used': {
                            'input': input_tokens,
                            'output': output_tokens,
                            'total': input_tokens + output_tokens
                        },
                        'estimated_cost': self.token_manager.estimate_cost(input_tokens, output_tokens, f"agent-{agent.name}")
                    }
                    self.performance_tracker.record_task(task_data)

                    return {
                        'status': 'success',
                        'response': agent_result['result'],
                        'strategy_used': strategy,
                        'agent_used': agent.name,
                        'context_used': context.get('relevant_files', []),
                        'rag_context_files': len(self.rag.get_relevant_context(user_input).get('files', [])),
                        'tokens_used': {
                            'input': input_tokens,
                            'output': output_tokens,
                            'total': input_tokens + output_tokens
                        },
                        'response_time': response_time
                    }
                else:
                    # Fall back to direct approach
                    return self._execute_direct(user_input, model_selection)

            elif strategy == 'specialist':
                # Specialist approach - use specific agent or expert
                intent = self._analyze_intent(user_input)['intent']
                required_capabilities = self.model_router._get_required_capabilities(intent)
                agent = self.agent_selector.select_agent(user_input, required_capabilities)
                if agent:
                    agent_result = agent.process_task(user_input, context)
                    return {
                        'status': 'success',
                        'response': agent_result['result'],
                        'strategy_used': strategy,
                        'agent_used': agent.name,
                        'context_used': context.get('relevant_files', [])
                    }
                else:
                    # Fall back to direct approach
                    return self._execute_direct(user_input, model_selection)

            else:
                # Default single model approach
                return self._execute_direct(user_input, model_selection)

        except Exception as e:
            logger.error(f"Error executing request: {e}")
            return {
                'status': 'error',
                'error': str(e)
            }

    def _execute_direct(self, user_input: str, model_selection: Dict[str, Any]) -> Dict[str, Any]:
        """ 
        Execute request directly with model.
        
        Args:
            user_input: Normalized user input
            model_selection: Model selection details
            
        Returns:
            Execution result
        """
        try:
            # Validate model selection
            if not model_selection:
                raise ValueError("No model selected for execution")
            
            # Estimate tokens
            input_tokens = self.token_manager.estimate_input_tokens(user_input)
            output_tokens = self.token_manager.estimate_output_tokens("Sample response")

            # Record usage
            self.token_manager.add_usage(
                input_tokens=input_tokens,
                output_tokens=output_tokens,
                model_name=model_selection.get('model_name', 'unknown')
            )

            # Simulate processing time
            start_time = time.time()
            response_time = time.time() - start_time

            # Record task performance
            task_data = {
                'task_type': self._analyze_intent(user_input)['intent'],
                'model_used': model_selection.get('model_name', 'unknown'),
                'success': True,
                'response_time': response_time,
                'tokens_used': {
                    'input': input_tokens,
                    'output': output_tokens,
                    'total': input_tokens + output_tokens
                },
                'estimated_cost': self.token_manager.estimate_cost(input_tokens, output_tokens, model_selection.get('model_name', 'unknown'))
            }
            self.performance_tracker.record_task(task_data)

            return {
                'status': 'success',
                'response': 'This is a sample response. In a real implementation, this would use the selected model to process the request.',
                'strategy_used': 'direct',
                'model_used': model_selection.get('model_name', 'unknown'),
                'context_used': [],
                'rag_context_files': 0,
                'tokens_used': {
                    'input': input_tokens,
                    'output': output_tokens,
                    'total': input_tokens + output_tokens
                },
                'response_time': response_time
            }

        except Exception as e:
            logger.error(f"Error in direct execution: {e}")
            return {
                'status': 'error',
                'error': str(e)
            }

    def _validate_result(self, result: Dict[str, Any]) -> bool:
        """ 
        Validate execution result.
        
        Args:
            result: Execution result
            
        Returns:
            Validation status
        """
        try:
            if not isinstance(result, dict):
                return False
            return result.get('status') == 'success'
        except Exception as e:
            logger.error(f"Error validating result: {e}")
            return False