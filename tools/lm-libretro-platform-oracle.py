#!/usr/bin/env python3
"""Exercise every live-backend capability without redistributing a commercial ROM."""

import argparse
import hashlib
import struct
import subprocess

MAGIC = b"LMEMU001"
CAPABILITIES = 0x1FF


def framed(payload):
    return MAGIC + struct.pack("<I", len(payload)) + payload


class Backend:
    def __init__(self, executable, core):
        self.process = subprocess.Popen(
            [executable, core], stdin=subprocess.PIPE, stdout=subprocess.PIPE
        )

    def exchange(self, payload=None):
        if payload is not None:
            self.process.stdin.write(framed(payload))
            self.process.stdin.flush()
        header = self.process.stdout.read(12)
        if len(header) != 12 or header[:8] != MAGIC:
            raise RuntimeError(f"invalid backend header: {header!r}")
        length = struct.unpack("<I", header[8:])[0]
        event = self.process.stdout.read(length)
        if len(event) != length:
            raise RuntimeError("truncated backend event")
        if event[:1] == b"\xFF" and len(event) >= 5:
            size = struct.unpack("<I", event[1:5])[0]
            raise RuntimeError(event[5 : 5 + size].decode("utf-8", errors="replace"))
        return event

    def close(self):
        if self.process.poll() is None:
            assert self.exchange(b"\x08") == b"\x82\x00"
            self.process.stdin.close()
        if self.process.wait(timeout=5) != 0:
            raise RuntimeError("backend exited unsuccessfully")


def runtime_frame(event):
    if event[:1] != b"\x86":
        raise RuntimeError(f"expected runtime audio frame, got {event[:1].hex()}")
    width, height, byte_count = struct.unpack("<III", event[1:13])
    rgba = event[13 : 13 + byte_count]
    if (width, height, len(rgba)) != (256, 224, 256 * 224 * 4):
        raise RuntimeError("invalid platform-oracle frame geometry")
    state_offset = 13 + byte_count
    state = struct.unpack("<BHBHH", event[state_offset : state_offset + 8])
    sample_rate, sample_count = struct.unpack(
        "<II", event[state_offset + 8 : state_offset + 16]
    )
    audio = event[state_offset + 16 :]
    if sample_rate != 32040 or sample_count != 8 or len(audio) != 16:
        raise RuntimeError("invalid platform-oracle audio")
    if any(rgba[index] != 0xFF for index in range(3, len(rgba), 4)):
        raise RuntimeError("platform-oracle alpha is not opaque")
    return state, hashlib.sha256(rgba).hexdigest(), hashlib.sha256(audio).hexdigest()


def enter_level(backend, level):
    result = None
    for _ in range(6):
        result = runtime_frame(backend.exchange(b"\x05"))
    if result[0][0] != 0x14 or result[0][1] != level:
        raise RuntimeError(f"level {level:03X} was not entered: {result[0]!r}")
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--backend", required=True)
    parser.add_argument("--core", required=True)
    args = parser.parse_args()
    rom = bytes(range(64))
    backend = Backend(args.backend, args.core)
    try:
        ready = backend.exchange()
        if ready[:1] != b"\x80" or struct.unpack("<I", ready[1:])[0] != CAPABILITIES:
            raise RuntimeError(f"wrong capability event: {ready.hex()}")
        initialize = (
            b"\x00"
            + struct.pack("<QHB", 1, 0x105, 0)
            + struct.pack("<I", len(rom))
            + rom
            + struct.pack("<I", 0)
        )
        if backend.exchange(initialize) != b"\x82\x01":
            raise RuntimeError("initialization was not acknowledged")
        initial = enter_level(backend, 0x105)

        sprites = b"\x00\x00\x0F\xFF"
        hot_reload = (
            b"\x0A"
            + struct.pack("<QH", 2, 0x105)
            + struct.pack("<I", len(rom))
            + rom
            + struct.pack("<I", len(sprites))
            + sprites
        )
        if backend.exchange(hot_reload) != b"\x81":
            raise RuntimeError("sprite hot reload was not acknowledged")
        runtime_frame(backend.exchange(b"\x05"))
        sprite_state = backend.exchange(b"\x0B")
        if (
            len(sprite_state) != 153
            or sprite_state[:1] != b"\x87"
            or sprite_state[1] == 0
            or sprite_state[13] != 0x0F
            or sprite_state[25] == 0
        ):
            raise RuntimeError("sprite hot reload did not reach live WRAM")

        if backend.exchange(b"\x02" + struct.pack("<H", 0x106)) != b"\x81":
            raise RuntimeError("level switch was not acknowledged")
        switched = enter_level(backend, 0x106)
        if switched[1] == initial[1]:
            raise RuntimeError("switched level did not change the video frame")

        reload_rom = b"\x01" + struct.pack("<Q", 3) + struct.pack("<I", len(rom)) + rom
        if backend.exchange(reload_rom) != b"\x82\x01":
            raise RuntimeError("ROM reload was not acknowledged")
        if backend.exchange(b"\x02" + struct.pack("<H", 0x105)) != b"\x81":
            raise RuntimeError("reloaded level was not acknowledged")
        reloaded = enter_level(backend, 0x105)
        if reloaded[1:] != initial[1:]:
            raise RuntimeError("identical ROM reload was not deterministic")

        if backend.exchange(b"\x04\x02") != b"\x81":
            raise RuntimeError("hard pause was not acknowledged")
        runtime_frame(backend.exchange(b"\x05"))
        print("platform_runtime\tpass")
        print(f"initial_frame_sha256\t{initial[1]}")
        print(f"initial_audio_sha256\t{initial[2]}")
        print(f"switch_frame_sha256\t{switched[1]}")
    finally:
        backend.close()


if __name__ == "__main__":
    main()
