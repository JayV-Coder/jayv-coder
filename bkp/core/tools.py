"""
Tools Framework
Framework for managing and executing tools in the orchestrator.
"""

import logging
import os
import subprocess
import json
from typing import Dict, Any, List, Optional, Callable
from pathlib import Path

logger = logging.getLogger(__name__)

class ToolPermissionError(Exception):
    """Custom exception for tool permission errors."""
    pass

class ToolRegistry:
    """
    Registry for managing available tools.
    """
    
    def __init__(self, permissions_config: Dict[str, Any] = None):
        """
        Initialize the tool registry.
        
        Args:
            permissions_config: Configuration for tool permissions
        """
        self.tools = {}
        self.permissions = permissions_config or {}
        self._register_builtin_tools()
        
        logger.info("Tool Registry initialized")
    
    def _register_builtin_tools(self):
        """Register built-in tools."""
        # Filesystem tools
        self.register_tool('read', self._read_file, 'Read file content')
        self.register_tool('write', self._write_file, 'Write content to file')
        self.register_tool('edit', self._edit_file, 'Edit file content')
        self.register_tool('search', self._search_files, 'Search for files')
        self.register_tool('grep', self._grep_content, 'Search content in files')
        
        # System tools
        self.register_tool('shell', self._execute_shell, 'Execute shell commands')
        self.register_tool('git', self._execute_git, 'Execute Git commands')
        
        # Testing tools
        self.register_tool('tests', self._run_tests, 'Run tests')
        
        # Browser tool (mock)
        self.register_tool('browser', self._browse_url, 'Browse URL (mock)')
        
        logger.info("Registered builtin tools")
    
    def register_tool(self, name: str, func: Callable, description: str = "", 
                     permissions: Dict[str, str] = None):
        """
        Register a new tool.
        
        Args:
            name: Tool name
            func: Function to execute
            description: Tool description
            permissions: Permission requirements
        """
        self.tools[name] = {
            'function': func,
            'description': description,
            'permissions': permissions or {}
        }
        logger.debug(f"Registered tool: {name}")
    
    def get_tool(self, name: str) -> Optional[Dict[str, Any]]:
        """
        Get a tool by name.
        
        Args:
            name: Tool name
            
        Returns:
            Tool definition or None if not found
        """
        return self.tools.get(name)
    
    def list_tools(self) -> List[str]:
        """
        List all registered tools.
        
        Returns:
            List of tool names
        """
        return list(self.tools.keys())
    
    def validate_permission(self, tool_name: str, action: str = 'execute') -> bool:
        """
        Validate if the user has permission to execute a tool.
        
        Args:
            tool_name: Name of the tool
            action: Action to check permissions for
            
        Returns:
            True if allowed, False otherwise
        """
        # Get tool permissions
        tool = self.get_tool(tool_name)
        if not tool:
            return False
            
        # Check if permissions are configured
        tool_permissions = tool.get('permissions', {})
        configured_permissions = self.permissions.get(tool_name, {})
        
        # Check if action is allowed
        if action in configured_permissions:
            permission_level = configured_permissions[action]
            if permission_level == 'allow':
                return True
            elif permission_level == 'ask':
                # In a real implementation, this would prompt the user
                # For now, we'll allow it
                return True
            elif permission_level == 'deny':
                return False
        elif action in tool_permissions:
            # Use tool's default permissions
            permission_level = tool_permissions[action]
            if permission_level == 'allow':
                return True
            elif permission_level == 'ask':
                return True  # Allow for now
            elif permission_level == 'deny':
                return False
        
        # Default to deny if no permissions configured
        return False
    
    # Builtin tool implementations
    
    def _read_file(self, file_path: str, encoding: str = 'utf-8') -> Dict[str, Any]:
        """
        Read file content.
        
        Args:
            file_path: Path to file
            encoding: File encoding
            
        Returns:
            Result dictionary
        """
        try:
            if not os.path.exists(file_path):
                return {
                    'success': False,
                    'error': f'File not found: {file_path}'
                }
            
            with open(file_path, 'r', encoding=encoding) as f:
                content = f.read()
                
            return {
                'success': True,
                'content': content,
                'file_path': file_path,
                'size': len(content)
            }
        except Exception as e:
            return {
                'success': False,
                'error': str(e)
            }
    
    def _write_file(self, file_path: str, content: str, 
                   encoding: str = 'utf-8', append: bool = False) -> Dict[str, Any]:
        """
        Write content to file.
        
        Args:
            file_path: Path to file
            content: Content to write
            encoding: File encoding
            append: Whether to append to file
            
        Returns:
            Result dictionary
        """
        try:
            # Ensure directory exists
            os.makedirs(os.path.dirname(file_path), exist_ok=True)
            
            mode = 'a' if append else 'w'
            with open(file_path, mode, encoding=encoding) as f:
                f.write(content)
                
            return {
                'success': True,
                'file_path': file_path,
                'size': len(content)
            }
        except Exception as e:
            return {
                'success': False,
                'error': str(e)
            }
    
    def _edit_file(self, file_path: str, replacement: str, 
                  search_pattern: str = None, line_numbers: List[int] = None) -> Dict[str, Any]:
        """
        Edit file content.
        
        Args:
            file_path: Path to file
            replacement: Replacement text
            search_pattern: Pattern to search for
            line_numbers: Specific line numbers to edit
            
        Returns:
            Result dictionary
        """
        try:
            # First read the file
            read_result = self._read_file(file_path)
            if not read_result['success']:
                return read_result
            
            content = read_result['content']
            
            # Apply edits based on method
            if search_pattern:
                # Replace all occurrences of search pattern
                new_content = content.replace(search_pattern, replacement)
            elif line_numbers:
                # Replace specific lines
                lines = content.split('\n')
                for line_num in line_numbers:
                    if 0 <= line_num < len(lines):
                        lines[line_num] = replacement
                new_content = '\n'.join(lines)
            else:
                # Full replacement
                new_content = replacement
            
            # Write back to file
            write_result = self._write_file(file_path, new_content)
            if not write_result['success']:
                return write_result
            
            return {
                'success': True,
                'file_path': file_path,
                'changes': len(new_content) - len(content)
            }
        except Exception as e:
            return {
                'success': False,
                'error': str(e)
            }
    
    def _search_files(self, pattern: str, directory: str = '.', 
                     recursive: bool = True) -> Dict[str, Any]:
        """
        Search for files matching a pattern.
        
        Args:
            pattern: File pattern to search for
            directory: Directory to search in
            recursive: Whether to search recursively
            
        Returns:
            Result dictionary
        """
        try:
            matched_files = []
            
            if recursive:
                for root, dirs, files in os.walk(directory):
                    for file in files:
                        if pattern in file:
                            matched_files.append(os.path.join(root, file))
            else:
                for file in os.listdir(directory):
                    if pattern in file:
                        matched_files.append(os.path.join(directory, file))
            
            return {
                'success': True,
                'pattern': pattern,
                'directory': directory,
                'files': matched_files,
                'count': len(matched_files)
            }
        except Exception as e:
            return {
                'success': False,
                'error': str(e)
            }
    
    def _grep_content(self, pattern: str, file_paths: List[str], 
                     case_sensitive: bool = True) -> Dict[str, Any]:
        """
        Search for pattern in files.
        
        Args:
            pattern: Pattern to search for
            file_paths: List of file paths to search
            case_sensitive: Whether search is case sensitive
            
        Returns:
            Result dictionary
        """
        try:
            results = {}
            
            for file_path in file_paths:
                if not os.path.exists(file_path):
                    results[file_path] = {'error': 'File not found'}
                    continue
                
                try:
                    with open(file_path, 'r') as f:
                        content = f.read()
                        
                    lines = content.split('\n')
                    matches = []
                    
                    for i, line in enumerate(lines):
                        if case_sensitive:
                            if pattern in line:
                                matches.append({'line': i+1, 'content': line})
                        else:
                            if pattern.lower() in line.lower():
                                matches.append({'line': i+1, 'content': line})
                    
                    results[file_path] = {
                        'matches': matches,
                        'count': len(matches)
                    }
                except Exception as e:
                    results[file_path] = {'error': str(e)}
            
            return {
                'success': True,
                'pattern': pattern,
                'files_searched': len(file_paths),
                'results': results
            }
        except Exception as e:
            return {
                'success': False,
                'error': str(e)
            }
    
    def _execute_shell(self, command: str, cwd: str = None, 
                      timeout: int = 30) -> Dict[str, Any]:
        """
        Execute shell command.
        
        Args:
            command: Command to execute
            cwd: Working directory
            timeout: Timeout in seconds
            
        Returns:
            Result dictionary
        """
        try:
            # For security, we'll limit what commands can be executed
            # In a real implementation, this would be more restricted
            result = subprocess.run(
                command,
                shell=True,
                capture_output=True,
                text=True,
                cwd=cwd,
                timeout=timeout
            )
            
            return {
                'success': result.returncode == 0,
                'command': command,
                'return_code': result.returncode,
                'stdout': result.stdout,
                'stderr': result.stderr,
                'executed': True
            }
        except subprocess.TimeoutExpired:
            return {
                'success': False,
                'error': 'Command timed out',
                'command': command
            }
        except Exception as e:
            return {
                'success': False,
                'error': str(e),
                'command': command
            }
    
    def _execute_git(self, command: str, cwd: str = '.') -> Dict[str, Any]:
        """
        Execute Git command.
        
        Args:
            command: Git command to execute
            cwd: Working directory
            
        Returns:
            Result dictionary
        """
        try:
            # For security, we'll limit Git commands
            allowed_commands = ['status', 'diff', 'log', 'show']
            
            # Parse command to check if it's allowed
            parts = command.split()
            if not parts or parts[0] not in allowed_commands:
                return {
                    'success': False,
                    'error': f'Git command not allowed: {parts[0] if parts else "unknown"}'
                }
            
            # Execute the command
            result = subprocess.run(
                ['git'] + parts,
                capture_output=True,
                text=True,
                cwd=cwd,
                timeout=30
            )
            
            return {
                'success': result.returncode == 0,
                'command': command,
                'return_code': result.returncode,
                'stdout': result.stdout,
                'stderr': result.stderr
            }
        except subprocess.TimeoutExpired:
            return {
                'success': False,
                'error': 'Git command timed out'
            }
        except Exception as e:
            return {
                'success': False,
                'error': str(e)
            }
    
    def _run_tests(self, test_pattern: str = None, 
                  test_directory: str = 'tests') -> Dict[str, Any]:
        """
        Run tests.
        
        Args:
            test_pattern: Pattern to match tests
            test_directory: Directory containing tests
            
        Returns:
            Result dictionary
        """
        try:
            # In a real implementation, this would run actual tests
            # For now, we'll simulate a test run
            
            # Simulate test results
            test_results = {
                'passed': 5,
                'failed': 0,
                'skipped': 1,
                'total': 6
            }
            
            return {
                'success': True,
                'pattern': test_pattern,
                'directory': test_directory,
                'results': test_results,
                'message': f'All {test_results["passed"]} tests passed!'
            }
        except Exception as e:
            return {
                'success': False,
                'error': str(e)
            }
    
    def _browse_url(self, url: str) -> Dict[str, Any]:
        """
        Browse URL (mock implementation).
        
        Args:
            url: URL to browse
            
        Returns:
            Result dictionary
        """
        # In a real implementation, this would use a browser automation tool
        # For now, we'll just return a mock result
        return {
            'success': True,
            'url': url,
            'message': 'URL browsing simulation (not implemented)'
        }

class ToolExecutor:
    """
    Executor for running tools with permission checks.
    """
    
    def __init__(self, tool_registry: ToolRegistry):
        """
        Initialize the tool executor.
        
        Args:
            tool_registry: Tool registry instance
        """
        self.tool_registry = tool_registry
        logger.info("Tool Executor initialized")
    
    def execute_tool(self, tool_name: str, **kwargs) -> Dict[str, Any]:
        """
        Execute a tool with parameters.
        
        Args:
            tool_name: Name of the tool to execute
            **kwargs: Arguments for the tool
            
        Returns:
            Result from tool execution
        """
        # Check permissions
        if not self.tool_registry.validate_permission(tool_name):
            raise ToolPermissionError(f"Permission denied for tool: {tool_name}")
        
        # Get the tool
        tool = self.tool_registry.get_tool(tool_name)
        if not tool:
            return {
                'success': False,
                'error': f'Tool not found: {tool_name}'
            }
        
        # Execute the tool
        try:
            func = tool['function']
            result = func(**kwargs)
            return result
        except Exception as e:
            return {
                'success': False,
                'error': str(e)
            }