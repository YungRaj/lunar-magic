#define _POSIX_C_SOURCE 200809L
/* Deterministic libretro Snes9x adapter for the opt-in SMW gameplay gates. */
#include <dlfcn.h>
#include <errno.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>

enum {
  RETRO_DEVICE_JOYPAD = 1,
  RETRO_B = 0,
  RETRO_START = 3,
  RETRO_A = 8,
  RETRO_MEMORY_SYSTEM_RAM = 2,
  RETRO_ENV_SET_PIXEL_FORMAT = 10,
  RETRO_ENV_SET_INPUT_DESCRIPTORS = 11,
  RETRO_ENV_SET_VARIABLES = 16,
  RETRO_ENV_SET_SUPPORT_NO_GAME = 18,
  RETRO_ENV_GET_AUDIO_VIDEO_ENABLE = 47,
  RETRO_ENV_SET_MEMORY_MAPS = 36,
  RETRO_ENV_SET_GEOMETRY = 37,
};

struct retro_game_info {
  const char *path;
  const void *data;
  size_t size;
  const char *meta;
};

struct retro_system_info {
  const char *library_name;
  const char *library_version;
  const char *valid_extensions;
  bool need_fullpath;
  bool block_extract;
};

typedef void (*retro_set_environment_fn)(bool (*)(unsigned, void *));
typedef void (*retro_set_video_refresh_fn)(void (*)(const void *, unsigned, unsigned, size_t));
typedef void (*retro_set_audio_sample_fn)(void (*)(int16_t, int16_t));
typedef void (*retro_set_audio_sample_batch_fn)(size_t (*)(const int16_t *, size_t));
typedef void (*retro_set_input_poll_fn)(void (*)(void));
typedef void (*retro_set_input_state_fn)(int16_t (*)(unsigned, unsigned, unsigned, unsigned));
typedef void (*retro_init_fn)(void);
typedef unsigned (*retro_api_version_fn)(void);
typedef void (*retro_get_system_info_fn)(struct retro_system_info *);
typedef bool (*retro_load_game_fn)(const struct retro_game_info *);
typedef void (*retro_run_fn)(void);
typedef void *(*retro_get_memory_data_fn)(unsigned);
typedef size_t (*retro_get_memory_size_fn)(unsigned);
typedef size_t (*retro_serialize_size_fn)(void);
typedef bool (*retro_serialize_fn)(void *, size_t);
typedef bool (*retro_unserialize_fn)(const void *, size_t);
typedef void (*retro_unload_game_fn)(void);
typedef void (*retro_deinit_fn)(void);

struct core_api {
  void *library;
  retro_set_environment_fn set_environment;
  retro_set_video_refresh_fn set_video_refresh;
  retro_set_audio_sample_fn set_audio_sample;
  retro_set_audio_sample_batch_fn set_audio_sample_batch;
  retro_set_input_poll_fn set_input_poll;
  retro_set_input_state_fn set_input_state;
  retro_init_fn init;
  retro_api_version_fn api_version;
  retro_get_system_info_fn get_system_info;
  retro_load_game_fn load_game;
  retro_run_fn run;
  retro_get_memory_data_fn get_memory_data;
  retro_get_memory_size_fn get_memory_size;
  retro_serialize_size_fn serialize_size;
  retro_serialize_fn serialize;
  retro_unserialize_fn unserialize;
  retro_unload_game_fn unload_game;
  retro_deinit_fn deinit;
};

struct endpoint { uint16_t x, y; uint8_t submap; };
struct options {
  const char *core, *rom, *scenario, *snapshot, *screenshot;
  struct endpoint source, expected;
  uint16_t expected_timer;
};
struct snapshot { uint8_t *bytes; size_t size; };

static uint16_t held_buttons;
static unsigned pixel_format;
static uint8_t *video_pixels;
static size_t video_size, video_pitch;
static unsigned video_width, video_height;

