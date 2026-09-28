"""
Terminal User Interface for Jev AI Orchestrator
"""

import curses
import asyncio
import logging
from typing import Dict, Any, Optional
from core.orchestrator import JevOrchestrator

logger = logging.getLogger(__name__)

class JevTUI:
    """
    Terminal User Interface for Jev AI Orchestrator.
    """
    
    def __init__(self, orchestrator: JevOrchestrator):
        """
        Initialize the TUI.
        
        Args:
            orchestrator: Jev orchestrator instance
        """
        self.orchestrator = orchestrator
        self.stdscr = None
        self.current_input = ""
        self.history = []
        self.session_id = "default"
        
        # UI dimensions
        self.height = 0
        self.width = 0
        
        logger.info("Jev TUI initialized")
    
    def run(self):
        """
        Run the TUI application.
        """
        try:
            curses.wrapper(self._main_loop)
        except KeyboardInterrupt:
            logger.info("TUI interrupted by user")
        except Exception as e:
            logger.error(f"TUI error: {e}")
    
    def _main_loop(self, stdscr):
        """
        Main TUI loop.
        
        Args:
            stdscr: Curses standard screen
        """
        self.stdscr = stdscr
        curses.curs_set(1)  # Show cursor
        self.stdscr.nodelay(False)
        self.stdscr.timeout(100)  # Refresh every 100ms
        
        # Get initial screen dimensions
        self.height, self.width = self.stdscr.getmaxyx()
        
        # Initialize colors if supported
        if curses.has_colors():
            curses.start_color()
            curses.init_pair(1, curses.COLOR_WHITE, curses.COLOR_BLACK)
            curses.init_pair(2, curses.COLOR_CYAN, curses.COLOR_BLACK)
            curses.init_pair(3, curses.COLOR_GREEN, curses.COLOR_BLACK)
            curses.init_pair(4, curses.COLOR_YELLOW, curses.COLOR_BLACK)
            curses.init_pair(5, curses.COLOR_RED, curses.COLOR_BLACK)
        
        # Main loop
        while True:
            self._draw_screen()
            key = self.stdscr.getch()
            
            if key == ord('q') or key == curses.KEY_EXIT:
                break
            elif key == curses.KEY_BACKSPACE or key == 127 or key == 8:
                self.current_input = self.current_input[:-1]
            elif key == ord('\n'):
                if self.current_input.strip():
                    self._process_input()
            elif 32 <= key <= 126:  # Printable characters
                self.current_input += chr(key)
            
            # Handle window resize
            if key == curses.KEY_RESIZE:
                self.height, self.width = self.stdscr.getmaxyx()
    
    def _draw_screen(self):
        """
        Draw the entire screen.
        """
        if not self.stdscr:
            return
            
        self.stdscr.clear()
        
        # Draw header
        self._draw_header()
        
        # Draw input area
        self._draw_input_area()
        
        # Draw output area
        self._draw_output_area()
        
        # Draw footer
        self._draw_footer()
        
        self.stdscr.refresh()
    
    def _draw_header(self):
        """
        Draw the header section.
        """
        header_text = "Jev AI Orchestrator"
        project_text = "project: jev-project"
        branch_text = "main"
        
        # Center the header
        header_y = 0
        header_x = max(0, (self.width - len(header_text)) // 2)
        project_x = max(0, (self.width - len(project_text)) // 2)
        branch_x = max(0, (self.width - len(branch_text)) // 2)
        
        if self.width > len(header_text):
            self.stdscr.addstr(header_y, header_x, header_text, curses.color_pair(2))
        
        if self.width > len(project_text):
            self.stdscr.addstr(header_y + 1, project_x, project_text, curses.color_pair(3))
        
        if self.width > len(branch_text):
            self.stdscr.addstr(header_y + 2, branch_x, branch_text, curses.color_pair(4))
    
    def _draw_input_area(self):
        """
        Draw the input area.
        """
        input_y = 5
        input_x = 0
        
        # Draw input prompt
        self.stdscr.addstr(input_y, input_x, "> ", curses.color_pair(2))
        
        # Draw current input
        input_display = self.current_input[-(self.width - 3):]  # Truncate if too long
        self.stdscr.addstr(input_y, input_x + 2, input_display)
        
        # Position cursor at the end of input
        self.stdscr.move(input_y, input_x + 2 + len(input_display))
    
    def _draw_output_area(self):
        """
        Draw the output area.
        """
        output_y = 8
        output_x = 0
        
        # Draw recent history
        for i, entry in enumerate(self.history[-(self.height - output_y - 5):]):
            if output_y + i >= self.height - 3:
                break
                
            if isinstance(entry, dict) and 'result' in entry:
                # Show the response from the orchestrator
                response = entry['result'].get('response', 'No response')
                response_lines = response.split('\n')
                
                for j, line in enumerate(response_lines[:self.height - output_y - i - 5]):
                    if output_y + i + j >= self.height - 3:
                        break
                    self.stdscr.addstr(output_y + i + j, output_x, line[:self.width])
            elif isinstance(entry, str):
                # Show plain text
                self.stdscr.addstr(output_y + i, output_x, entry[:self.width])
    
    def _draw_footer(self):
        """
        Draw the footer section.
        """
        footer_y = self.height - 3
        footer_x = 0
        
        # Draw status bar
        status_text = "Tokens 0K | Context 0% | Cost $0.00 | agent:auto"
        if self.width > len(status_text):
            self.stdscr.addstr(footer_y, footer_x, status_text, curses.color_pair(1))
    
    def _process_input(self):
        """
        Process user input.
        """
        if not self.current_input.strip():
            return
            
        # Add to history
        self.history.append(f"> {self.current_input}")
        
        try:
            # Process through orchestrator
            result = self.orchestrator.process_request(self.current_input, self.session_id)
            
            # Add to history
            self.history.append(result)
            
            # Display result
            self._display_result(result)
            
        except Exception as e:
            error_msg = f"Error processing request: {e}"
            self.history.append(error_msg)
            logger.error(error_msg)
        
        # Clear input
        self.current_input = ""
    
    def _display_result(self, result: Dict[str, Any]):
        """
        Display the result in the output area.
        
        Args:
            result: Processing result from orchestrator
        """
        # In a real implementation, this would display the result properly
        # For now, we just add it to history
        pass