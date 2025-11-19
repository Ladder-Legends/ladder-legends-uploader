#!/usr/bin/env python3
"""
Inspect SC2 replay files to understand their structure.
This helps us write a minimal Rust parser.
"""
import sys
import sc2reader

def inspect_replay(replay_path):
    print(f"\n{'='*60}")
    print(f"Inspecting: {replay_path}")
    print(f"{'='*60}\n")

    # Load replay with more complete parsing
    replay = sc2reader.load_replay(replay_path, load_level=4)

    print(f"Game Type: {getattr(replay, 'game_type', 'N/A')}")
    print(f"Player Count: {len(replay.players) if replay.players else 0}")
    print(f"Real Type: {getattr(replay, 'real_type', 'N/A')}")
    print(f"Category: {getattr(replay, 'category', 'N/A')}")
    print(f"Teams: {len(replay.teams) if hasattr(replay, 'teams') else 'N/A'}")
    print(f"\nPlayers:")
    if replay.players:
        for i, player in enumerate(replay.players, 1):
            print(f"  {i}. {player.name} ({getattr(player, 'pick_race', 'Unknown')})")
    else:
        print("  No players found!")

    # Now let's look at the raw MPQ structure
    print(f"\n{'='*60}")
    print("MPQ Archive Contents:")
    print(f"{'='*60}\n")

    import mpyq
    archive = mpyq.MPQArchive(replay_path)

    for filename in archive.files:
        try:
            file_data = archive.read_file(filename)
            if file_data:
                print(f"{filename}: {len(file_data)} bytes")
            else:
                print(f"{filename}: (empty or null)")
        except:
            print(f"{filename}: (could not read)")

    # Let's examine replay.initData specifically
    print(f"\n{'='*60}")
    print("replay.initData Analysis:")
    print(f"{'='*60}\n")

    init_data = archive.read_file("replay.initData")
    print(f"Size: {len(init_data)} bytes")
    print(f"First 100 bytes (hex): {init_data[:100].hex()}")
    print(f"First 100 bytes (repr): {repr(init_data[:100])}")

    # Let's also check replay.details
    print(f"\n{'='*60}")
    print("replay.details Analysis:")
    print(f"{'='*60}\n")

    details = archive.read_file("replay.details")
    print(f"Size: {len(details)} bytes")
    print(f"First 100 bytes (hex): {details[:100].hex()}")

    # Search for player count in initData
    print(f"\n{'='*60}")
    print("Searching for player indicators:")
    print(f"{'='*60}\n")

    # In sc2reader, the player count is derived from parsing the bitpacked data
    # Let's see if we can find a simple pattern
    player_count = len(replay.players)
    print(f"Expected player count: {player_count}")

    # Look for the byte value matching player count in first 200 bytes
    for i in range(min(200, len(init_data))):
        if init_data[i] == player_count:
            context_start = max(0, i - 10)
            context_end = min(len(init_data), i + 10)
            print(f"Found {player_count} at offset {i}: {init_data[context_start:context_end].hex()}")

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: inspect_replay.py <replay1.SC2Replay> [replay2.SC2Replay ...]")
        sys.exit(1)

    for replay_path in sys.argv[1:]:
        try:
            inspect_replay(replay_path)
        except Exception as e:
            print(f"Error inspecting {replay_path}: {e}")
            import traceback
            traceback.print_exc()
