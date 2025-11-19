#!/usr/bin/env python3
"""
Simple script to check if a SC2 replay is a 1v1 game.
Exits with code 0 if 1v1, code 1 if not 1v1, code 2 on error.
Prints game_type to stdout for debugging.
"""
import sys
import os

# Add the bundled sc2reader to the path if it exists
script_dir = os.path.dirname(os.path.abspath(__file__))
sc2reader_path = os.path.join(script_dir, "python_modules")
if os.path.exists(sc2reader_path):
    sys.path.insert(0, sc2reader_path)

try:
    import sc2reader
except ImportError:
    print("sc2reader not found. Please install: pip install sc2reader", file=sys.stderr)
    sys.exit(2)

def main():
    if len(sys.argv) != 2:
        print("Usage: check_replay_type.py <replay_file>", file=sys.stderr)
        sys.exit(2)

    replay_path = sys.argv[1]

    try:
        # Load replay with minimal parsing (load_level=1 for basic info only)
        replay = sc2reader.load_replay(replay_path, load_level=1)

        # Check game type
        game_type = getattr(replay, 'game_type', None)
        player_count = len(replay.players)

        # Print debug info
        print(f"game_type={game_type}, players={player_count}", file=sys.stderr)

        # Exit code 0 if 1v1 (2 players), otherwise 1
        # We can also check game_type if available
        is_1v1 = player_count == 2
        if game_type:
            is_1v1 = is_1v1 and game_type in ('1v1', '1v1AI', 'AutoMM')

        sys.exit(0 if is_1v1 else 1)

    except Exception as e:
        print(f"Error parsing replay: {e}", file=sys.stderr)
        import traceback
        traceback.print_exc(file=sys.stderr)
        sys.exit(2)

if __name__ == "__main__":
    main()
