#!/usr/bin/env python3
"""Exercise LMEMU001 selected-level loading against a real SMW ROM and libretro core."""

import argparse
import hashlib
import struct
import subprocess

MAGIC = b"LMEMU001"


def framed(payload):
    return MAGIC + struct.pack("<I", len(payload)) + payload


def describe_event(event):
    if event[:1] == b"\xFF" and len(event) >= 5:
        length = struct.unpack("<I", event[1:5])[0]
        message = event[5 : 5 + length]
        if len(message) == length:
            return f"backend error: {message.decode('utf-8', errors='replace')}"
    return f"event {event.hex()}"


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
    if not event or event[0] not in (0x85, 0x86):
        raise RuntimeError(f"expected runtime frame, got tag {event[:1].hex()}")
    width, height, byte_count = struct.unpack("<III", event[1:13])
    rgba = event[13 : 13 + byte_count]
    if len(rgba) != width * height * 4:
        raise RuntimeError("runtime frame geometry does not match RGBA payload")
    state = struct.unpack("<BHBHH", event[13 + byte_count : 21 + byte_count])
    if event[0] == 0x85:
        return width, height, rgba, state, None, b""
    audio_offset = 21 + byte_count
    sample_rate, sample_count = struct.unpack("<II", event[audio_offset : audio_offset + 8])
    audio = event[audio_offset + 8 :]
    if len(audio) != sample_count * 2 or sample_count % 2:
        raise RuntimeError("runtime audio payload is not bounded interleaved stereo")
    if not 8_000 <= sample_rate <= 384_000:
        raise RuntimeError("runtime audio sample rate is invalid")
    return width, height, rgba, state, sample_rate, audio


def await_level(backend, level, limit):
    saw_overworld = False
    saw_requested_transition = False
    transitions = []
    previous = None
    for frame in range(limit):
        width, height, rgba, state, sample_rate, audio = runtime_frame(backend.exchange(b"\x05"))
        mode, sublevel, translevel, camera_x, camera_y = state
        if mode != previous:
            transitions.append((frame, mode, sublevel))
            previous = mode
        saw_overworld |= mode == 0x0E
        saw_requested_transition |= mode == 0x0F and sublevel == level
        if mode == 0x14 and sublevel == level and (
            saw_overworld or saw_requested_transition or frame < 300
        ):
            for _ in range(10):
                width, height, rgba, state, sample_rate, audio = runtime_frame(
                    backend.exchange(b"\x05")
                )
            mode, sublevel, translevel, camera_x, camera_y = state
            if mode != 0x14 or sublevel != level:
                continue
            if width != 256 or height != 224 or len(set(rgba)) < 8:
                raise RuntimeError("selected level did not publish a bounded nonuniform frame")
            if any(rgba[index] != 0xFF for index in range(3, len(rgba), 4)):
                raise RuntimeError("selected-level frame has invalid alpha")
            if sample_rate is None or not audio or len(set(audio)) < 4:
                raise RuntimeError("selected level did not publish nonuniform stereo audio")
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
                "sample_rate": sample_rate,
                "audio_frames": len(audio) // 4,
                "audio_sha256": hashlib.sha256(audio).hexdigest(),
                "transitions": transitions,
            }
    raise RuntimeError(f"level {level:03X} was not entered: {transitions!r}")


def vanilla_sprite_bounds(rom, level):
    header = 512 if len(rom) % 0x8000 == 512 else 0
    low_table = header + 0x2EC00 + level * 2
    bank_offset = header + 0x2D8F6
    low = struct.unpack("<H", rom[low_table : low_table + 2])[0]
    snes = (rom[bank_offset] << 16) | low
    logical = ((snes >> 16) & 0x7F) * 0x8000 + (snes & 0x7FFF)
    start = header + logical
    cursor = start + 1
    while rom[cursor] != 0xFF:
        cursor += 3
    return start, cursor + 1


