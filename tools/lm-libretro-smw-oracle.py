#!/usr/bin/env python3
"""Exercise LMEMU001 selected-level loading against a real SMW ROM and libretro core."""

import argparse
import hashlib
import struct
import subprocess

MAGIC = b"LMEMU001"


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
        data = self.process.stdout.read(length)
        if len(data) != length:
            raise RuntimeError("truncated backend event")
        return data

    def close(self):
        if self.process.poll() is None:
            assert self.exchange(b"\x08") == b"\x82\x00"
            self.process.stdin.close()
        if self.process.wait(timeout=5) != 0:
            raise RuntimeError("backend exited unsuccessfully")


def runtime_frame(event):
    if not event or event[0] != 0x85:
        raise RuntimeError(f"expected runtime frame, got tag {event[:1].hex()}")
    width, height, byte_count = struct.unpack("<III", event[1:13])
    rgba = event[13 : 13 + byte_count]
    if len(rgba) != width * height * 4:
        raise RuntimeError("runtime frame geometry does not match RGBA payload")
    state = struct.unpack("<BHBHH", event[13 + byte_count : 21 + byte_count])
    return width, height, rgba, state


def await_level(backend, level, limit):
    saw_overworld = False
    transitions = []
    previous = None
    for frame in range(limit):
        width, height, rgba, state = runtime_frame(backend.exchange(b"\x05"))
        mode, sublevel, translevel, camera_x, camera_y = state
        if mode != previous:
            transitions.append((frame, mode, sublevel))
            previous = mode
        saw_overworld |= mode == 0x0E
        if mode == 0x14 and sublevel == level and (saw_overworld or frame < 300):
            if width != 256 or height != 224 or len(set(rgba)) < 8:
                raise RuntimeError("selected level did not publish a bounded nonuniform frame")
            if any(rgba[index] != 0xFF for index in range(3, len(rgba), 4)):
                raise RuntimeError("selected-level frame has invalid alpha")
            return {
                "frame": frame,
                "mode": mode,
                "sublevel": sublevel,
                "translevel": translevel,
                "camera_x": camera_x,
                "camera_y": camera_y,
                "width": width,
                "height": height,
                "sha256": hashlib.sha256(rgba).hexdigest(),
                "transitions": transitions,
            }
    raise RuntimeError(f"level {level:03X} was not entered: {transitions!r}")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--backend", required=True)
    parser.add_argument("--core", required=True)
    parser.add_argument("--rom", required=True)
    parser.add_argument("--initial-level", type=lambda value: int(value, 16), default=0x105)
    parser.add_argument("--switch-level", type=lambda value: int(value, 16), default=0x106)
    args = parser.parse_args()
    rom = open(args.rom, "rb").read()
    backend = Backend(args.backend, args.core)
    try:
        ready = backend.exchange()
        if ready[:1] != b"\x80" or struct.unpack("<I", ready[1:])[0] & 0x7F != 0x7F:
            raise RuntimeError(f"backend lacks required capabilities: {ready.hex()}")
        initialize = (
            b"\x00"
            + struct.pack("<QHB", 1, args.initial_level, 0)
            + struct.pack("<I", len(rom))
            + rom
            + struct.pack("<I", 0)
        )
        if backend.exchange(initialize) != b"\x82\x01":
            raise RuntimeError("backend rejected initialization")
        initial = await_level(backend, args.initial_level, 5000)
        if backend.exchange(b"\x02" + struct.pack("<H", args.switch_level)) != b"\x81":
            raise RuntimeError("backend rejected live level switch")
        switched = await_level(backend, args.switch_level, 300)
        if backend.exchange(b"\x04\x02") != b"\x81":
            raise RuntimeError("backend rejected hard pause")
        runtime_frame(backend.exchange(b"\x05"))
        print("result\tlevel\tframe\tmode\ttranslevel\tcamera\tsize\tframe_sha256")
        for label, level, result in (
            ("initial", args.initial_level, initial),
            ("switch", args.switch_level, switched),
        ):
            print(
                f"{label}\t{level:03X}\t{result['frame']}\t{result['mode']:02X}\t"
                f"{result['translevel']:02X}\t{result['camera_x']},{result['camera_y']}\t"
                f"{result['width']}x{result['height']}\t{result['sha256']}"
            )
        print(f"rom_sha256\t{hashlib.sha256(rom).hexdigest()}")
    finally:
        backend.close()


if __name__ == "__main__":
    main()
