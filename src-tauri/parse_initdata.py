#!/usr/bin/env python3
"""
Parse replay.initData to extract player count.
This is a simplified version based on sc2reader's InitDataReader.
"""
import sys
import mpyq

class BitPackedDecoder:
    """Minimal bitpacked decoder for SC2 replay data"""
    def __init__(self, data):
        self.data = data
        self.pos = 0
        self.bit_shift = 0
        self.next_byte = None

    def read_bits(self, count):
        """Read count bits from the bitpacked data"""
        result = 0
        bits_remaining = count

        # If we have a byte in progress, use it first
        if self.bit_shift != 0:
            bits_available = 8 - self.bit_shift

            if bits_available < bits_remaining:
                bits_remaining -= bits_available
                result = (self.next_byte >> self.bit_shift) << bits_remaining
            elif bits_available > bits_remaining:
                self.bit_shift += bits_remaining
                return (self.next_byte >> self.bit_shift - bits_remaining) & ((1 << bits_remaining) - 1)
            else:
                self.bit_shift = 0
                return self.next_byte >> self.bit_shift

        # Read whole bytes
        while bits_remaining >= 8:
            if self.pos >= len(self.data):
                return result
            byte = self.data[self.pos]
            self.pos += 1
            bits_remaining -= 8
            result = result | (byte << bits_remaining)

        # Read remaining bits
        if bits_remaining > 0:
            if self.pos >= len(self.data):
                return result
            self.next_byte = self.data[self.pos]
            self.pos += 1
            self.bit_shift = bits_remaining
            result = result | (self.next_byte & ((1 << bits_remaining) - 1))

        return result

    def read_uint8(self):
        """Read 8 bits as unsigned integer"""
        return self.read_bits(8)

    def read_bool(self):
        """Read 1 bit as boolean"""
        return self.read_bits(1) != 0

    def byte_align(self):
        """Move to the next byte boundary"""
        self.bit_shift = 0
        self.next_byte = None

    def read_aligned_string(self, length):
        """Read a byte-aligned string of given length"""
        self.byte_align()
        if self.pos + length > len(self.data):
            return ""
        s = self.data[self.pos:self.pos + length].decode('utf-8', errors='replace')
        self.pos += length
        return s

def parse_initdata(replay_path):
    """Parse replay.initData to extract player count and game info"""
    archive = mpyq.MPQArchive(replay_path)
    init_data = archive.read_file("replay.initData")

    decoder = BitPackedDecoder(init_data)

    # Read user_initial_data array
    # The first value is the count of slots/players (stored in 5 bits)
    user_count = decoder.read_bits(5)

    print(f"User slots from initData: {user_count}")

    # Skip through user_initial_data entries to get to game_description
    # This is complex as each entry has variable-length data
    # For simplicity, let's parse replay.details instead which has simpler format

    # Try replay.details which has player information
    details = archive.read_file("replay.details")
    print(f"\nreplay.details size: {len(details)} bytes")

    # replay.details is also bitpacked but has a simpler structure
    # It contains player names and info
    # Let's count occurrences of player data structures

    return user_count

if __name__ == "__main__":
    if len(sys.argv) != 2:
        print("Usage: parse_initdata.py <replay_file>")
        sys.exit(1)

    replay_path = sys.argv[1]
    player_count = parse_initdata(replay_path)
    print(f"Result: {player_count} players")