static bool environment(unsigned command, void *data) {
  switch (command) {
    case RETRO_ENV_SET_PIXEL_FORMAT:
      pixel_format = *(const unsigned *)data;
      return true;
    case RETRO_ENV_GET_AUDIO_VIDEO_ENABLE:
      *(int *)data = 3;
      return true;
    case RETRO_ENV_SET_INPUT_DESCRIPTORS:
    case RETRO_ENV_SET_VARIABLES:
    case RETRO_ENV_SET_SUPPORT_NO_GAME:
    case RETRO_ENV_SET_MEMORY_MAPS:
    case RETRO_ENV_SET_GEOMETRY:
      return true;
    default:
      return false;
  }
}

static void video_refresh(const void *data, unsigned width, unsigned height, size_t pitch) {
  if (!data || width == 0 || width > 512 || height == 0 || height > 478 || pitch == 0 ||
      pitch > 4096 || height > SIZE_MAX / pitch || (size_t)height * pitch > 16u * 1024u * 1024u)
    return;
  size_t size = (size_t)height * pitch;
  uint8_t *replacement = realloc(video_pixels, size);
  if (!replacement)
    return;
  video_pixels = replacement;
  memcpy(video_pixels, data, size);
  video_size = size;
  video_width = width;
  video_height = height;
  video_pitch = pitch;
}

static void audio_sample(int16_t left, int16_t right) { (void)left; (void)right; }
static size_t audio_batch(const int16_t *data, size_t frames) { (void)data; return frames; }
static void input_poll(void) {}
static int16_t input_state(unsigned port, unsigned device, unsigned index, unsigned id) {
  (void)index;
  return port == 0 && device == RETRO_DEVICE_JOYPAD && id < 16 &&
         (held_buttons & (uint16_t)(1u << id)) ? 1 : 0;
}

static bool load_symbol(void *library, const char *name, void *destination) {
  void *symbol = dlsym(library, name);
  if (!symbol) {
    fprintf(stderr, "Snes9x core is missing %s: %s\n", name, dlerror());
    return false;
  }
  memcpy(destination, &symbol, sizeof(symbol));
  return true;
}

#define LOAD(api, member, name) \
  if (!load_symbol((api)->library, (name), &(api)->member)) return false

static bool load_core(struct core_api *api, const char *path) {
  memset(api, 0, sizeof(*api));
  api->library = dlopen(path, RTLD_NOW | RTLD_LOCAL);
  if (!api->library) {
    fprintf(stderr, "cannot load Snes9x libretro core %s: %s\n", path, dlerror());
    return false;
  }
  LOAD(api, set_environment, "retro_set_environment");
  LOAD(api, set_video_refresh, "retro_set_video_refresh");
  LOAD(api, set_audio_sample, "retro_set_audio_sample");
  LOAD(api, set_audio_sample_batch, "retro_set_audio_sample_batch");
  LOAD(api, set_input_poll, "retro_set_input_poll");
  LOAD(api, set_input_state, "retro_set_input_state");
  LOAD(api, init, "retro_init");
  LOAD(api, api_version, "retro_api_version");
  LOAD(api, get_system_info, "retro_get_system_info");
  LOAD(api, load_game, "retro_load_game");
  LOAD(api, run, "retro_run");
  LOAD(api, get_memory_data, "retro_get_memory_data");
  LOAD(api, get_memory_size, "retro_get_memory_size");
  LOAD(api, serialize_size, "retro_serialize_size");
  LOAD(api, serialize, "retro_serialize");
  LOAD(api, unserialize, "retro_unserialize");
  LOAD(api, unload_game, "retro_unload_game");
  LOAD(api, deinit, "retro_deinit");
  return true;
}

static bool validate_core(struct core_api *api) {
  struct retro_system_info info = {0};
  api->get_system_info(&info);
  if (api->api_version() != 1 || !info.library_name || !strstr(info.library_name, "Snes9x")) {
    fprintf(stderr, "gameplay driver requires an official Snes9x libretro core\n");
    return false;
  }
  return true;
}

static bool parse_hex(const char *text, unsigned limit, unsigned *value) {
  char *end = NULL;
  errno = 0;
  unsigned long parsed = strtoul(text, &end, 16);
  if (errno || !text[0] || !end || *end || parsed > limit)
    return false;
  *value = (unsigned)parsed;
  return true;
}

