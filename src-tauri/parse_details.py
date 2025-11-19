#!/usr/bin/env python3
"""
Parse replay.details to extract player count and team information.
Based on sc2reader's DetailsReader.
"""
import sys
import mpyq

# Import sc2reader for the BitPackedDecoder
try:
    from sc2reader.decoders import BitPackedDecoder
except ImportError:
    print("Error: sc2reader not installed. Run: pip install sc2reader", file=sys.stderr)
    sys.exit(2)

def parse_details(replay_path):
    """Parse replay.details to extract player and team info"""
    archive = mpyq.MPQArchive(replay_path)
    details_data = archive.read_file("replay.details")

    # Use sc2reader's BitPackedDecoder with read_struct()
    decoder = BitPackedDecoder(details_data)
    details = decoder.read_struct()

    # details is a list where:
    # details[0] = array of players
    # details[1] = map name
    # etc.

    players = details[0]
    print(f"Total players: {len(players)}")

    # Count players per team
    # Each player is a dict/array where index 5 is the team ID
    teams = {}
    for p in players:
        # p[0] = name (blob)
        # p[5] = team (vint)
        # p[7] = observe (vint) - 0 for participants, 1+ for observers
        name = p[0].decode('utf-8', errors='replace')
        team = p[5]
        observe = p[7]

        print(f"  {name}: team={team}, observe={observe}")

        # Only count non-observers as players
        if observe == 0:
            if team not in teams:
                teams[team] = []
            teams[team].append(name)

    print(f"\nNon-observer teams:")
    for team_id, team_players in sorted(teams.items()):
        print(f"  Team {team_id}: {len(team_players)} players - {', '.join(team_players)}")

    # Calculate game type
    team_sizes = [len(players) for players in teams.values()]
    if len(team_sizes) == 0:
        game_type = "0v0 (no players)"
    elif len(team_sizes) > 2 and sum(team_sizes) == len(team_sizes):
        game_type = "FFA"
    else:
        game_type = "v".join(str(size) for size in sorted(team_sizes))

    print(f"\nDerived game type: {game_type}")

    is_1v1 = game_type == "1v1"
    print(f"Is 1v1: {is_1v1}")

    return is_1v1

if __name__ == "__main__":
    if len(sys.argv) != 2:
        print("Usage: parse_details.py <replay_file>")
        sys.exit(1)

    replay_path = sys.argv[1]
    try:
        is_1v1 = parse_details(replay_path)
        sys.exit(0 if is_1v1 else 1)
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        import traceback
        traceback.print_exc(file=sys.stderr)
        sys.exit(2)
