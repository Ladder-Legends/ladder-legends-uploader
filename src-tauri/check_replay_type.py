#!/usr/bin/env python3
"""
Simple script to check if a SC2 replay is a 1v1 game.
Exits with code 0 if 1v1, code 1 if not 1v1, code 2 on error.
"""
import sys
import sc2reader

def main():
    if len(sys.argv) != 2:
        print("Usage: check_replay_type.py <replay_file>", file=sys.stderr)
        sys.exit(2)

    replay_path = sys.argv[1]

    try:
        # Load replay with minimal parsing (load_level=1 for basic info only)
        replay = sc2reader.load_replay(replay_path, load_level=1)

        # Count number of players
        player_count = len(replay.players)

        # Exit code 0 if 1v1 (2 players), otherwise 1
        sys.exit(0 if player_count == 2 else 1)

    except Exception as e:
        print(f"Error parsing replay: {e}", file=sys.stderr)
        sys.exit(2)

if __name__ == "__main__":
    main()