static bool parse_options(int argc, char **argv, struct options *options) {
  memset(options, 0, sizeof(*options));
  unsigned seen = 0;
  for (int i = 1; i + 1 < argc; i += 2) {
    const char *key = argv[i], *value = argv[i + 1];
    unsigned parsed;
    if (!strcmp(key, "--emulator")) { options->core = value; seen |= 1u << 0; }
    else if (!strcmp(key, "--rom")) { options->rom = value; seen |= 1u << 1; }
    else if (!strcmp(key, "--scenario")) { options->scenario = value; seen |= 1u << 2; }
    else if (!strcmp(key, "--snapshot")) { options->snapshot = value; seen |= 1u << 3; }
    else if (!strcmp(key, "--screenshot")) { options->screenshot = value; seen |= 1u << 4; }
    else if (!strcmp(key, "--source-x") && parse_hex(value, 0xffff, &parsed)) { options->source.x = (uint16_t)parsed; seen |= 1u << 5; }
    else if (!strcmp(key, "--source-y") && parse_hex(value, 0xffff, &parsed)) { options->source.y = (uint16_t)parsed; seen |= 1u << 6; }
    else if (!strcmp(key, "--source-submap") && parse_hex(value, 0xff, &parsed)) { options->source.submap = (uint8_t)parsed; seen |= 1u << 7; }
    else if (!strcmp(key, "--expected-x") && parse_hex(value, 0xffff, &parsed)) { options->expected.x = (uint16_t)parsed; seen |= 1u << 8; }
    else if (!strcmp(key, "--expected-y") && parse_hex(value, 0xffff, &parsed)) { options->expected.y = (uint16_t)parsed; seen |= 1u << 9; }
    else if (!strcmp(key, "--expected-submap") && parse_hex(value, 0xff, &parsed)) { options->expected.submap = (uint8_t)parsed; seen |= 1u << 10; }
    else if (!strcmp(key, "--expected-timer") && parse_hex(value, 0x999, &parsed)) { options->expected_timer = (uint16_t)parsed; seen |= 1u << 11; }
    else {
      fprintf(stderr, "invalid or unknown gameplay-driver argument: %s %s\n", key, value);
      return false;
    }
  }
  if (!strcmp(options->scenario, "smw-overworld-path-link"))
    return argc == 23 && seen == 0x7ff;
  if (!strcmp(options->scenario, "smw-level-header"))
    return argc == 13 && seen == 0x81f;
  return false;
}

static void put_u16(uint8_t *ram, size_t offset, uint16_t value) {
  ram[offset] = (uint8_t)value;
  ram[offset + 1] = (uint8_t)(value >> 8);
}

static uint16_t get_u16(const uint8_t *ram, size_t offset) {
  return (uint16_t)(ram[offset] | (uint16_t)(ram[offset + 1] << 8));
}

static bool is_destination(const uint8_t *ram, struct endpoint expected) {
  return ram[0x0100] == 0x0e && ram[0x1f11] == expected.submap &&
         get_u16(ram, 0x1f17) == expected.x && get_u16(ram, 0x1f19) == expected.y &&
         get_u16(ram, 0x1f1f) == (expected.x >> 4) &&
         get_u16(ram, 0x1f21) == (expected.y >> 4);
}

static bool enter_overworld(struct core_api *api, uint8_t *ram) {
  unsigned age = 0, previous_mode = 0xff;
  for (unsigned frame = 0; frame < 3600; frame++) {
    unsigned mode = ram[0x0100];
    if (mode != previous_mode) { previous_mode = mode; age = 0; } else { age++; }
    held_buttons = 0;
    if (mode == 0x06 && age == 60) held_buttons = (uint16_t)(1u << RETRO_START);
    else if ((mode == 0x08 || mode == 0x0a) && age == 60) held_buttons = (uint16_t)(1u << RETRO_A);
    else if (mode == 0x14 && ram[0x1426] && age > 120 && age % 120 == 0)
      held_buttons = (uint16_t)(1u << RETRO_B);
    api->run();
    if (ram[0x0100] == 0x0e)
      return true;
  }
  fprintf(stderr, "SMW did not reach overworld mode within 3600 frames\n");
  return false;
}

