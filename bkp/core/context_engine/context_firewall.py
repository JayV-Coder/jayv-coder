"""
Context Firewall for Jev AI Orchestrator
Protects sensitive information from being sent to external providers.
"""

import re
import fnmatch
from typing import Dict, List, Any, Optional, Tuple
from dataclasses import dataclass
from pathlib import Path

@dataclass
class FilePrivacyInfo:
    """Information about a file's privacy status."""
    
    file_path: str
    is_sensitive: bool
    reason: str
    severity: str  # 'high', 'medium', 'low'

class ContextFirewall:
    """Security layer that filters context to prevent sensitive data exposure."""
    
    def __init__(self, config: Dict[str, Any] = None):
        self.config = config or {
            'deny_patterns': [
                r'\.env',
                r'\.pem$',
                r'\.ssh/.*',
                r'secrets/.*',
                r'\.key$',
                r'\.secret$'
            ],
            'local_only_patterns': [
                r'internal/.*',
                r'private/.*'
            ],
            'redact_patterns': [
                'email',
                'ip_address',
                'mac_address',
                'uuid',
                'credit_card',
                'ssn'
            ]
        }
        
        # Compile patterns for performance
        self.deny_compiled = [re.compile(pattern) for pattern in self.config['deny_patterns']]
        self.local_only_compiled = [re.compile(pattern) for pattern in self.config['local_only_patterns']]
        
    def check_file_privacy(self, file_path: str) -> FilePrivacyInfo:
        """
        Check if a file contains sensitive information.
        
        Returns:
            FilePrivacyInfo with privacy status and reason
        """
        # Check deny patterns
        for pattern in self.deny_compiled:
            if pattern.search(file_path):
                return FilePrivacyInfo(
                    file_path=file_path,
                    is_sensitive=True,
                    reason="Denied pattern match",
                    severity="high"
                )
                
        # Check local-only patterns
        for pattern in self.local_only_compiled:
            if pattern.search(file_path):
                return FilePrivacyInfo(
                    file_path=file_path,
                    is_sensitive=True,
                    reason="Local-only pattern match",
                    severity="medium"
                )
        
        return FilePrivacyInfo(
            file_path=file_path,
            is_sensitive=False,
            reason="No privacy concerns",
            severity="low"
        )
    
    def filter_context(self, context: Dict[str, Any]) -> Dict[str, Any]:
        """
        Filter context to remove sensitive files and information.
        
        Returns:
            Filtered context with sensitive files removed
        """
        if 'files' not in context:
            return context
            
        filtered_files = []
        filtered_context = context.copy()
        
        for file_info in context['files']:
            # Handle both string paths and dict objects
            if isinstance(file_info, str):
                file_path = file_info
                file_details = {}
            elif isinstance(file_info, dict):
                file_path = file_info.get('path', '')
                file_details = file_info
            else:
                file_path = str(file_info)
                file_details = {}
                
            privacy_info = self.check_file_privacy(file_path)
            
            if not privacy_info.is_sensitive:
                filtered_files.append(file_info)
            else:
                # Log privacy violation (in production, you might want to log this)
                print(f"INFO: Filtering sensitive file from context: {file_path}")
        
        filtered_context['files'] = filtered_files
        return filtered_context
    
    def redact_secrets(self, content: str) -> str:
        """
        Redact sensitive information from content.
        
        Returns:
            Content with sensitive information redacted
        """
        # Simple redaction rules
        redactions = {
            r'\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b': '[EMAIL_REDACTED]',
            r'\b(?:\d{1,3}\.){3}\d{1,3}\b': '[IP_ADDRESS_REDACTED]',
            r'\b(?:[0-9a-fA-F]{2}:){5}[0-9a-fA-F]{2}\b': '[MAC_ADDRESS_REDACTED]',
            r'\b[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}\b': '[UUID_REDACTED]',
            r'\b(?:\d{4}[-\s]?){3}\d{4}\b': '[CREDIT_CARD_REDACTED]',
            r'\b\d{3}[-\s]?\d{2}[-\s]?\d{4}\b': '[SSN_REDACTED]'
        }
        
        redacted_content = content
        for pattern, replacement in redactions.items():
            redacted_content = re.sub(pattern, replacement, redacted_content)
            
        return redacted_content
    
    def validate_context_security(self, context: Dict[str, Any]) -> Tuple[bool, List[str]]:
        """
        Validate that context doesn't contain sensitive information.
        
        Returns:
            Tuple of (is_secure, list_of_violations)
        """
        violations = []
        is_secure = True
        
        # Check files
        if 'files' in context:
            for file_info in context['files']:
                if isinstance(file_info, str):
                    file_path = file_info
                elif isinstance(file_info, dict):
                    file_path = file_info.get('path', '')
                else:
                    file_path = str(file_info)
                    
                privacy_info = self.check_file_privacy(file_path)
                if privacy_info.is_sensitive:
                    violations.append(f"Sensitive file detected: {file_path} ({privacy_info.reason})")
                    is_secure = False
        
        # Check content for secrets
        if 'content' in context:
            redacted = self.redact_secrets(context['content'])
            if redacted != context['content']:
                violations.append("Potential secrets found in content")
                is_secure = False
                
        return is_secure, violations

# Example usage function
def demo_context_firewall():
    """Demonstrate context firewall functionality."""
    
    # Create firewall
    firewall = ContextFirewall()
    
    # Test privacy checking
    sensitive_files = [
        ".env",
        "src/main.py",
        ".ssh/id_rsa",
        "config.json",
        "secret.key"
    ]
    
    for file_path in sensitive_files:
        info = firewall.check_file_privacy(file_path)
        print(f"File {file_path}: sensitive={info.is_sensitive}")
    
    # Test context filtering
    context = {
        "files": [".env", "src/main.py", "app.py"],
        "content": "Main application code"
    }
    
    filtered = firewall.filter_context(context)
    print(f"Filtered context has sensitive files: {any(f['is_sensitive'] for f in filtered['files'])}")
    
    # Test secret redaction
    sensitive_content = "Contact email: test@example.com and IP: 192.168.1.1"
    redacted = firewall.redact_secrets(sensitive_content)
    print(f"Redacted content: {redacted}")
    
    # Test security validation
    is_secure, violations = firewall.validate_context_security(context)
    print(f"Context secure: {is_secure}")
    print(f"Violations: {violations}")

if __name__ == "__main__":
    demo_context_firewall()