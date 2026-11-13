/* The narrow C surface the app talks to libavcodec through.
 *
 * Why C and not hand-written Rust bindings: AVCodecContext and friends are ABI-unstable
 * across ffmpeg majors, and one wrong field offset corrupts memory. Compiled against the
 * headers that are installed, every layout is correct by construction; the shim checks the
 * runtime major at open time and reports "unavailable" when they differ. Rust never sees an
 * ffmpeg type.
 *
 * The shim loads the libraries at runtime — dlopen on Unix, LoadLibraryExW on Windows — so
 * the binary neither links them nor fails to start without them. Windows resolves no
 * dependencies through GetProcAddress, so avcodec and avutil are opened beside avformat.
 */
#ifndef CRABSNAP_FFMPEG_SHIM_H
#define CRABSNAP_FFMPEG_SHIM_H

#define CS_NAME 32
#define CS_TEXT 96
#define CS_DEFAULT 72

/* One muxer (output format). */
typedef struct {
    char name[CS_NAME];
    char long_name[CS_TEXT];
    /* Comma-separated, as libavformat states it. */
    char extensions[CS_TEXT];
    char mime[CS_TEXT];
    int video_codec; /* AVCodecID; 0 = AV_CODEC_ID_NONE */
} cs_muxer;

/* One video encoder. */
typedef struct {
    char name[CS_NAME];
    char long_name[CS_TEXT];
    int id; /* AVCodecID */
} cs_encoder;

/* One encoder option, with its default rendered as text. */
typedef struct {
    char name[CS_NAME];
    char help[CS_TEXT];
    char default_text[CS_DEFAULT];
} cs_option;

/* What colour the picture is in, as the encoder should tag it. */
typedef struct {
    int primaries; /* AVColorPrimaries; 2 = unspecified */
    int transfer;  /* AVColorTransferCharacteristic; 2 = unspecified */
    int space;     /* AVColorSpace; 2 = unspecified */
    int range;     /* AVColorRange; 0 = unspecified */
} cs_colour;

/* The picture to encode plus the settings to encode it with.
 *
 * `option_names`/`option_values` are AVOptions of the encoder ('lossless', 'crf', ...); a name
 * the encoder does not have is skipped, so one recipe can be offered to several encoders. */
typedef struct {
    int width;
    int height;
    /* Top-down 8-bit RGBA, `pitch` bytes per row; alpha is dropped when the format has none. */
    const unsigned char *rgba;
    long pitch;
    /* AV_PIX_FMT_* to encode in; -1 asks the shim for one it can fill without a conversion. */
    int pix_fmt;
    cs_colour colour;
    /* -1 leaves the encoder's default. Encoders that only take an old-style qscale read it. */
    int global_quality;
    const char *const *option_names;
    const char *const *option_values;
    int option_count;
} cs_picture;

/* How a decoded picture is packed. */
#define CS_STORAGE_RGBA 0    /* 8-bit straight-alpha RGBA, four bytes per pixel */
#define CS_STORAGE_PACKED10 1 /* 2-10-10-10 little-endian: R bits 0-9, G 10-19, B 20-29,
                               * alpha bits 30-31 (3 = opaque) */

/* What cs_decode produced: its size, its packing, and what the stream said its pixels mean. */
typedef struct {
    int width;
    int height;
    int storage; /* CS_STORAGE_* */
    cs_colour colour;
} cs_decoded;

/* 1 when the libraries loaded and their versions match these headers. */
int cs_open(void);

/* "63.1.102 63.1.102 61.1.102" (avcodec, avformat, avutil), or NULL. */
const char *cs_versions(void);

/* Why the libraries cannot be used; NULL when they can. */
const char *cs_error(void);

/* Forget the loaded libraries and take `directory` (may be NULL) for the next cs_open.
 *
 * A reload has to see a directory the user has just fixed, or a library installed while the
 * daemon was running, so the loaded state is tied to the probe that made it rather than
 * living for the process. */
void cs_reopen(const char *directory);

/* Every listing below fills at most `cap` entries and returns the total number there are,
 * so a caller sizes its buffer with one call and fills it with the next. */

/* The muxers (output formats). */
int cs_muxers(cs_muxer *out, int cap);

/* The video encoders. */
int cs_encoders(cs_encoder *out, int cap);

/* The encoder's own options, skipping constants; -1 when the encoder is unknown. */
int cs_options(const char *encoder, cs_option *out, int cap);

/* 1 when this encoder (or its context) has an option of that name, 0 when not, -1 when the
 * encoder is unknown. */
int cs_has_option(const char *encoder, const char *name);

/* The pixel formats the encoder accepts, as AV_PIX_FMT_* values; -1 when unknown. */
int cs_pix_fmts(const char *encoder, int *out, int cap);

/* AV_PIX_FMT_* for a name such as "bgra"; -1 when there is no such format. */
int cs_pix_fmt(const char *name);

/* The name of an AV_PIX_FMT_* value; NULL when there is no such format. */
const char *cs_pix_fmt_name(int format);

/* The name of an AVCodecID, such as "mjpeg"; NULL when there is no such codec. */
const char *cs_codec_name(int codec);

/* The codec ffmpeg itself would put in a file called "out.<extension>", as AVCodecID;
 * 0 when no muxer takes that extension. */
int cs_guess_codec(const char *extension);

/* Fill in the tags for 8-bit sRGB: BT.709 primaries and matrix, sRGB transfer, full range. */
void cs_srgb(cs_colour *colour);

/*
 * Encode one picture and mux it for `extension` ("jpg", "webp", "avif").
 * The returned buffer is the whole file; release it with cs_free.
 * 0 on success.
 */
int cs_encode(const char *encoder, const char *extension, const cs_picture *picture,
              unsigned char **out, long *size, char *error, int error_cap);

/*
 * Decode the first picture of a file (or of `data`, when `path` is NULL) into `out`, packed as
 * `info->storage` says and tagged as the stream described it. Only the first frame of an
 * animated file is read: a pin is a still. Release the buffer with cs_free. 0 on success.
 */
int cs_decode(const char *path, const unsigned char *data, long size,
              unsigned char **out, long *out_size, cs_decoded *info,
              char *error, int error_cap);

/* Release a buffer cs_encode produced. */
void cs_free(void *buffer);

#endif