static bool adjacent(struct endpoint source, unsigned direction, uint16_t *x, uint16_t *y) {
  *x = source.x; *y = source.y;
  if (direction == 0) { if (source.y > 0xffef) return false; *y += 0x10; }
  else if (direction == 2) { if (source.y < 0x10) return false; *y -= 0x10; }
  else if (direction == 4) { if (source.x > 0xffef) return false; *x += 0x10; }
  else if (direction == 6) { if (source.x < 0x10) return false; *x -= 0x10; }
  else return false;
  return true;
}

static void stage_approach(uint8_t *ram, struct endpoint source, unsigned direction) {
  uint16_t x, y;
  (void)adjacent(source, direction, &x, &y);
  ram[0x1f11] = source.submap;
  put_u16(ram, 0x13c3, source.submap);
  put_u16(ram, 0x1f17, x); put_u16(ram, 0x1f19, y);
  put_u16(ram, 0x1f1f, x >> 4); put_u16(ram, 0x1f21, y >> 4);
  put_u16(ram, 0x0dc7, x); put_u16(ram, 0x0dc9, y);
  memset(ram + 0x0dcf, 0, 4);
  put_u16(ram, 0x0dd3, (uint16_t)direction);
  put_u16(ram, 0x0dd6, 0);
  ram[0x13d9] = 4;
}

static bool capture_snapshot(struct core_api *api, struct snapshot *snapshot) {
  snapshot->size = api->serialize_size();
  if (snapshot->size == 0 || snapshot->size > 64u * 1024u * 1024u) return false;
  snapshot->bytes = malloc(snapshot->size);
  return snapshot->bytes && api->serialize(snapshot->bytes, snapshot->size);
}

static bool traverse(struct core_api *api, uint8_t *ram, const struct options *options,
                     struct snapshot *snapshot) {
  size_t baseline_size = api->serialize_size();
  if (baseline_size == 0 || baseline_size > 64u * 1024u * 1024u)
    return false;
  uint8_t *baseline = malloc(baseline_size);
  if (!baseline || !api->serialize(baseline, baseline_size)) { free(baseline); return false; }
  const unsigned directions[] = {0, 2, 4, 6};
  for (size_t attempt = 0; attempt < sizeof(directions) / sizeof(directions[0]); attempt++) {
    unsigned direction = directions[attempt];
    if (!adjacent(options->source, direction, &(uint16_t){0}, &(uint16_t){0}) ||
        !api->unserialize(baseline, baseline_size))
      continue;
    stage_approach(ram, options->source, direction);
    for (unsigned frame = 0; frame < 900; frame++) {
      held_buttons = 0;
      api->run();
      if (is_destination(ram, options->expected)) {
        if (!capture_snapshot(api, snapshot)) { free(baseline); return false; }
        /* Retain exact-arrival WRAM above, then let the destination map finish rendering. */
        for (unsigned settle = 0; settle < 900; settle++) {
          held_buttons = 0;
          api->run();
          if (ram[0x0100] == 0x0e && ram[0x1f11] == options->expected.submap &&
              ram[0x13d9] == 3) {
            free(baseline);
            return true;
          }
        }
        free(snapshot->bytes); snapshot->bytes = NULL; snapshot->size = 0;
        free(baseline);
        fprintf(stderr, "edited destination did not finish rendering within 900 frames\n");
        return false;
      }
    }
  }
  free(baseline);
  fprintf(stderr, "Snes9x did not traverse the edited source route to the expected destination\n");
  return false;
}

static bool timer_matches(const uint8_t *ram, uint16_t timer) {
  return ram[0x0f31] == (uint8_t)(timer >> 8) &&
         ram[0x0f32] == (uint8_t)((timer >> 4) & 0x0f) &&
         ram[0x0f33] == (uint8_t)(timer & 0x0f);
}

static bool enter_current_level(struct core_api *api, uint8_t *ram,
                                const struct options *options, struct snapshot *snapshot) {
  unsigned overworld_age = 0;
  for (unsigned frame = 0; frame < 3600; frame++) {
    held_buttons = 0;
    if (ram[0x0100] == 0x0e) {
      overworld_age++;
      if (overworld_age >= 120 && overworld_age % 30 == 0)
        held_buttons = (uint16_t)(1u << RETRO_A);
    }
    api->run();
    if (ram[0x0100] == 0x14 && timer_matches(ram, options->expected_timer)) {
      if (!capture_snapshot(api, snapshot))
        return false;
      for (unsigned settle = 0; settle < 120; settle++) {
        held_buttons = 0;
        api->run();
      }
      return true;
    }
  }
  fprintf(stderr,
          "SMW did not enter the current level with timer %03X within 3600 frames; "
          "mode=%02X level=%02X%02X timer=%02X%02X%02X\n",
          options->expected_timer, ram[0x0100], ram[0x010c], ram[0x010b],
          ram[0x0f31], ram[0x0f32], ram[0x0f33]);
  return false;
}

