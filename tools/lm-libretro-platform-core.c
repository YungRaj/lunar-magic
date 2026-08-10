/* Deterministic libretro-v1 core for cross-platform lm-libretro process verification. */
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>

#if defined(_WIN32)
#define RETRO_EXPORT __declspec(dllexport)
#else
#define RETRO_EXPORT __attribute__((visibility("default")))
#endif

typedef bool (*environment_fn)(unsigned, void *);
typedef void (*video_fn)(const void *, unsigned, unsigned, size_t);
typedef void (*audio_fn)(int16_t, int16_t);
typedef size_t (*audio_batch_fn)(const int16_t *, size_t);
typedef void (*input_poll_fn)(void);
typedef int16_t (*input_state_fn)(unsigned, unsigned, unsigned, unsigned);

struct game_info {
    const char *path;
    const void *data;
    size_t size;
    const char *meta;
};

struct system_info {
    const char *library_name;
    const char *library_version;
    const char *valid_extensions;
    bool need_fullpath;
    bool block_extract;
};

struct game_geometry {
    unsigned base_width;
    unsigned base_height;
    unsigned max_width;
    unsigned max_height;
    float aspect_ratio;
};

struct system_timing {
    double fps;
    double sample_rate;
};

struct system_av_info {
    struct game_geometry geometry;
    struct system_timing timing;
};

static environment_fn environment_callback;
static video_fn video_callback;
static audio_fn audio_callback;
static audio_batch_fn audio_batch_callback;
static input_poll_fn input_poll_callback;
static input_state_fn input_state_callback;
static uint8_t wram[128 * 1024];
static uint8_t sram[8 * 1024];
static uint32_t pixels[256 * 224];
static uint32_t frame_number;

RETRO_EXPORT void retro_set_environment(environment_fn callback) {
    environment_callback = callback;
}

RETRO_EXPORT void retro_set_video_refresh(video_fn callback) {
    video_callback = callback;
}

RETRO_EXPORT void retro_set_audio_sample(audio_fn callback) {
    audio_callback = callback;
}

RETRO_EXPORT void retro_set_audio_sample_batch(audio_batch_fn callback) {
    audio_batch_callback = callback;
}

RETRO_EXPORT void retro_set_input_poll(input_poll_fn callback) {
    input_poll_callback = callback;
}

RETRO_EXPORT void retro_set_input_state(input_state_fn callback) {
    input_state_callback = callback;
}

RETRO_EXPORT void retro_init(void) {
    unsigned format = 1; /* RETRO_PIXEL_FORMAT_XRGB8888 */
    if (environment_callback != NULL) {
        (void)environment_callback(10, &format);
    }
}

RETRO_EXPORT void retro_deinit(void) {}

RETRO_EXPORT unsigned retro_api_version(void) {
    return 1;
}

RETRO_EXPORT void retro_get_system_info(struct system_info *info) {
    *info = (struct system_info){
        .library_name = "LM platform oracle",
        .library_version = "1",
        .valid_extensions = "smc|sfc",
        .need_fullpath = false,
        .block_extract = false,
    };
}

RETRO_EXPORT void retro_get_system_av_info(struct system_av_info *info) {
    *info = (struct system_av_info){
        .geometry = {256, 224, 256, 224, 8.0f / 7.0f},
        .timing = {60.0, 32040.0},
    };
}

RETRO_EXPORT bool retro_load_game(const struct game_info *game) {
    if (game == NULL || game->data == NULL || game->size == 0) {
        return false;
    }
    memset(wram, 0, sizeof(wram));
    memset(sram, 0xa5, sizeof(sram));
    wram[0x0100] = 0x0e;
    wram[0x13bf] = 0x1c;
    wram[0x001c] = 0xc0;
    frame_number = 0;
    return true;
}

RETRO_EXPORT void retro_unload_game(void) {}
RETRO_EXPORT void retro_reset(void) {}

static void advance_level_transition(void) {
    switch (wram[0x0100]) {
    case 0x0f:
    case 0x10:
    case 0x11:
    case 0x12:
    case 0x13:
        ++wram[0x0100];
        break;
    default:
        break;
    }
}

static void consume_sprite_overlay(void) {
    if (wram[0x0100] != 0x14 || wram[0x00ce] != 0 || wram[0x00cf] != 0 ||
        wram[0x00d0] != 0x70 || sram[1] == 0xff) {
        return;
    }
    wram[0x14c8] = 8;
    wram[0x009e] = sram[3];
    wram[0x1938] = 1;
}

RETRO_EXPORT void retro_run(void) {
    uint16_t level;
    unsigned x;
    unsigned y;
    int16_t audio[8];

    if (input_poll_callback != NULL) {
        input_poll_callback();
    }
    if (input_state_callback != NULL) {
        (void)input_state_callback(0, 1, 0, 0);
    }
    advance_level_transition();
    consume_sprite_overlay();
    level = (uint16_t)wram[0x010b] | ((uint16_t)wram[0x010c] << 8);
    for (y = 0; y < 224; ++y) {
        for (x = 0; x < 256; ++x) {
            uint8_t red = (uint8_t)(x + level);
            uint8_t green = (uint8_t)(y + frame_number);
            uint8_t blue = (uint8_t)(x ^ y ^ level);
            pixels[y * 256 + x] = ((uint32_t)red << 16) | ((uint32_t)green << 8) | blue;
        }
    }
    if (video_callback != NULL) {
        video_callback(pixels, 256, 224, 256 * sizeof(uint32_t));
    }
    for (x = 0; x < 4; ++x) {
        audio[x * 2] = (int16_t)(level + frame_number + x);
        audio[x * 2 + 1] = (int16_t)(-audio[x * 2]);
    }
    if (audio_batch_callback != NULL) {
        (void)audio_batch_callback(audio, 4);
    } else if (audio_callback != NULL) {
        for (x = 0; x < 4; ++x) {
            audio_callback(audio[x * 2], audio[x * 2 + 1]);
        }
    }
    ++frame_number;
}

RETRO_EXPORT void *retro_get_memory_data(unsigned id) {
    if (id == 2) {
        return wram;
    }
    if (id == 0) {
        return sram;
    }
    return NULL;
}

RETRO_EXPORT size_t retro_get_memory_size(unsigned id) {
    if (id == 2) {
        return sizeof(wram);
    }
    if (id == 0) {
        return sizeof(sram);
    }
    return 0;
}
