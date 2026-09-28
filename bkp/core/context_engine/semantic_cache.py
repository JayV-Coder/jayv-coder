"""
Semantic Context Cache for Jev AI Orchestrator
Caches context fragments to avoid redundant RAG operations.
"""

import hashlib
import json
import time
from typing import Dict, List, Any, Optional
from dataclasses import dataclass

@dataclass
class CachedContext:
    """Represents cached context with metadata."""
    
    cache_key: str
    query: str
    repository_hash: str
    git_commit: str
    semantic_query: str
    retrieved_symbols: List[str]
    file_hashes: Dict[str, str]
    context: Dict[str, Any]
    semantic_similarity: float
    created_at: float
    expires_at: float
    hits: int = 0

class SemanticContextCache:
    """Semantic caching mechanism for context fragments."""
    
    def __init__(self, ttl_seconds: int = 3600, max_entries: int = 1000):
        self.ttl_seconds = ttl_seconds
        self.max_entries = max_entries
        self.cache: Dict[str, CachedContext] = {}
        self.access_times: Dict[str, float] = {}
        self.stats = {
            'hits': 0,
            'misses': 0,
            'evictions': 0
        }
        
    def _generate_cache_key(self, query: str, repository_hash: str, git_commit: str,
                          semantic_query: str, retrieved_symbols: List[str],
                          file_hashes: Dict[str, str]) -> str:
        """Generate a consistent cache key for the context."""
        # Create a unique identifier for this context
        key_string = f"{query}|{repository_hash}|{git_commit}|{semantic_query}|{sorted(retrieved_symbols)}|{sorted(file_hashes.items())}"
        return hashlib.md5(key_string.encode()).hexdigest()
    
    def cache_context(self, query: str, repository_hash: str, git_commit: str,
                     semantic_query: str, retrieved_symbols: List[str],
                     file_hashes: Dict[str, str], context: Dict[str, Any],
                     semantic_similarity: float = 0.0) -> str:
        """
        Cache a context fragment.
        
        Returns:
            Cache key for the cached context
        """
        # Generate cache key
        cache_key = self._generate_cache_key(
            query, repository_hash, git_commit, semantic_query,
            retrieved_symbols, file_hashes
        )
        
        # Create cached context
        cached = CachedContext(
            cache_key=cache_key,
            query=query,
            repository_hash=repository_hash,
            git_commit=git_commit,
            semantic_query=semantic_query,
            retrieved_symbols=retrieved_symbols,
            file_hashes=file_hashes,
            context=context,
            semantic_similarity=semantic_similarity,
            created_at=time.time(),
            expires_at=time.time() + self.ttl_seconds
        )
        
        # Store in cache
        self.cache[cache_key] = cached
        self.access_times[cache_key] = time.time()
        
        # Manage cache size
        self._manage_cache_size()
        
        return cache_key
    
    def get_cached_context(self, query: str, repository_hash: str, git_commit: str,
                          semantic_query: str, retrieved_symbols: List[str],
                          file_hashes: Dict[str, str]) -> Optional[Dict[str, Any]]:
        """
        Retrieve cached context if available and not expired.
        
        Returns:
            Cached context or None if not found/expired
        """
        # Generate cache key
        cache_key = self._generate_cache_key(
            query, repository_hash, git_commit, semantic_query,
            retrieved_symbols, file_hashes
        )
        
        # Check if cache entry exists
        if cache_key not in self.cache:
            self.stats['misses'] += 1
            return None
            
        cached = self.cache[cache_key]
        
        # Check expiration
        if time.time() > cached.expires_at:
            # Remove expired entry
            del self.cache[cache_key]
            del self.access_times[cache_key]
            self.stats['misses'] += 1
            return None
            
        # Update access time and hit counter
        self.access_times[cache_key] = time.time()
        cached.hits += 1
        self.stats['hits'] += 1
        
        return cached.context
    
    def invalidate_cache(self, cache_key: str) -> bool:
        """Invalidate a specific cache entry."""
        if cache_key in self.cache:
            del self.cache[cache_key]
            del self.access_times[cache_key]
            return True
        return False
    
    def invalidate_by_repository(self, repository_hash: str) -> int:
        """Invalidate all cache entries for a repository."""
        invalidated = 0
        keys_to_remove = []
        
        for cache_key, cached in self.cache.items():
            if cached.repository_hash == repository_hash:
                keys_to_remove.append(cache_key)
        
        for cache_key in keys_to_remove:
            del self.cache[cache_key]
            del self.access_times[cache_key]
            invalidated += 1
            
        return invalidated
    
    def get_cache_stats(self) -> Dict[str, Any]:
        """Get cache statistics."""
        return {
            'hits': self.stats['hits'],
            'misses': self.stats['misses'],
            'evictions': self.stats['evictions'],
            'entries': len(self.cache),
            'ttl_seconds': self.ttl_seconds
        }
    
    def _manage_cache_size(self):
        """Manage cache size by removing oldest entries if needed."""
        if len(self.cache) <= self.max_entries:
            return
            
        # Remove oldest entries
        sorted_keys = sorted(self.access_times.items(), key=lambda x: x[1])
        keys_to_remove = [k for k, _ in sorted_keys[:len(sorted_keys) - self.max_entries]]
        
        for cache_key in keys_to_remove:
            if cache_key in self.cache:
                del self.cache[cache_key]
                del self.access_times[cache_key]
                self.stats['evictions'] += 1

# Example usage function
def demo_semantic_cache():
    """Demonstrate semantic caching functionality."""
    
    # Create cache
    cache = SemanticContextCache(ttl_seconds=3600)
    
    # Create some context data
    context = {
        "files": ["auth.service.ts", "token.service.ts"],
        "content": "Authentication service implementation",
        "related_symbols": ["AuthService", "TokenService"]
    }
    
    # Cache context
    cache_key = cache.cache_context(
        query="Implement authentication",
        repository_hash="abc123",
        git_commit="commit_hash_123",
        semantic_query="authentication implementation",
        retrieved_symbols=["AuthService", "TokenService"],
        file_hashes={"auth.service.ts": "hash1", "token.service.ts": "hash2"},
        context=context,
        semantic_similarity=0.95
    )
    
    print(f"Cached context with key: {cache_key}")
    
    # Retrieve cached context
    cached = cache.get_cached_context(
        query="Implement authentication",
        repository_hash="abc123",
        git_commit="commit_hash_123",
        semantic_query="authentication implementation",
        retrieved_symbols=["AuthService", "TokenService"],
        file_hashes={"auth.service.ts": "hash1", "token.service.ts": "hash2"}
    )
    
    print(f"Retrieved cached context: {cached is not None}")
    
    # Check cache stats
    stats = cache.get_cache_stats()
    print(f"Cache stats: {stats}")

if __name__ == "__main__":
    demo_semantic_cache()