static uint32_t crc32_bytes(const uint8_t *data, size_t length) {
  uint32_t crc = 0xffffffffu;
  for (size_t i = 0; i < length; i++) {
    crc ^= data[i];
    for (unsigned bit = 0; bit < 8; bit++) crc = (crc >> 1) ^ (0xedb88320u & (0u - (crc & 1u)));
  }
  return ~crc;
}

static uint32_t adler32_bytes(const uint8_t *data, size_t length) {
  uint32_t a = 1, b = 0;
  for (size_t i = 0; i < length; i++) { a = (a + data[i]) % 65521u; b = (b + a) % 65521u; }
  return (b << 16) | a;
}

static bool write_all(FILE *file, const void *data, size_t length) {
  return fwrite(data, 1, length, file) == length;
}

static void be32(uint8_t output[4], uint32_t value) {
  output[0] = (uint8_t)(value >> 24); output[1] = (uint8_t)(value >> 16);
  output[2] = (uint8_t)(value >> 8); output[3] = (uint8_t)value;
}

static bool png_chunk(FILE *file, const char type[4], const uint8_t *data, size_t length) {
  if (length > UINT32_MAX) return false;
  uint8_t word[4]; be32(word, (uint32_t)length);
  if (!write_all(file, word, 4) || !write_all(file, type, 4) || !write_all(file, data, length)) return false;
  uint8_t *crc_data = malloc(length + 4);
  if (!crc_data) return false;
  memcpy(crc_data, type, 4);
  if (length) memcpy(crc_data + 4, data, length);
  be32(word, crc32_bytes(crc_data, length + 4)); free(crc_data);
  return write_all(file, word, 4);
}

static void pixel_rgb(const uint8_t *pixel, uint8_t rgb[3]) {
  if (pixel_format == 1) {
    uint32_t value; memcpy(&value, pixel, sizeof(value));
    rgb[0] = (uint8_t)(value >> 16); rgb[1] = (uint8_t)(value >> 8); rgb[2] = (uint8_t)value;
  } else {
    uint16_t value; memcpy(&value, pixel, sizeof(value));
    if (pixel_format == 2) {
      rgb[0] = (uint8_t)(((value >> 11) & 31) * 255 / 31);
      rgb[1] = (uint8_t)(((value >> 5) & 63) * 255 / 63);
      rgb[2] = (uint8_t)((value & 31) * 255 / 31);
    } else {
      rgb[0] = (uint8_t)(((value >> 10) & 31) * 255 / 31);
      rgb[1] = (uint8_t)(((value >> 5) & 31) * 255 / 31);
      rgb[2] = (uint8_t)((value & 31) * 255 / 31);
    }
  }
}

