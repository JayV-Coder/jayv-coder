""" 
Repository RAG
Handles retrieval relevant context repository advanced features.
"""

import logging
import os
import hashlib
import pickle
import json
import time
from typing import List, Dict, Any, Optional
from pathlib import Path

# For real implementation we'll use these libraries (commented for now)
# from sentence_transformers import SentenceTransformer
# import chromadb
# import nltk

logger = logging.getLogger(__name__)

class RepositoryRAG:
    """ 
    Repository RAG system retrieving relevant context features.
    """

    def __init__(self, cache_dir: str = ".jev_cache"):
        """Initialize RAG system."""
        self.cache_dir = cache_dir
        self.indexed_files = {}
        self.vector_store = {}
        self.metadata_index = {}
        self.file_hashes = {}
        
        # Create cache directory if it doesn't exist
        os.makedirs(cache_dir, exist_ok=True)
        
        logger.info("Repository RAG initialized")

    def index_repository(self, repo_path: str = ".", force_reindex: bool = False) -> bool:
        """ 
        Index repository for RAG with incremental updates.

        Args:
            repo_path: Path to repository
            force_reindex: Whether to force full reindexing

        Returns:
            True if successful, False otherwise
        """
        try:
            logger.info(f"Indexing repository {repo_path}")
            
            # Check if cached index is still valid
            cache_file = os.path.join(self.cache_dir, "index_cache.pkl")
            
            # Check if we should force reindexing
            if force_reindex:
                logger.info("Force reindexing requested")
            else:
                # Check if cache file exists and is recent enough
                if os.path.exists(cache_file):
                    try:
                        # Check file modification time
                        cache_mtime = os.path.getmtime(cache_file)
                        current_time = time.time()
                        
                        # Cache is valid if less than 1 hour old
                        if current_time - cache_mtime < 3600:  # 1 hour
                            with open(cache_file, 'rb') as f:
                                cached_data = pickle.load(f)
                            
                            self.indexed_files = cached_data.get('indexed_files', {})
                            self.metadata_index = cached_data.get('metadata_index', {})
                            logger.info("Using cached index (valid)")
                            return True
                        else:
                            logger.info("Cache expired, reindexing")
                    except Exception as e:
                        logger.warning(f"Cache invalid or corrupted: {e}. Reindexing...")
            
            # Perform actual indexing
            self._perform_indexing(repo_path)
            
            # Save to cache
            cache_data = {
                'indexed_files': self.indexed_files,
                'metadata_index': self.metadata_index
            }
            
            with open(cache_file, 'wb') as f:
                pickle.dump(cache_data, f)
                
            logger.info("Repository indexed successfully")
            return True
            
        except Exception as e:
            logger.error(f"Failed to index repository: {e}")
            return False

    def _perform_indexing(self, repo_path: str):
        """Perform actual indexing of repository files."""
        # Walk through repository and index files
        logger.info("Performing real indexing...")
        
        # Clear existing index
        self.indexed_files.clear()
        self.metadata_index.clear()
        
        # Get all Python and Markdown files
        excluded_dirs = ['.git', '__pycache__', '.venv', 'node_modules']
        supported_extensions = ['.py', '.md', '.txt', '.yaml', '.yml', '.json']
        
        try:
            for root, dirs, files in os.walk(repo_path):
                # Skip excluded directories
                dirs[:] = [d for d in dirs if d not in excluded_dirs]
                
                for file in files:
                    if any(file.endswith(ext) for ext in supported_extensions):
                        file_path = os.path.join(root, file)
                        
                        # Get relative path
                        rel_path = os.path.relpath(file_path, repo_path)
                        
                        # Calculate file hash
                        file_hash = self._calculate_file_hash(file_path)
                        
                        # Get file content
                        try:
                            with open(file_path, 'r', encoding='utf-8') as f:
                                content = f.read()
                        except Exception:
                            # If we can't read the file, try with latin-1 encoding
                            try:
                                with open(file_path, 'r', encoding='latin-1') as f:
                                    content = f.read()
                            except Exception:
                                content = ""
                        
                        # Index the file
                        self.indexed_files[rel_path] = {
                            'size': len(content),
                            'language': self._detect_language(rel_path),
                            'last_modified': self._get_file_modification_time(file_path),
                            'hash': file_hash,
                            'chunks': self._extract_chunks(content, rel_path)
                        }
                        
                        # Build metadata
                        self.metadata_index[rel_path] = {
                            'path': rel_path,
                            'language': self._detect_language(rel_path),
                            'dependencies': self._get_dependencies(content, rel_path),
                            'imports': self._get_imports(content, rel_path),
                            'exports': self._get_exports(content, rel_path),
                            'git_hash': file_hash,
                            'last_modified': self._get_file_modification_time(file_path),
                            'content_preview': content[:200] + "..." if len(content) > 200 else content
                        }
            
            logger.debug(f"Indexed {len(self.indexed_files)} files")
            
        except Exception as e:
            logger.error(f"Error during indexing: {e}")
            # Even if there's an error, we'll keep what we have so far
            pass

    def _calculate_file_hash(self, file_path: str) -> str:
        """Calculate SHA256 hash of file content."""
        try:
            with open(file_path, 'rb') as f:
                file_hash = hashlib.sha256()
                for chunk in iter(lambda: f.read(4096), b""):
                    file_hash.update(chunk)
                return file_hash.hexdigest()[:16]
        except Exception:
            # Fallback to MD5 if SHA256 fails
            try:
                with open(file_path, 'rb') as f:
                    file_hash = hashlib.md5()
                    for chunk in iter(lambda: f.read(4096), b""):
                        file_hash.update(chunk)
                    return file_hash.hexdigest()[:16]
            except Exception:
                return "unknown"

    def _detect_language(self, file_path: str) -> str:
        """Detect programming language based on file extension."""
        ext = os.path.splitext(file_path)[1].lower()
        language_map = {
            '.py': 'python',
            '.js': 'javascript',
            '.jsx': 'javascript',
            '.ts': 'typescript',
            '.tsx': 'typescript',
            '.html': 'html',
            '.css': 'css',
            '.md': 'markdown',
            '.txt': 'text',
            '.yaml': 'yaml',
            '.yml': 'yaml',
            '.json': 'json'
        }
        return language_map.get(ext, 'unknown')

    def _get_file_modification_time(self, file_path: str) -> str:
        """Get file modification time."""
        try:
            import datetime
            mod_time = os.path.getmtime(file_path)
            return datetime.datetime.fromtimestamp(mod_time).isoformat()
        except Exception:
            return "unknown"

    def _extract_chunks(self, content: str, file_path: str) -> List[Dict[str, Any]]:
        """Extract semantic chunks from file content."""
        # Simple paragraph-based chunking for now
        # In a real implementation, we would use semantic chunking
        
        if not content:
            return []
            
        # Split by paragraphs
        paragraphs = [p.strip() for p in content.split('\n\n') if p.strip()]
        
        chunks = []
        for i, paragraph in enumerate(paragraphs):
            chunks.append({
                'id': f"{file_path}_chunk_{i}",
                'content': paragraph,
                'start_line': i * 5,  # Approximate line number
                'end_line': (i + 1) * 5,
                'tokens': len(paragraph.split())  # Approximate token count
            })
            
        return chunks

    def _get_dependencies(self, content: str, file_path: str) -> List[str]:
        """Extract dependencies from file content."""
        deps = []
        
        # For Python files, look for import statements
        if file_path.endswith('.py'):
            lines = content.split('\n')
            for line in lines:
                line = line.strip()
                if line.startswith('import ') or line.startswith('from '):
                    # Extract module names
                    if 'import ' in line:
                        parts = line.split('import ')[1].split(',')
                        for part in parts:
                            dep = part.strip().split('.')[0]  # Get base module name
                            if dep and dep != '__future__':
                                deps.append(dep)
        
        # For requirements.txt files
        elif file_path.endswith('requirements.txt'):
            lines = content.split('\n')
            for line in lines:
                line = line.strip()
                if line and not line.startswith('#'):
                    # Remove version specifiers
                    dep = line.split('==')[0].split('>=')[0].split('<=')[0].split('>')[0].split('<')[0].strip()
                    if dep:
                        deps.append(dep)
        
        return list(set(deps))  # Remove duplicates

    def _get_imports(self, content: str, file_path: str) -> List[str]:
        """Extract imports from file content."""
        imports = []
        
        if file_path.endswith('.py'):
            lines = content.split('\n')
            for line in lines:
                line = line.strip()
                if line.startswith('import ') or line.startswith('from '):
                    # Extract module names
                    if 'import ' in line:
                        parts = line.split('import ')[1].split(',')
                        for part in parts:
                            imp = part.strip().split('.')[0]  # Get base module name
                            if imp and imp != '__future__':
                                imports.append(imp)
                    elif line.startswith('from '):
                        parts = line.split('import ')
                        if len(parts) > 1:
                            module = parts[0].split('from ')[1].split('.')[0]
                            imports.append(module)
        
        return list(set(imports))  # Remove duplicates

    def _get_exports(self, content: str, file_path: str) -> List[str]:
        """Extract exports from file content."""
        exports = []
        
        if file_path.endswith('.py'):
            lines = content.split('\n')
            for line in lines:
                line = line.strip()
                if line.startswith('def ') or line.startswith('class '):
                    # Extract function/class names
                    if line.startswith('def '):
                        func_name = line.split('def ')[1].split('(')[0]
                        exports.append(func_name)
                    elif line.startswith('class '):
                        class_name = line.split('class ')[1].split('(')[0]
                        exports.append(class_name)
        
        return exports
    
    def _extract_chunks_mock(self, file_path: str) -> List[Dict[str, Any]]:
        """Extract semantic chunks from a file (mock implementation)."""
        # In a real implementation, this would parse the file and extract meaningful chunks
        # For now, we return a mock structure
        return [
            {
                'content': f"Content of {file_path}",
                'start_line': 1,
                'end_line': 10,
                'type': 'generic',
                'embedding': [0.1, 0.2, 0.3, 0.4]  # Mock embedding
            }
        ]
    
    def _get_dependencies_mock(self, file_path: str) -> List[str]:
        """Get dependencies from a file (mock implementation)."""
        # In a real implementation, this would parse the file and extract dependencies
        if 'requirements.txt' in file_path:
            return ['numpy', 'pandas']
        elif 'setup.py' in file_path:
            return ['setuptools', 'wheel']
        return []
    
    def _get_imports_mock(self, file_path: str) -> List[str]:
        """Get imports from a file (mock implementation)."""
        # In a real implementation, this would parse the file and extract imports
        if 'src/main.py' in file_path:
            return ['os', 'sys', 'utils']
        elif 'src/utils.py' in file_path:
            return ['json', 'logging']
        return []
    
    def _get_exports_mock(self, file_path: str) -> List[str]:
        """Get exports from a file (mock implementation)."""
        # In a real implementation, this would parse the file and extract exports
        if 'src/main.py' in file_path:
            return ['main', 'run_app']
        elif 'src/utils.py' in file_path:
            return ['helper', 'process_data']
        return []
    
    def search(self, query: str, num_results: int = 5, 
               search_types: List[str] = None) -> List[Dict[str, Any]]:
        """
        Search for relevant context in the repository.
        
        Args:
            query: Search query
            num_results: Number of results to return
            search_types: Types of search to perform (semantic, lexical, symbol, etc.)
            
        Returns:
            List of relevant context items
        """
        logger.info(f"Searching for: {query}")
        
        # In a real implementation, this would:
        # 1. Convert query to embedding
        # 2. Search vector store
        # 3. Apply hybrid search (semantic + lexical + symbol)
        # 4. Return ranked results
        
        # Mock search implementation
        results = self._mock_search(query, num_results, search_types)
        
        logger.debug(f"Search returned {len(results)} results")
        return results
    
    def _mock_search(self, query: str, num_results: int, 
                     search_types: List[str] = None) -> List[Dict[str, Any]]:
        """Mock search implementation."""
        # Simple keyword-based search for demo
        query_lower = query.lower()
        
        # Determine search types to use
        if search_types is None:
            search_types = ['semantic', 'lexical']
        
        # Mock relevant files based on query keywords
        relevant_files = []
        
        if 'auth' in query_lower or 'token' in query_lower:
            relevant_files.extend([
                {'path': 'src/auth.py', 'score': 0.95, 'type': 'semantic'},
                {'path': 'src/token_manager.py', 'score': 0.85, 'type': 'semantic'},
                {'path': 'AGENTS.md', 'score': 0.75, 'type': 'lexical'}
            ])
        elif 'code' in query_lower or 'fix' in query_lower:
            relevant_files.extend([
                {'path': 'src/main.py', 'score': 0.90, 'type': 'semantic'},
                {'path': 'src/utils.py', 'score': 0.80, 'type': 'semantic'},
                {'path': 'requirements.txt', 'score': 0.70, 'type': 'lexical'}
            ])
        elif 'test' in query_lower:
            relevant_files.extend([
                {'path': 'tests/test_main.py', 'score': 0.95, 'type': 'semantic'},
                {'path': 'tests/test_utils.py', 'score': 0.85, 'type': 'semantic'}
            ])
        else:
            # Default to some files
            relevant_files.extend([
                {'path': 'README.md', 'score': 0.95, 'type': 'lexical'},
                {'path': 'AGENTS.md', 'score': 0.85, 'type': 'lexical'},
                {'path': 'src/main.py', 'score': 0.75, 'type': 'semantic'}
            ])
        
        # Sort by score and limit results
        relevant_files.sort(key=lambda x: x['score'], reverse=True)
        return relevant_files[:num_results]
    
    def get_file_content(self, file_path: str) -> str:
        """
        Get content of a specific file.
        
        Args:
            file_path: Path to the file
            
        Returns:
            File content or empty string if not found
        """
        # In a real implementation, this would read the actual file content
        # For now, return mock content
        mock_content = {
            'src/main.py': '#!/usr/bin/env python3\nprint("Hello from main")\n',
            'src/utils.py': 'def helper():\n    return "helper"\n',
            'README.md': '# Jev AI Orchestrator\nThis is a demo project.\n',
            'AGENTS.md': '# Development Agents\n- Developer\n- Security Expert\n',
            'requirements.txt': 'numpy==1.24.0\npandas==1.5.0\n',
            'setup.py': 'from setuptools import setup\nsetup(name="jev")\n'
        }
        
        return mock_content.get(file_path, '')
    
    def get_relevant_context(self, query: str, num_files: int = 3, 
                           include_metadata: bool = True) -> Dict[str, Any]:
        """
        Get relevant context for a query.
        
        Args:
            query: Query to search for
            num_files: Number of files to include
            include_metadata: Whether to include metadata
            
        Returns:
            Dictionary with formatted context and metadata
        """
        logger.info(f"Getting relevant context for: {query}")
        
        # Search for relevant files
        relevant_files = self.search(query, num_files)
        
        # Build context from relevant files
        context_parts = []
        metadata_parts = {}
        
        for file_info in relevant_files:
            file_path = file_info['path']
            content = self.get_file_content(file_path)
            
            context_parts.append({
                'path': file_path,
                'content': content,
                'metadata': self.metadata_index.get(file_path, {})
            })
        
        context = "\n".join([f"File: {item['path']}\n{item['content']}\n" for item in context_parts])
        
        result = {
            'query': query,
            'context': context,
            'files': context_parts,
            'count': len(context_parts)
        }
        
        if include_metadata:
            result['metadata'] = {
                'search_results': relevant_files,
                'total_files': len(self.indexed_files)
            }
        
        logger.debug(f"Generated context with {len(context_parts)} files")
        return result
    
    def get_file_metadata(self, file_path: str) -> Optional[Dict[str, Any]]:
        """
        Get metadata for a specific file.
        
        Args:
            file_path: Path to the file
            
        Returns:
            File metadata or None if not found
        """
        return self.metadata_index.get(file_path)
    
    def get_project_summary(self) -> Dict[str, Any]:
        """
        Get a summary of the project.
        
        Returns:
            Project summary dictionary
        """
        return {
            'total_files': len(self.indexed_files),
            'languages': list(set(item.get('language', 'unknown') 
                                for item in self.indexed_files.values())),
            'dependencies': list(set(dep for item in self.indexed_files.values() 
                                   for dep in item.get('dependencies', []))),
            'last_updated': '2026-09-21'
        }
    
    def update_file(self, file_path: str, new_content: str):
        """
        Update a file in the index.
        
        Args:
            file_path: Path to the file
            new_content: New content for the file
        """
        # In a real implementation, this would update the file in the index
        # and regenerate embeddings if needed
        
        file_hash = hashlib.md5(new_content.encode()).hexdigest()[:8]
        
        if file_path in self.indexed_files:
            self.indexed_files[file_path]['hash'] = file_hash
            self.indexed_files[file_path]['last_modified'] = '2026-09-21'
            
            # Update metadata
            self.metadata_index[file_path]['last_modified'] = '2026-09-21'
            self.metadata_index[file_path]['git_hash'] = file_hash
        else:
            # Add new file
            self.indexed_files[file_path] = {
                'size': len(new_content),
                'language': 'python' if file_path.endswith('.py') else 'markdown',
                'last_modified': '2026-09-21',
                'hash': file_hash,
                'chunks': self._extract_chunks_mock(file_path)
            }
            
            # Add metadata
            self.metadata_index[file_path] = {
                'path': file_path,
                'language': 'python' if file_path.endswith('.py') else 'markdown',
                'dependencies': self._get_dependencies_mock(file_path),
                'imports': self._get_imports_mock(file_path),
                'exports': self._get_exports_mock(file_path),
                'git_hash': file_hash,
                'last_modified': '2026-09-21'
            }
        
        logger.info(f"Updated file: {file_path}")

    def get_changed_files(self, since: str = None) -> List[str]:
        """
        Get list of changed files since a certain point.
        
        Args:
            since: Timestamp or reference point
            
        Returns:
            List of changed file paths
        """
        # In a real implementation, this would use Git or other version control
        # For now, return all files
        return list(self.indexed_files.keys())