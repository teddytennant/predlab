#!/usr/bin/env python3
"""
Entry point for the PredLab command.
"""

from .tui import main as tui_main

def main():
    tui_main()

if __name__ == "__main__":
    main()