static bool write_png(const char *path) {
  size_t row = 1 + (size_t)video_width * 3;
  if (!video_pixels || !video_width || !video_height || video_width > 512 || video_height > 478 ||
      row > SIZE_MAX / video_height) return false;
  size_t raw_size = row * video_height;
  uint8_t *raw = malloc(raw_size);
  if (!raw) return false;
  size_t pixel_bytes = pixel_format == 1 ? 4 : 2;
  for (unsigned y = 0; y < video_height; y++) {
    raw[(size_t)y * row] = 0;
    for (unsigned x = 0; x < video_width; x++)
      pixel_rgb(video_pixels + (size_t)y * video_pitch + (size_t)x * pixel_bytes,
                raw + (size_t)y * row + 1 + (size_t)x * 3);
  }
  size_t blocks = (raw_size + 65534) / 65535;
  size_t zsize = 2 + raw_size + blocks * 5 + 4;
  uint8_t *z = malloc(zsize), *cursor = z;
  if (!z) { free(raw); return false; }
  *cursor++ = 0x78; *cursor++ = 0x01;
  size_t offset = 0;
  while (offset < raw_size) {
    uint16_t length = (uint16_t)((raw_size - offset) > 65535 ? 65535 : raw_size - offset);
    *cursor++ = offset + length == raw_size ? 1 : 0;
    *cursor++ = (uint8_t)length; *cursor++ = (uint8_t)(length >> 8);
    uint16_t inverse = (uint16_t)~length;
    *cursor++ = (uint8_t)inverse; *cursor++ = (uint8_t)(inverse >> 8);
    memcpy(cursor, raw + offset, length); cursor += length; offset += length;
  }
  uint8_t adler[4]; be32(adler, adler32_bytes(raw, raw_size)); memcpy(cursor, adler, 4); cursor += 4;
  uint8_t ihdr[13] = {0}; be32(ihdr, video_width); be32(ihdr + 4, video_height); ihdr[8] = 8; ihdr[9] = 2;
  FILE *file = fopen(path, "wbx");
  static const uint8_t signature[8] = {137,80,78,71,13,10,26,10};
  bool ok = file && write_all(file, signature, 8) && png_chunk(file, "IHDR", ihdr, sizeof(ihdr)) &&
            png_chunk(file, "IDAT", z, (size_t)(cursor - z)) && png_chunk(file, "IEND", NULL, 0) &&
            fflush(file) == 0;
  if (file && fclose(file) != 0) ok = false;
  free(z); free(raw);
  return ok;
}

static bool write_snapshot(const struct snapshot *snapshot, const char *path) {
  FILE *file = fopen(path, "wbx");
  bool ok = file && write_all(file, snapshot->bytes, snapshot->size) && fflush(file) == 0;
  if (file && fclose(file) != 0) ok = false;
  return ok;
}

int main(int argc, char **argv) {
  struct options options;
  if (!parse_options(argc, argv, &options)) {
    fprintf(stderr, "usage: driver --emulator CORE --rom ROM --scenario smw-overworld-path-link --source-x HEX --source-y HEX --source-submap HEX --expected-x HEX --expected-y HEX --expected-submap HEX --snapshot FILE --screenshot PNG\n"
                    "   or: driver --emulator CORE --rom ROM --scenario smw-level-header --expected-timer BCD --snapshot FILE --screenshot PNG\n");
    return 2;
  }
  struct stat metadata;
  if (lstat(options.snapshot, &metadata) == 0 || errno != ENOENT ||
      lstat(options.screenshot, &metadata) == 0 || errno != ENOENT) {
    fprintf(stderr, "snapshot and screenshot outputs must not already exist\n");
    return 2;
  }
  struct core_api api;
  if (!load_core(&api, options.core) || !validate_core(&api)) return 2;
  api.set_environment(environment); api.set_video_refresh(video_refresh);
  api.set_audio_sample(audio_sample); api.set_audio_sample_batch(audio_batch);
  api.set_input_poll(input_poll); api.set_input_state(input_state); api.init();
  struct retro_game_info game = {options.rom, NULL, 0, NULL};
  bool loaded = api.load_game(&game);
  uint8_t *ram = loaded ? api.get_memory_data(RETRO_MEMORY_SYSTEM_RAM) : NULL;
  struct snapshot snapshot = {0};
  bool scenario_ok = false;
  if (loaded && ram && api.get_memory_size(RETRO_MEMORY_SYSTEM_RAM) == 128u * 1024u &&
      enter_overworld(&api, ram)) {
    if (!strcmp(options.scenario, "smw-overworld-path-link"))
      scenario_ok = traverse(&api, ram, &options, &snapshot);
    else if (!strcmp(options.scenario, "smw-level-header"))
      scenario_ok = enter_current_level(&api, ram, &options, &snapshot);
  }
  bool ok = loaded && ram && api.get_memory_size(RETRO_MEMORY_SYSTEM_RAM) == 128u * 1024u &&
            scenario_ok &&
            write_snapshot(&snapshot, options.snapshot) && write_png(options.screenshot);
  if (loaded) api.unload_game(); api.deinit(); dlclose(api.library); free(video_pixels);
  free(snapshot.bytes);
  if (!ok) { remove(options.snapshot); remove(options.screenshot); return 1; }
  return 0;
}
