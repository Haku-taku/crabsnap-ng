/* Built instead of the real shim when the build machine has no ffmpeg headers.
 *
 * The Rust side calls the same symbols either way, so the app's code has no notion of which
 * one was compiled: `cs_open` simply answers no, and the app falls back to the ffmpeg
 * program at run time.
 */
#include <stdio.h>

#include "ffmpeg_shim.h"

static const char unavailable[] =
    "this build has no ffmpeg headers, so the libraries cannot be used";

int cs_open(void) { return 0; }

const char *cs_versions(void) { return NULL; }

const char *cs_error(void) { return unavailable; }

void cs_reopen(const char *directory) { (void)directory; }

int cs_muxers(cs_muxer *out, int cap) {
    (void)out;
    (void)cap;
    return 0;
}

int cs_encoders(cs_encoder *out, int cap) {
    (void)out;
    (void)cap;
    return 0;
}

int cs_options(const char *encoder, cs_option *out, int cap) {
    (void)encoder;
    (void)out;
    (void)cap;
    return -1;
}

int cs_has_option(const char *encoder, const char *name) {
    (void)encoder;
    (void)name;
    return -1;
}

int cs_pix_fmts(const char *encoder, int *out, int cap) {
    (void)encoder;
    (void)out;
    (void)cap;
    return -1;
}

int cs_pix_fmt(const char *name) {
    (void)name;
    return -1;
}

const char *cs_pix_fmt_name(int format) {
    (void)format;
    return NULL;
}

const char *cs_codec_name(int codec) {
    (void)codec;
    return NULL;
}

int cs_guess_codec(const char *extension) {
    (void)extension;
    return 0;
}

void cs_srgb(cs_colour *colour) {
    if (!colour) return;
    colour->primaries = 2; /* unspecified */
    colour->transfer = 2;
    colour->space = 2;
    colour->range = 0;
}

int cs_encode(const char *encoder, const char *extension, const cs_picture *picture,
              unsigned char **out, long *size, char *error, int error_cap) {
    (void)encoder;
    (void)extension;
    (void)picture;
    *out = NULL;
    *size = 0;
    snprintf(error, (size_t)error_cap, "%s", unavailable);
    return -1;
}

int cs_decode(const char *path, const unsigned char *data, long size,
              unsigned char **out, long *out_size, cs_decoded *info,
              char *error, int error_cap) {
    (void)path;
    (void)data;
    (void)size;
    (void)info;
    *out = NULL;
    *out_size = 0;
    snprintf(error, (size_t)error_cap, "%s", unavailable);
    return -1;
}

void cs_free(void *buffer) { (void)buffer; }