def vanilla_sprite_stream(rom, level):
    start, end = vanilla_sprite_bounds(rom, level)
    return rom[start + 1 : end]


def hot_reload_sprites(backend, revision, rom, level):
    sprite_stream = vanilla_sprite_stream(rom, level)
    command = (
        b"\x0A"
        + struct.pack("<Q", revision)
        + struct.pack("<H", level)
        + struct.pack("<I", len(rom))
        + rom
        + struct.pack("<I", len(sprite_stream))
        + sprite_stream
    )
    if backend.exchange(command) != b"\x81":
        raise RuntimeError("backend rejected state-preserving sprite reload")


def frame_sequence(backend, count, joypad=0):
    if backend.exchange(b"\x09" + struct.pack("<H", joypad)) != b"\x81":
        raise RuntimeError("backend rejected deterministic sprite-oracle input")
    result = []
    for _ in range(count):
        width, height, rgba, state, _, _ = runtime_frame(backend.exchange(b"\x05"))
        result.append((width, height, state, hashlib.sha256(rgba).digest()))
    if backend.exchange(b"\x09\x00\x00") != b"\x81":
        raise RuntimeError("backend rejected sprite-oracle input release")
    return result


def runtime_sprites(backend):
    event = backend.exchange(b"\x0B")
    if len(event) != 1 + 12 + 12 + 128 or event[:1] != b"\x87":
        raise RuntimeError(f"backend rejected runtime-sprite query: {event[:1].hex()}")
    return event[1:13], event[13:25], event[25:]


