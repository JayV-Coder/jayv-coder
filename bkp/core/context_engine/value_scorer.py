"""
Context Value Scoring for Jev AI Orchestrator
Scores context fragments for information value per token.
"""

import math
from typing import Dict, List, Any, Optional
from dataclasses import dataclass

@dataclass
class ContextFragment:
    """Represents a context fragment with metadata."""
    
    id: str
    content: str
    relevance: float  # 0.0 to 1.0
    confidence: float  # 0.0 to 1.0
    freshness: float  # 0.0 to 1.0 (newness)
    tokens: int
    dependency_importance: float  # 0.0 to 1.0
    file_path: str = ""
    source: str = ""
    timestamp: float = 0.0

class ContextValueScorer:
    """Scores context fragments for information value per token."""
    
    def __init__(self):
        self.weights = {
            'relevance': 0.3,
            'confidence': 0.25,
            'freshness': 0.2,
            'dependency_importance': 0.15,
            'token_cost': 0.1
        }
        
    def calculate_context_value(self, fragment: ContextFragment) -> float:
        """
        Calculate value score for a context fragment.
        
        Value = relevance × confidence × freshness × dependency_importance ÷ token_cost
        
        Higher value means more information per token.
        """
        try:
            # Normalize token cost (avoid division by zero)
            normalized_tokens = max(fragment.tokens, 1)
            
            # Calculate weighted score
            value_score = (
                fragment.relevance * self.weights['relevance'] +
                fragment.confidence * self.weights['confidence'] +
                fragment.freshness * self.weights['freshness'] +
                fragment.dependency_importance * self.weights['dependency_importance'] -
                (normalized_tokens / 10000) * self.weights['token_cost']  # Reduce score for high token count
            )
            
            # Ensure score is positive
            return max(0.0, value_score)
            
        except Exception as e:
            print(f"Error calculating context value: {e}")
            return 0.0
    
    def rank_context_fragments(self, fragments: List[ContextFragment]) -> List[ContextFragment]:
        """Rank context fragments by value score."""
        scored_fragments = [(fragment, self.calculate_context_value(fragment)) 
                           for fragment in fragments]
        
        # Sort by value score descending
        scored_fragments.sort(key=lambda x: x[1], reverse=True)
        
        return [fragment for fragment, score in scored_fragments]
    
    def optimize_context_for_budget(self, fragments: List[ContextFragment], 
                                  max_tokens: int = 8000) -> List[ContextFragment]:
        """
        Optimize context by selecting fragments with highest value.
        
        Returns fragments that fit within token budget.
        """
        # Rank fragments by value
        ranked_fragments = self.rank_context_fragments(fragments)
        
        # Select fragments until we hit the token limit
        selected_fragments = []
        total_tokens = 0
        
        for fragment in ranked_fragments:
            if total_tokens + fragment.tokens <= max_tokens:
                selected_fragments.append(fragment)
                total_tokens += fragment.tokens
            else:
                # Try to include a partial fragment or skip
                # For now, we'll stop here
                break
                
        return selected_fragments
    
    def get_context_summary(self, fragments: List[ContextFragment]) -> Dict[str, Any]:
        """Get summary statistics for context fragments."""
        if not fragments:
            return {
                'total_fragments': 0,
                'total_tokens': 0,
                'average_value': 0.0,
                'max_value': 0.0,
                'min_value': 0.0
            }
            
        total_tokens = sum(f.tokens for f in fragments)
        values = [self.calculate_context_value(f) for f in fragments]
        
        return {
            'total_fragments': len(fragments),
            'total_tokens': total_tokens,
            'average_value': sum(values) / len(values),
            'max_value': max(values),
            'min_value': min(values),
            'top_fragments': [
                {
                    'id': f.id,
                    'value': self.calculate_context_value(f),
                    'tokens': f.tokens,
                    'relevance': f.relevance,
                    'confidence': f.confidence
                }
                for f in self.rank_context_fragments(fragments)[:5]
            ]
        }

# Example usage function
def demo_context_scoring():
    """Demonstrate context value scoring."""
    
    scorer = ContextValueScorer()
    
    # Create sample fragments
    fragments = [
        ContextFragment(
            id="auth.ts",
            content="Authentication service implementation",
            relevance=0.96,
            confidence=0.95,
            freshness=0.9,
            tokens=1500,
            dependency_importance=0.9,
            file_path="src/auth.ts"
        ),
        ContextFragment(
            id="token.ts",
            content="Token management functions",
            relevance=0.94,
            confidence=0.92,
            freshness=0.85,
            tokens=1200,
            dependency_importance=0.85,
            file_path="src/token.ts"
        ),
        ContextFragment(
            id="README.md",
            content="Project documentation",
            relevance=0.42,
            confidence=0.3,
            freshness=0.2,
            tokens=2000,
            dependency_importance=0.1,
            file_path="README.md"
        ),
        ContextFragment(
            id="package.json",
            content="Dependencies and package info",
            relevance=0.31,
            confidence=0.25,
            freshness=0.3,
            tokens=800,
            dependency_importance=0.2,
            file_path="package.json"
        ),
        ContextFragment(
            id="old-auth-doc.md",
            content="Outdated authentication documentation",
            relevance=0.18,
            confidence=0.1,
            freshness=0.05,
            tokens=1500,
            dependency_importance=0.05,
            file_path="docs/old-auth-doc.md"
        )
    ]
    
    # Score all fragments
    print("Fragment Scores:")
    for fragment in fragments:
        score = scorer.calculate_context_value(fragment)
        print(f"  {fragment.id}: {score:.3f} (relevance:{fragment.relevance:.2f}, "
              f"tokens:{fragment.tokens})")
    
    # Get top fragments
    top_fragments = scorer.optimize_context_for_budget(fragments, 5000)
    print(f"\nTop fragments for 5000 token budget:")
    for fragment in top_fragments:
        score = scorer.calculate_context_value(fragment)
        print(f"  {fragment.id}: {score:.3f} ({fragment.tokens} tokens)")
    
    # Summary
    summary = scorer.get_context_summary(fragments)
    print(f"\nContext Summary:")
    print(f"  Total fragments: {summary['total_fragments']}")
    print(f"  Total tokens: {summary['total_tokens']}")
    print(f"  Average value: {summary['average_value']:.3f}")

if __name__ == "__main__":
    demo_context_scoring()