def runtime_sprite_sequence(backend, count, joypad):
    if backend.exchange(b"\x09" + struct.pack("<H", joypad)) != b"\x81":
        raise RuntimeError("backend rejected runtime-sprite oracle input")
    result = []
    for _ in range(count):
        runtime_frame(backend.exchange(b"\x05"))
        result.append(runtime_sprites(backend))
    if backend.exchange(b"\x09\x00\x00") != b"\x81":
        raise RuntimeError("backend rejected runtime-sprite oracle input release")
    return result


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
        if ready[:1] != b"\x80" or struct.unpack("<I", ready[1:])[0] & 0x1FF != 0x1FF:
            raise RuntimeError(f"backend lacks required capabilities: {ready.hex()}")
        initialize = (
            b"\x00"
            + struct.pack("<QHB", 1, args.initial_level, 0)
            + struct.pack("<I", len(rom))
            + rom
            + struct.pack("<I", 0)
        )
        initialized = backend.exchange(initialize)
        if initialized != b"\x82\x01":
            raise RuntimeError(f"backend rejected initialization: {describe_event(initialized)}")
        initial = await_level(backend, args.initial_level, 5000)
        hot_reload_sprites(backend, 2, rom, args.initial_level)
        _, _, _, hot_state, _, _ = runtime_frame(backend.exchange(b"\x05"))
        if hot_state[0] != 0x14 or hot_state[1] != args.initial_level:
            raise RuntimeError(
                f"sprite reload restarted or left the selected level: {hot_state!r}"
            )
        modified_rom = bytearray(rom)
        sprite_start, sprite_end = vanilla_sprite_bounds(modified_rom, args.initial_level)
        changed_sprites = 0
        for record in range(sprite_start + 1, sprite_end - 1, 3):
            if modified_rom[record + 2] != 0x0F:
                modified_rom[record + 2] = 0x0F
                changed_sprites += 1
        # Put the first edited Goomba on screen 0's current loader boundary. At level entry the
        # camera is stationary, so merely changing off-boundary records would correctly remain
        # dormant until scrolling resumes and would not prove immediate in-place consumption.
        modified_rom[sprite_start + 2] = 0x00
        modified_rom[sprite_start + 3] = 0x0F
        modified_rom[sprite_start + 5] = 0x21
        modified_rom[sprite_start + 6] = 0x0F
        if not changed_sprites:
            raise RuntimeError("oracle sprite mutation did not change the vanilla stream")
        reload_rom = b"\x01" + struct.pack("<Q", 3) + struct.pack("<I", len(rom)) + rom
        if backend.exchange(reload_rom) != b"\x82\x01":
            raise RuntimeError("backend rejected deterministic sprite baseline reload")
        if backend.exchange(b"\x02" + struct.pack("<H", args.initial_level)) != b"\x81":
            raise RuntimeError("backend rejected deterministic sprite baseline level")
        await_level(backend, args.initial_level, 5000)
        hot_reload_sprites(backend, 4, rom, args.initial_level)
        baseline_runtime_sprites = runtime_sprite_sequence(backend, 300, 1 << 7)
        reload_rom = b"\x01" + struct.pack("<Q", 5) + struct.pack("<I", len(rom)) + rom
        if backend.exchange(reload_rom) != b"\x82\x01":
            raise RuntimeError("backend rejected edited-sprite comparison reload")
        if backend.exchange(b"\x02" + struct.pack("<H", args.initial_level)) != b"\x81":
            raise RuntimeError("backend rejected edited-sprite comparison level")
        await_level(backend, args.initial_level, 5000)
        hot_reload_sprites(backend, 6, bytes(modified_rom), args.initial_level)
        modified_runtime_sprites = runtime_sprite_sequence(backend, 300, 1 << 7)
        if baseline_runtime_sprites == modified_runtime_sprites:
            raise RuntimeError(
                "edited sprite record did not alter SMW's native runtime sprite tables"
            )
        if not any(
            any(state and number == 0x0F for state, number in zip(status, numbers))
            for status, numbers, _ in modified_runtime_sprites
        ):
            raise RuntimeError("edited Goomba record was not instantiated by SMW's native loader")
        if not any(any(load_status) for _, _, load_status in modified_runtime_sprites):
            raise RuntimeError("edited stream did not update SMW's record load-status table")
        if backend.exchange(b"\x02" + struct.pack("<H", args.switch_level)) != b"\x81":
            raise RuntimeError("backend rejected live level switch")
        switched = await_level(backend, args.switch_level, 5000)
        if switched["sha256"] == initial["sha256"]:
            raise RuntimeError("selected-level switch reproduced the initial level frame")
        reload_rom = b"\x01" + struct.pack("<Q", 7) + struct.pack("<I", len(rom)) + rom
        if backend.exchange(reload_rom) != b"\x82\x01":
            raise RuntimeError("backend rejected live ROM revision reload")
        if backend.exchange(b"\x02" + struct.pack("<H", args.initial_level)) != b"\x81":
            raise RuntimeError("backend rejected selected level after ROM reload")
        reloaded = await_level(backend, args.initial_level, 5000)
        if reloaded["sha256"] != initial["sha256"]:
            raise RuntimeError("identical ROM reload did not reproduce the selected-level frame")
        if backend.exchange(b"\x04\x02") != b"\x81":
            raise RuntimeError("backend rejected hard pause")
        runtime_frame(backend.exchange(b"\x05"))
        print("result\tlevel\tframe\tmode\ttranslevel\tcamera\tsize\tframe_sha256\taudio")
        for label, level, result in (
            ("initial", args.initial_level, initial),
            ("switch", args.switch_level, switched),
            ("reload", args.initial_level, reloaded),
        ):
            print(
                f"{label}\t{level:03X}\t{result['frame']}\t{result['mode']:02X}\t"
                f"{result['translevel']:02X}\t{result['camera_x']},{result['camera_y']}\t"
                f"{result['width']}x{result['height']}\t{result['sha256']}\t"
                f"{result['sample_rate']}Hz/{result['audio_frames']}f/{result['audio_sha256']}"
            )
        print(f"rom_sha256\t{hashlib.sha256(rom).hexdigest()}")
    finally:
        backend.close()


if __name__ == "__main__":
    main()
