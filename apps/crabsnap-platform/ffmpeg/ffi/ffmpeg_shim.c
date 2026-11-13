/* See ffmpeg_shim.h for why this is C. Everything ABI-sensitive stays in here. */
#include "ffmpeg_shim.h"

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/avutil.h>
#include <libavutil/error.h>
#include <libavutil/pixdesc.h>
#include <libavutil/opt.h>
#include <libswscale/swscale.h>

#define XSTR_(x) #x
#define XSTR(x) XSTR_(x)

/* One library handle, and how a name becomes one: the loaders differ in the handle type,
 * the registry they search, and whether a name reaches a module's own dependencies. */
#ifdef _WIN32
#include <windows.h>
typedef HMODULE cs_library;

/* GetProcAddress reads one module and Windows has no soname registry, so a name is the
 * file's stem with the major version the headers state. */
#define CS_FORMAT_VERSIONED "avformat-" XSTR(LIBAVFORMAT_VERSION_MAJOR) ".dll"
#define CS_FORMAT_BARE "avformat.dll"
#define CS_CODEC_VERSIONED "avcodec-" XSTR(LIBAVCODEC_VERSION_MAJOR) ".dll"
#define CS_CODEC_BARE "avcodec.dll"
#define CS_UTIL_VERSIONED "avutil-" XSTR(LIBAVUTIL_VERSION_MAJOR) ".dll"
#define CS_UTIL_BARE "avutil.dll"
#define CS_SWS_VERSIONED "swscale-" XSTR(LIBSWSCALE_VERSION_MAJOR) ".dll"
#define CS_SWS_BARE "swscale.dll"

/* The loader's DLL_LOAD_DIR flag takes only a Windows path: a forward slash under it reads
 * as a module name and the load fails with "module not found", while plain flags accept it. */
#define CS_SEPARATOR "\\"

/* A configured path brings its own directory for the libraries it depends on; a bare name
 * is left to the standard search, which is what finds one installed on PATH. */
static cs_library library_open(const char *name, int configured) {
    wchar_t wide[1024];
    if (MultiByteToWideChar(CP_UTF8, 0, name, -1, wide, 1024) == 0) return NULL;
    if (configured) {
        return LoadLibraryExW(wide, NULL,
                              LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR |
                                  LOAD_LIBRARY_SEARCH_DEFAULT_DIRS);
    }
    return LoadLibraryExW(wide, NULL, 0);
}

static void *library_symbol(cs_library library, const char *name) {
    return (void *)(uintptr_t)GetProcAddress(library, name);
}

static void library_close(cs_library library) { FreeLibrary(library); }

/* The system's text for the last failed call, without the newline it ends with, in UTF-8:
 * FormatMessageA answers in the console's code page and the message travels to a UTF-8
 * string. */
static const char *library_reason(void) {
    static char text[1536];
    wchar_t wide[512];
    DWORD code = GetLastError();
    DWORD length =
        FormatMessageW(FORMAT_MESSAGE_FROM_SYSTEM | FORMAT_MESSAGE_IGNORE_INSERTS, NULL, code,
                       0, wide, (DWORD)(sizeof wide / sizeof wide[0]), NULL);
    if (!length ||
        WideCharToMultiByte(CP_UTF8, 0, wide, -1, text, sizeof text, NULL, NULL) == 0) {
        snprintf(text, sizeof text, "error %lu", (unsigned long)code);
        return text;
    }
    length = (DWORD)strlen(text);
    while (length && (text[length - 1] == '\n' || text[length - 1] == '\r')) {
        text[--length] = 0;
    }
    return text;
}

static SRWLOCK state_lock = SRWLOCK_INIT;
static void lock_state(void) { AcquireSRWLockExclusive(&state_lock); }
static void unlock_state(void) { ReleaseSRWLockExclusive(&state_lock); }
#else
#include <dlfcn.h>
#include <pthread.h>
typedef void *cs_library;

#define CS_FORMAT_VERSIONED "libavformat.so." XSTR(LIBAVFORMAT_VERSION_MAJOR)
#define CS_FORMAT_BARE "libavformat.so"
#define CS_CODEC_VERSIONED "libavcodec.so." XSTR(LIBAVCODEC_VERSION_MAJOR)
#define CS_CODEC_BARE "libavcodec.so"
#define CS_UTIL_VERSIONED "libavutil.so." XSTR(LIBAVUTIL_VERSION_MAJOR)
#define CS_UTIL_BARE "libavutil.so"
#define CS_SWS_VERSIONED "libswscale.so." XSTR(LIBSWSCALE_VERSION_MAJOR)
#define CS_SWS_BARE "libswscale.so"
#define CS_SEPARATOR "/"

/* dlopen(NULL) is the main program, not "nothing": never call it with a NULL name. */
static cs_library library_open(const char *name, int configured) {
    (void)configured;
    return dlopen(name, RTLD_NOW | RTLD_LOCAL);
}

static void *library_symbol(cs_library library, const char *name) {
    return dlsym(library, name);
}

static void library_close(cs_library library) { dlclose(library); }

static const char *library_reason(void) { return dlerror(); }

static pthread_mutex_t state_lock = PTHREAD_MUTEX_INITIALIZER;
static void lock_state(void) { pthread_mutex_lock(&state_lock); }
static void unlock_state(void) { pthread_mutex_unlock(&state_lock); }
#endif

typedef struct {
    cs_library handle; /* libavformat */
    /* The Windows loader resolves one module at a time, so avcodec and avutil are opened
     * beside it; the ELF loader finds them through libavformat's own dependencies. */
    cs_library codec;
    cs_library util;
    /* libswscale is a separate library: libavformat does not depend on it. */
    cs_library sws;
    /* 0 untried, 1 ready, -1 unusable. */
    int state;
    char versions[64];

    unsigned (*codec_version)(void);
    unsigned (*format_version)(void);
    unsigned (*util_version)(void);
    const AVCodec *(*codec_iterate)(void **);
    int (*codec_is_encoder)(const AVCodec *);
    const AVCodec *(*find_encoder)(const char *);
    const AVOutputFormat *(*muxer_iterate)(void **);
    const AVOption *(*opt_next)(const void *, const AVOption *);
    const AVOption *(*opt_find)(void *, const char *, const char *, int, int);
    int (*opt_set)(void *, const char *, const char *, int);
#if LIBAVCODEC_VERSION_MAJOR >= 61
    int (*supported_config)(const AVCodecContext *, const AVCodec *,
                            enum AVCodecConfig, int, const void **, int *);
#endif
    AVCodecContext *(*alloc_context)(const AVCodec *);
    void (*free_context)(AVCodecContext **);
    int (*open2)(AVCodecContext *, const AVCodec *, AVDictionary **);
    AVFrame *(*frame_alloc)(void);
    void (*frame_free)(AVFrame **);
    int (*frame_get_buffer)(AVFrame *, int);
    AVPacket *(*packet_alloc)(void);
    void (*packet_free)(AVPacket **);
    int (*send_frame)(AVCodecContext *, const AVFrame *);
    int (*receive_packet)(AVCodecContext *, AVPacket *);
    int (*send_packet)(AVCodecContext *, const AVPacket *);
    int (*receive_frame)(AVCodecContext *, AVFrame *);
    int (*parameters_from_context)(AVCodecParameters *, const AVCodecContext *);
    AVFormatContext *(*format_alloc)(void);
    int (*open_input)(AVFormatContext **, const char *, AVInputFormat *, AVDictionary **);
    int (*find_stream_info)(AVFormatContext *, AVDictionary **);
    void (*close_input)(AVFormatContext **);
    int (*read_frame)(AVFormatContext *, AVPacket *);
    int (*parameters_to_context)(AVCodecContext *, const AVCodecParameters *);
    const AVCodec *(*find_decoder)(enum AVCodecID);
    void (*packet_unref)(AVPacket *);
    void (*frame_unref)(AVFrame *);
    const AVPixFmtDescriptor *(*pix_fmt_desc_get)(enum AVPixelFormat);
    int (*format_alloc_output)(AVFormatContext **, const AVOutputFormat *,
                               const char *, const char *);
    void (*format_free_context)(AVFormatContext *);
    AVStream *(*new_stream)(AVFormatContext *, const AVCodec *);
    const AVOutputFormat *(*guess_format)(const char *, const char *, const char *);
    enum AVCodecID (*guess_codec)(const AVOutputFormat *, const char *, const char *,
                                  const char *, enum AVMediaType);
    enum AVPixelFormat (*get_pix_fmt)(const char *);
    const char *(*get_pix_fmt_name)(enum AVPixelFormat);
    const char *(*get_codec_name)(enum AVCodecID);
    int (*write_header)(AVFormatContext *, AVDictionary **);
    int (*interleaved_write_frame)(AVFormatContext *, AVPacket *);
    int (*write_trailer)(AVFormatContext *);
    AVIOContext *(*io_alloc_context)(unsigned char *, int, int, void *,
                                     int (*)(void *, uint8_t *, int),
                                     int (*)(void *, uint8_t *, int),
                                     int64_t (*)(void *, int64_t, int));
    void (*io_context_free)(AVIOContext **);
    int (*io_flush)(AVIOContext *);
    void *(*av_malloc)(size_t);
    void (*av_free)(void *);
    int (*strerror_)(int, char *, size_t);
    void (*set_log_level)(int);
    struct SwsContext *(*sws_get_context)(int, int, enum AVPixelFormat, int, int,
                                          enum AVPixelFormat, int, void *, void *,
                                          const double *);
    int (*sws_scale)(struct SwsContext *, const uint8_t *const *, const int *, int,
                     int, uint8_t *const *, const int *);
    void (*sws_free_context)(struct SwsContext *);
    /* Optional: without them a decode still works, with libswscale's own guess at the
     * matrix, which is right for BT.709 and BT.601 and wrong for BT.2020. */
    const int *(*sws_coefficients)(int);
    int (*sws_colorspace_details)(struct SwsContext *, const int *, int, const int *,
                                  int, int, int, int);
} Lib;

/* One library state for the process, published whole: a probe builds a candidate and swaps
 * it in, so a call already running keeps a complete set of pointers. The libraries are never
 * unmapped — retrying opens another reference, and dropping the last one under a running call
 * would leave it calling freed code. */
static Lib lib;

/* Wide enough for a configured library path inside a message. Named apart from the `error`
 * parameter each call carries, which is the caller's buffer. */
static char failure_text[704];

static void copy_text(char *to, size_t cap, const char *from) {
    if (!from) {
        to[0] = 0;
        return;
    }
    snprintf(to, cap, "%s", from);
}

/* One symbol from whichever loaded module exports it. Windows reads one module at a time, so
 * avcodec and avutil stand beside libavformat; on ELF the first lookup already reaches them. */
static void *find_symbol(Lib *out, const char *name) {
#ifdef _WIN32
    void *found = library_symbol(out->handle, name);
    if (!found && out->codec) found = library_symbol(out->codec, name);
    if (!found && out->util) found = library_symbol(out->util, name);
    return found;
#else
    return library_symbol(out->handle, name);
#endif
}

#define LOAD(out, field, name)                                                 \
    do {                                                                       \
        *(void **)(&(out)->field) = find_symbol(out, name);                    \
        if (!(out)->field) {                                                   \
            snprintf(failure_text, sizeof failure_text, "missing symbol %s", name);          \
            goto failed;                                                       \
        }                                                                      \
    } while (0)

/* Fill `out` with the symbols this shim uses; `out->handle` must already be open. */
static int resolve(Lib *out) {
    LOAD(out, codec_version, "avcodec_version");
    LOAD(out, format_version, "avformat_version");
    LOAD(out, util_version, "avutil_version");
    LOAD(out, codec_iterate, "av_codec_iterate");
    LOAD(out, codec_is_encoder, "av_codec_is_encoder");
    LOAD(out, find_encoder, "avcodec_find_encoder_by_name");
    LOAD(out, muxer_iterate, "av_muxer_iterate");
    LOAD(out, opt_next, "av_opt_next");
    LOAD(out, opt_find, "av_opt_find");
    LOAD(out, opt_set, "av_opt_set");
#if LIBAVCODEC_VERSION_MAJOR >= 61
    LOAD(out, supported_config, "avcodec_get_supported_config");
#endif
    LOAD(out, alloc_context, "avcodec_alloc_context3");
    LOAD(out, free_context, "avcodec_free_context");
    LOAD(out, open2, "avcodec_open2");
    LOAD(out, frame_alloc, "av_frame_alloc");
    LOAD(out, frame_free, "av_frame_free");
    LOAD(out, frame_get_buffer, "av_frame_get_buffer");
    LOAD(out, packet_alloc, "av_packet_alloc");
    LOAD(out, packet_free, "av_packet_free");
    LOAD(out, send_frame, "avcodec_send_frame");
    LOAD(out, receive_packet, "avcodec_receive_packet");
    LOAD(out, send_packet, "avcodec_send_packet");
    LOAD(out, receive_frame, "avcodec_receive_frame");
    LOAD(out, parameters_from_context, "avcodec_parameters_from_context");
    LOAD(out, format_alloc, "avformat_alloc_context");
    LOAD(out, open_input, "avformat_open_input");
    LOAD(out, find_stream_info, "avformat_find_stream_info");
    LOAD(out, close_input, "avformat_close_input");
    LOAD(out, read_frame, "av_read_frame");
    LOAD(out, parameters_to_context, "avcodec_parameters_to_context");
    LOAD(out, find_decoder, "avcodec_find_decoder");
    LOAD(out, packet_unref, "av_packet_unref");
    LOAD(out, frame_unref, "av_frame_unref");
    LOAD(out, pix_fmt_desc_get, "av_pix_fmt_desc_get");
    LOAD(out, format_alloc_output, "avformat_alloc_output_context2");
    LOAD(out, format_free_context, "avformat_free_context");
    LOAD(out, new_stream, "avformat_new_stream");
    LOAD(out, guess_format, "av_guess_format");
    LOAD(out, guess_codec, "av_guess_codec");
    LOAD(out, get_pix_fmt, "av_get_pix_fmt");
    LOAD(out, get_pix_fmt_name, "av_get_pix_fmt_name");
    LOAD(out, get_codec_name, "avcodec_get_name");
    LOAD(out, write_header, "avformat_write_header");
    LOAD(out, interleaved_write_frame, "av_interleaved_write_frame");
    LOAD(out, write_trailer, "av_write_trailer");
    LOAD(out, io_alloc_context, "avio_alloc_context");
    LOAD(out, io_context_free, "avio_context_free");
    LOAD(out, io_flush, "avio_flush");
    LOAD(out, av_malloc, "av_malloc");
    LOAD(out, av_free, "av_free");
    LOAD(out, strerror_, "av_strerror");
    LOAD(out, set_log_level, "av_log_set_level");
    return 0;
failed:
    return -1;
}

/* A directory the caller wants searched before the dynamic linker's own paths. */
static char library_directory[512];

void cs_reopen(const char *directory) {
    lock_state();
    snprintf(library_directory, sizeof library_directory, "%s", directory ? directory : "");
#ifdef _WIN32
    /* A configured path may carry either separator; the loader's own flag takes one. */
    for (char *at = library_directory; *at; at++) {
        if (*at == '/') *at = '\\';
    }
#endif
    /* What is published stays usable; the next cs_open builds a candidate from scratch. */
    lib.state = 0;
    unlock_state();
}

/* `<library_directory><separator><name>` written into `path`, or NULL when none was set. */
static const char *in_library_directory(char *path, size_t cap, const char *name) {
    if (!library_directory[0]) return NULL;
    snprintf(path, cap, "%s" CS_SEPARATOR "%s", library_directory, name);
    return path;
}

/* Choose the first of the configured directory, the versioned name and the bare one. A total
 * failure reports the configured path's reason: that is the one the caller set. */
static int open_any(cs_library *into, const char *versioned, const char *bare) {
    char path[600];
    char configured_failure[2048];
    configured_failure[0] = 0;
    const char *configured = in_library_directory(path, sizeof path, versioned);
    if (configured) {
        *into = library_open(configured, 1);
        if (!*into) {
            snprintf(configured_failure, sizeof configured_failure, "%s: %s", configured,
                     library_reason());
        }
    }
    if (!*into) {
        configured = in_library_directory(path, sizeof path, bare);
        if (configured) *into = library_open(configured, 1);
    }
    if (!*into) *into = library_open(versioned, 0);
    if (!*into) *into = library_open(bare, 0);
    if (*into) return 0;
    if (configured_failure[0]) {
        snprintf(failure_text, sizeof failure_text, "%s", configured_failure);
        return -1;
    }
    const char *reason = library_reason();
    snprintf(failure_text, sizeof failure_text, "%s: %s", versioned,
             reason ? reason : "not found");
    return -1;
}

#ifdef _WIN32
/* The modules libavformat does not answer for: GetProcAddress reads one module, where the
 * ELF loader reaches a name through its dependencies. */
static int open_companions(Lib *out) {
    if (open_any(&out->codec, CS_CODEC_VERSIONED, CS_CODEC_BARE) != 0) return -1;
    if (open_any(&out->util, CS_UTIL_VERSIONED, CS_UTIL_BARE) != 0) return -1;
    return 0;
}
#else
static int open_companions(Lib *out) {
    (void)out;
    return 0;
}
#endif

/* Release a candidate that never became the process's state. */
static void close_candidate(Lib *out) {
    if (out->sws) library_close(out->sws);
    if (out->codec) library_close(out->codec);
    if (out->util) library_close(out->util);
    if (out->handle) library_close(out->handle);
    out->sws = out->codec = out->util = out->handle = NULL;
}

/* Open the libraries into `out`; 0 when no name works, with `failure_text` saying why.
 * Candidates that are not published are closed again; nothing that was published ever is. */
static int load(Lib *out) {
    /* The headers pin the ABI, so only the matching major can be used. Asking for the
     * versioned name first also keeps an older install (a compat package) from being picked
     * up by the unversioned one. */
    if (open_any(&out->handle, CS_FORMAT_VERSIONED, CS_FORMAT_BARE) != 0) return 0;
    if (open_companions(out) != 0) {
        close_candidate(out);
        return 0;
    }
    if (resolve(out) != 0) {
        close_candidate(out);
        return 0;
    }
    open_any(&out->sws, CS_SWS_VERSIONED, CS_SWS_BARE);
    if (out->sws) {
        *(void **)(&out->sws_get_context) = library_symbol(out->sws, "sws_getContext");
        *(void **)(&out->sws_scale) = library_symbol(out->sws, "sws_scale");
        *(void **)(&out->sws_free_context) = library_symbol(out->sws, "sws_freeContext");
        if (!out->sws_get_context || !out->sws_scale || !out->sws_free_context) {
            library_close(out->sws);
            out->sws = NULL;
        }
        if (out->sws) {
            /* Optional: a decode without them keeps libswscale's own matrix guess. */
            *(void **)(&out->sws_coefficients) = library_symbol(out->sws, "sws_getCoefficients");
            *(void **)(&out->sws_colorspace_details) =
                library_symbol(out->sws, "sws_setColorspaceDetails");
        }
    }
    unsigned codec = out->codec_version();
    unsigned format = out->format_version();
    unsigned util = out->util_version();
    if ((int)(codec >> 16) != LIBAVCODEC_VERSION_MAJOR ||
        (int)(format >> 16) != LIBAVFORMAT_VERSION_MAJOR ||
        (int)(util >> 16) != LIBAVUTIL_VERSION_MAJOR) {
        snprintf(failure_text, sizeof failure_text,
                 "avcodec %u, avformat %u, avutil %u; these headers are %d, %d, %d",
                 codec >> 16, format >> 16, util >> 16,
                 LIBAVCODEC_VERSION_MAJOR, LIBAVFORMAT_VERSION_MAJOR,
                 LIBAVUTIL_VERSION_MAJOR);
        close_candidate(out);
        return 0;
    }
    snprintf(out->versions, sizeof out->versions, "%u.%u.%u %u.%u.%u %u.%u.%u",
             codec >> 16, (codec >> 8) & 0xff, codec & 0xff,
             format >> 16, (format >> 8) & 0xff, format & 0xff,
             util >> 16, (util >> 8) & 0xff, util & 0xff);
    if (!out->sws) {
        snprintf(failure_text, sizeof failure_text, "libavformat loaded but libswscale did not");
        close_candidate(out);
        return 0;
    }
    /* The libraries log their choices to stderr; a GUI tool has no place for an
     * informational line on every save. Errors still come through. */
    out->set_log_level(AV_LOG_ERROR);
    return 1;
}

int cs_open(void) {
    lock_state();
    int ready;
    if (lib.state != 0) {
        ready = lib.state > 0;
    } else {
        Lib candidate;
        memset(&candidate, 0, sizeof candidate);
        ready = load(&candidate);
        if (ready) {
            candidate.state = 1;
            lib = candidate;
        } else {
            lib.state = -1;
        }
    }
    unlock_state();
    return ready;
}

const char *cs_versions(void) { return cs_open() ? lib.versions : NULL; }
const char *cs_error(void) {
    if (lib.state >= 0) return NULL;
    return failure_text[0] ? failure_text : "the ffmpeg libraries could not be used";
}

/* Every listing below fills at most `cap` entries and always returns the total, so a caller
 * sizes its buffer with one call and fills it with the next. */

int cs_muxers(cs_muxer *out, int cap) {
    if (!cs_open()) return 0;
    void *opaque = NULL;
    const AVOutputFormat *f;
    int n = 0;
    while ((f = lib.muxer_iterate(&opaque))) {
        if (!f->name || f->name[0] == '_') continue;
        if (n < cap) {
            cs_muxer *m = &out[n];
            memset(m, 0, sizeof *m);
            copy_text(m->name, sizeof m->name, f->name);
            copy_text(m->long_name, sizeof m->long_name, f->long_name);
            copy_text(m->extensions, sizeof m->extensions, f->extensions);
            copy_text(m->mime, sizeof m->mime, f->mime_type);
            m->video_codec = (int)f->video_codec;
        }
        n++;
    }
    return n;
}

int cs_encoders(cs_encoder *out, int cap) {
    if (!cs_open()) return 0;
    void *opaque = NULL;
    const AVCodec *c;
    int n = 0;
    while ((c = lib.codec_iterate(&opaque))) {
        if (!lib.codec_is_encoder(c) || c->type != AVMEDIA_TYPE_VIDEO) continue;
        if (n < cap) {
            cs_encoder *e = &out[n];
            memset(e, 0, sizeof *e);
            copy_text(e->name, sizeof e->name, c->name);
            copy_text(e->long_name, sizeof e->long_name, c->long_name);
            e->id = (int)c->id;
        }
        n++;
    }
    return n;
}

static const AVCodec *video_encoder(const char *name) {
    if (!cs_open() || !name) return NULL;
    const AVCodec *c = lib.find_encoder(name);
    if (!c || !lib.codec_is_encoder(c) || c->type != AVMEDIA_TYPE_VIDEO) return NULL;
    return c;
}

static void option_default(const AVOption *o, char *buf, size_t cap) {
    switch (o->type) {
    case AV_OPT_TYPE_BOOL:
    case AV_OPT_TYPE_INT:
    case AV_OPT_TYPE_INT64:
    case AV_OPT_TYPE_FLAGS:
    case AV_OPT_TYPE_DURATION:
    case AV_OPT_TYPE_PIXEL_FMT:
    case AV_OPT_TYPE_SAMPLE_FMT:
    case AV_OPT_TYPE_CHLAYOUT:
        snprintf(buf, cap, "%lld", (long long)o->default_val.i64);
        break;
    case AV_OPT_TYPE_DOUBLE:
    case AV_OPT_TYPE_FLOAT:
        snprintf(buf, cap, "%g", o->default_val.dbl);
        break;
    case AV_OPT_TYPE_STRING:
        snprintf(buf, cap, "%s", o->default_val.str ? o->default_val.str : "");
        break;
    case AV_OPT_TYPE_RATIONAL:
        snprintf(buf, cap, "%d/%d", o->default_val.q.num, o->default_val.q.den);
        break;
    default:
        buf[0] = 0;
        break;
    }
}

int cs_options(const char *encoder, cs_option *out, int cap) {
    const AVCodec *c = video_encoder(encoder);
    if (!c) return -1;
    const AVClass *klass = c->priv_class;
    if (!klass) return 0;
    const AVOption *o = NULL;
    int n = 0;
    while ((o = lib.opt_next(&klass, o))) {
        if (o->type == AV_OPT_TYPE_CONST) continue;
        if (!(o->flags & AV_OPT_FLAG_ENCODING_PARAM)) continue;
        if (n < cap) {
            cs_option *e = &out[n];
            memset(e, 0, sizeof *e);
            copy_text(e->name, sizeof e->name, o->name);
            copy_text(e->help, sizeof e->help, o->help);
            option_default(o, e->default_text, sizeof e->default_text);
        }
        n++;
    }
    return n;
}

int cs_has_option(const char *encoder, const char *name) {
    const AVCodec *c = video_encoder(encoder);
    if (!c || !name) return -1;
    AVCodecContext *ctx = lib.alloc_context(c);
    if (!ctx) return -1;
    /* The context's own class and the encoder's private class both count: quality lives in
     * the former, bitrate and crf in the latter. */
    const AVOption *found = lib.opt_find(ctx, name, NULL, AV_OPT_FLAG_ENCODING_PARAM,
                                         AV_OPT_SEARCH_CHILDREN);
    lib.free_context(&ctx);
    return found != NULL;
}

static const enum AVPixelFormat *encoder_pix_fmts(const AVCodec *c, int *count) {
#if LIBAVCODEC_VERSION_MAJOR >= 61
    const enum AVPixelFormat *fmts = NULL;
    *count = 0;
    if (lib.supported_config &&
        lib.supported_config(NULL, c, AV_CODEC_CONFIG_PIX_FORMAT, 0,
                             (const void **)&fmts, count) >= 0 &&
        fmts) {
        return fmts;
    }
    *count = 0;
    return NULL;
#else
    const enum AVPixelFormat *fmts = c->pix_fmts;
    *count = 0;
    while (fmts && fmts[*count] != AV_PIX_FMT_NONE) (*count)++;
    return fmts;
#endif
}

int cs_pix_fmts(const char *encoder, int *out, int cap) {
    const AVCodec *c = video_encoder(encoder);
    if (!c) return -1;
    int count = 0;
    const enum AVPixelFormat *fmts = encoder_pix_fmts(c, &count);
    if (!fmts) return 0;
    for (int i = 0; i < count && i < cap; i++) out[i] = (int)fmts[i];
    return count;
}

int cs_pix_fmt(const char *name) {
    if (!cs_open() || !name) return -1;
    return (int)lib.get_pix_fmt(name);
}

const char *cs_pix_fmt_name(int format) {
    if (!cs_open()) return NULL;
    return lib.get_pix_fmt_name((enum AVPixelFormat)format);
}

const char *cs_codec_name(int codec) {
    if (!cs_open()) return NULL;
    return lib.get_codec_name((enum AVCodecID)codec);
}

int cs_guess_codec(const char *extension) {
    if (!cs_open() || !extension) return 0;
    char name[64];
    snprintf(name, sizeof name, "out.%s", extension);
    const AVOutputFormat *format = lib.guess_format(NULL, name, NULL);
    if (!format) return 0;
    return (int)lib.guess_codec(format, NULL, name, NULL, AVMEDIA_TYPE_VIDEO);
}

void cs_srgb(cs_colour *colour) {
    if (!colour) return;
    colour->primaries = AVCOL_PRI_BT709;
    colour->transfer = AVCOL_TRC_IEC61966_2_1;
    colour->space = AVCOL_SPC_BT709;
    colour->range = AVCOL_RANGE_JPEG;
}

static void fail_message(char *error, int cap, const char *what, int code) {
    char text[128] = {0};
    if (lib.strerror_) lib.strerror_(code, text, sizeof text);
    snprintf(error, (size_t)cap, "%s: %s", what, text[0] ? text : "unknown error");
}

/* The encoder's formats, one we can fill without a conversion first. */
static enum AVPixelFormat pick_format(const AVCodec *c) {
    int count = 0;
    const enum AVPixelFormat *fmts = encoder_pix_fmts(c, &count);
    if (!fmts || count == 0) return AV_PIX_FMT_RGBA;
    static const enum AVPixelFormat preferred[] = {AV_PIX_FMT_RGBA, AV_PIX_FMT_BGRA,
                                                   AV_PIX_FMT_ARGB, AV_PIX_FMT_ABGR,
                                                   AV_PIX_FMT_RGB24, AV_PIX_FMT_BGR24};
    for (size_t p = 0; p < sizeof preferred / sizeof preferred[0]; p++) {
        for (int i = 0; i < count; i++) {
            if (fmts[i] == preferred[p]) return fmts[i];
        }
    }
    return fmts[0];
}

/* A growable buffer the muxer writes into. `pos` is where the next write lands and `size`
 * is how far the stream has ever reached: a muxer that seeks back to patch a header
 * (avif does) must still see the earlier end when it later seeks to or reports the end,
 * and must not lose the tail it is about to overwrite. */
typedef struct {
    unsigned char *data;
    long pos;
    long size;
    long capacity;
    int failed;
} Buffer;

static int buffer_write(void *opaque, uint8_t *data, int size) {
    Buffer *b = opaque;
    if (size <= 0) return 0;
    if (b->pos + size > b->capacity) {
        long want = b->capacity ? b->capacity : 65536;
        while (want < b->pos + size) want *= 2;
        unsigned char *grown = realloc(b->data, (size_t)want);
        if (!grown) {
            b->failed = 1;
            return AVERROR(ENOMEM);
        }
        b->data = grown;
        b->capacity = want;
    }
    memcpy(b->data + b->pos, data, (size_t)size);
    b->pos += size;
    if (b->pos > b->size) b->size = b->pos;
    return 0;
}

/* Some muxers (avif) rewrite parts of the header and need to seek. */
static int64_t buffer_seek(void *opaque, int64_t offset, int whence) {
    Buffer *b = opaque;
    if (whence == AVSEEK_SIZE) return b->size;
    int64_t base;
    switch (whence & ~AVSEEK_FORCE) {
    case SEEK_SET: base = 0; break;
    case SEEK_CUR: base = b->pos; break;
    case SEEK_END: base = b->size; break;
    default: return AVERROR(EINVAL);
    }
    int64_t target = base + offset;
    if (target < 0) return AVERROR(EINVAL);
    b->pos = (long)target;
    return target;
}

static void release_picture(AVFrame **frame) {
    if (*frame) lib.frame_free(frame);
}

/* Apply the caller's encoder options: one the encoder does not have is left alone (the same
 * recipe serves several encoders), anything else is a real refusal worth reporting. */
static int apply_options(AVCodecContext *ctx, const cs_picture *picture,
                         char *error, int error_cap) {
    for (int i = 0; i < picture->option_count; i++) {
        const char *name = picture->option_names[i];
        const char *value = picture->option_values[i];
        if (!name || !value) continue;
        int code = lib.opt_set(ctx, name, value, AV_OPT_SEARCH_CHILDREN);
        if (code == AVERROR_OPTION_NOT_FOUND) continue;
        if (code < 0) {
            char text[128] = {0};
            lib.strerror_(code, text, sizeof text);
            snprintf(error, (size_t)error_cap, "option %s=%s: %s", name, value,
                     text[0] ? text : "refused");
            return -1;
        }
    }
    return 0;
}

int cs_encode(const char *encoder, const char *extension, const cs_picture *picture,
              unsigned char **out, long *size, char *error, int error_cap) {
    *out = NULL;
    *size = 0;
    if (!cs_open()) {
        snprintf(error, (size_t)error_cap, "%s", cs_error() ? cs_error() : "no libraries");
        return -1;
    }
    if (!picture || !picture->rgba || picture->width <= 0 || picture->height <= 0) {
        snprintf(error, (size_t)error_cap, "nothing to encode");
        return -1;
    }
    const int width = picture->width;
    const int height = picture->height;
    const unsigned char *rgba = picture->rgba;
    const long pitch = picture->pitch;
    const AVCodec *codec = video_encoder(encoder);
    if (!codec) {
        snprintf(error, (size_t)error_cap, "no video encoder named %s", encoder);
        return -1;
    }
    /* The container decides whether the codec config goes in the stream or in the header,
     * so it has to be known before the encoder opens. avif keeps it in the header
     * (av1C), and an encoder opened without the flag carries no extradata to put there. */
    char name[32];
    snprintf(name, sizeof name, "out.%s", extension);
    const AVOutputFormat *muxer = lib.guess_format(NULL, name, NULL);
    if (!muxer) {
        snprintf(error, (size_t)error_cap, "no muxer writes .%s", extension);
        return -1;
    }
    /* image2 writes a file per frame and insists on a name pattern; the pipe variant is
     * the one that writes the picture to the stream it is given. */
    if (!strcmp(muxer->name, "image2")) {
        muxer = lib.guess_format("image2pipe", NULL, NULL);
        if (!muxer) {
            snprintf(error, (size_t)error_cap, "no image2pipe muxer");
            return -1;
        }
    }
    int result = -1;
    AVCodecContext *ctx = NULL;
    AVFrame *frame = NULL;
    AVPacket *packet = NULL;
    AVFormatContext *format = NULL;
    Buffer buffer = {0};
    AVIOContext *io = NULL;
    unsigned char *io_buffer = NULL;
    struct SwsContext *sws = NULL;

    enum AVPixelFormat target = picture->pix_fmt >= 0
                                    ? (enum AVPixelFormat)picture->pix_fmt
                                    : pick_format(codec);
    ctx = lib.alloc_context(codec);
    if (!ctx) {
        snprintf(error, (size_t)error_cap, "cannot allocate an encoder context");
        goto done;
    }
    ctx->width = width;
    ctx->height = height;
    ctx->pix_fmt = target;
    /* A still image has no clock; the muxer still wants a plausible one. */
    ctx->time_base = (AVRational){1, 25};
    ctx->framerate = (AVRational){25, 1};
    if (picture->colour.primaries) {
        ctx->color_primaries = (enum AVColorPrimaries)picture->colour.primaries;
    }
    if (picture->colour.transfer) {
        ctx->color_trc = (enum AVColorTransferCharacteristic)picture->colour.transfer;
    }
    if (picture->colour.space) {
        ctx->colorspace = (enum AVColorSpace)picture->colour.space;
    }
    if (picture->colour.range) {
        ctx->color_range = (enum AVColorRange)picture->colour.range;
    }
    if (picture->global_quality >= 0) {
        ctx->global_quality = picture->global_quality;
        ctx->flags |= AV_CODEC_FLAG_QSCALE;
    }
    if (apply_options(ctx, picture, error, error_cap) != 0) goto done;
    if (muxer->flags & AVFMT_GLOBALHEADER) ctx->flags |= AV_CODEC_FLAG_GLOBAL_HEADER;
    int code = lib.open2(ctx, codec, NULL);
    if (code < 0) {
        fail_message(error, error_cap, "the encoder refused its settings", code);
        goto done;
    }

    frame = lib.frame_alloc();
    if (!frame) {
        snprintf(error, (size_t)error_cap, "cannot allocate a frame");
        goto done;
    }
    frame->format = (int)target;
    frame->width = width;
    frame->height = height;
    code = lib.frame_get_buffer(frame, 32);
    if (code < 0) {
        fail_message(error, error_cap, "cannot allocate a picture", code);
        goto done;
    }
    if (target == AV_PIX_FMT_RGBA) {
        long row = (long)width * 4;
        for (int y = 0; y < height; y++) {
            memcpy(frame->data[0] + (long)y * frame->linesize[0],
                   rgba + (long)y * pitch, (size_t)row);
        }
    } else {
        sws = lib.sws_get_context(width, height, AV_PIX_FMT_RGBA, width, height,
                                  target, SWS_BILINEAR, NULL, NULL, NULL);
        if (!sws) {
            snprintf(error, (size_t)error_cap, "cannot convert to the encoder's format");
            goto done;
        }
        const uint8_t *source[1] = {rgba};
        const int source_lines[1] = {(int)pitch};
        if (lib.sws_scale(sws, source, source_lines, 0, height, frame->data,
                          frame->linesize) <= 0) {
            snprintf(error, (size_t)error_cap, "the colour conversion produced nothing");
            goto done;
        }
    }

    /* One still picture: send it, close the stream, and take whatever the encoder
     * needed to buffer before it would produce a packet. */
    code = lib.send_frame(ctx, frame);
    if (code < 0) {
        fail_message(error, error_cap, "the encoder refused the picture", code);
        goto done;
    }
    lib.send_frame(ctx, NULL);

    code = lib.format_alloc_output(&format, muxer, NULL, NULL);
    if (code < 0 || !format) {
        fail_message(error, error_cap, "cannot open the muxer", code);
        goto done;
    }
    AVStream *stream = lib.new_stream(format, NULL);
    if (!stream) {
        snprintf(error, (size_t)error_cap, "cannot add a stream");
        goto done;
    }
    stream->time_base = ctx->time_base;
    code = lib.parameters_from_context(stream->codecpar, ctx);
    if (code < 0) {
        fail_message(error, error_cap, "cannot describe the stream", code);
        goto done;
    }
    io_buffer = lib.av_malloc(65536);
    if (!io_buffer) {
        snprintf(error, (size_t)error_cap, "cannot allocate an output buffer");
        goto done;
    }
    io = lib.io_alloc_context(io_buffer, 65536, 1, &buffer, NULL, buffer_write,
                              buffer_seek);
    if (!io) {
        snprintf(error, (size_t)error_cap, "cannot open the output stream");
        goto done;
    }
    /* avio owns the buffer from here, and the format context takes the stream. */
    io_buffer = NULL;
    format->pb = io;
    io = NULL;
    code = lib.write_header(format, NULL);
    if (code < 0) {
        fail_message(error, error_cap, "the muxer refused the header", code);
        goto done;
    }
    packet = lib.packet_alloc();
    if (!packet) {
        snprintf(error, (size_t)error_cap, "cannot allocate a packet");
        goto done;
    }
    int written = 0;
    while ((code = lib.receive_packet(ctx, packet)) == 0) {
        packet->stream_index = stream->index;
        packet->flags |= AV_PKT_FLAG_KEY;
        /* One picture, stamped where the stream starts. */
        packet->pts = packet->dts = 0;
        packet->duration = 1;
        code = lib.interleaved_write_frame(format, packet);
        if (code < 0) {
            fail_message(error, error_cap, "the muxer refused the picture", code);
            goto done;
        }
        written++;
    }
    if (!written) {
        fail_message(error, error_cap, "the encoder produced nothing", code);
        goto done;
    }
    code = lib.write_trailer(format);
    if (code < 0) {
        fail_message(error, error_cap, "the muxer failed to finish", code);
        goto done;
    }
    /* AVIO buffers 32 KiB; nothing past the last flush is in the buffer yet. */
    lib.io_flush(format->pb);
    if (buffer.failed || buffer.size == 0) {
        snprintf(error, (size_t)error_cap, "the output could not be collected");
        goto done;
    }
    *out = buffer.data;
    *size = buffer.size;
    buffer.data = NULL;
    result = 0;

done:
    if (buffer.data) free(buffer.data);
    /* The format context borrowed the stream; free it before the context goes. */
    if (format && format->pb) lib.io_context_free(&format->pb);
    if (io) lib.io_context_free(&io);
    if (io_buffer) lib.av_free(io_buffer);
    if (format) lib.format_free_context(format);
    if (packet) lib.packet_free(&packet);
    release_picture(&frame);
    if (sws) lib.sws_free_context(sws);
    if (ctx) lib.free_context(&ctx);
    return result;
}

/* A call holds the whole file it is decoding: libavformat reads it through this callback
 * instead of opening the path itself, so a picture can also arrive as bytes. */
typedef struct {
    const unsigned char *data;
    long size;
    long offset;
} MemoryReader;

static int memory_read(void *opaque, uint8_t *to, int cap) {
    MemoryReader *reader = opaque;
    if (reader->offset >= reader->size) return AVERROR_EOF;
    long left = reader->size - reader->offset;
    long take = left < cap ? left : cap;
    memcpy(to, reader->data + reader->offset, (size_t)take);
    reader->offset += take;
    return (int)take;
}

/* ISO-BMFF (AVIF, HEIF) keeps its index at the end and seeks back to the picture, so a
 * memory source that cannot seek reads as a partial file. */
static int64_t memory_seek(void *opaque, int64_t offset, int whence) {
    MemoryReader *reader = opaque;
    int64_t base;
    switch (whence & ~AVSEEK_FORCE) {
    case AVSEEK_SIZE: return reader->size;
    case SEEK_SET: base = 0; break;
    case SEEK_CUR: base = reader->offset; break;
    case SEEK_END: base = reader->size; break;
    default: return AVERROR(EINVAL);
    }
    int64_t target = base + offset;
    if (target < 0 || target > reader->size) return AVERROR(EINVAL);
    reader->offset = (long)target;
    return target;
}

/* The frame's own tags win over the stream's: a decoder that knows better says so there. */
static cs_colour decoded_colour(const AVCodecParameters *parameters, const AVFrame *frame) {
    cs_colour colour;
    colour.primaries = frame->color_primaries != AVCOL_PRI_UNSPECIFIED
                           ? (int)frame->color_primaries
                           : (int)parameters->color_primaries;
    colour.transfer = frame->color_trc != AVCOL_TRC_UNSPECIFIED
                          ? (int)frame->color_trc
                          : (int)parameters->color_trc;
    colour.space = frame->colorspace != AVCOL_SPC_UNSPECIFIED
                       ? (int)frame->colorspace
                       : (int)parameters->color_space;
    colour.range = frame->color_range != AVCOL_RANGE_UNSPECIFIED
                       ? (int)frame->color_range
                       : (int)parameters->color_range;
    return colour;
}

/* Tell libswscale which matrix to undo. Without it the guess is right for BT.709 and BT.601
 * and wrong for BT.2020, which is exactly the case a deep picture comes in. */
static void set_matrix(struct SwsContext *sws, cs_colour colour, int rgb) {
    if (rgb || !lib.sws_coefficients || !lib.sws_colorspace_details) return;
    int standard = 0;
    switch (colour.space) {
    case AVCOL_SPC_BT709: standard = SWS_CS_ITU709; break;
    case AVCOL_SPC_BT470BG:
    case AVCOL_SPC_SMPTE170M: standard = SWS_CS_ITU601; break;
    case AVCOL_SPC_SMPTE240M: standard = SWS_CS_SMPTE240M; break;
    case AVCOL_SPC_FCC: standard = SWS_CS_FCC; break;
#if LIBSWSCALE_VERSION_MAJOR >= 6
    case AVCOL_SPC_BT2020_NCL:
    case AVCOL_SPC_BT2020_CL: standard = SWS_CS_BT2020; break;
#endif
    default: return;
    }
    const int *table = lib.sws_coefficients(standard);
    if (!table) return;
    int full = colour.range == AVCOL_RANGE_JPEG;
    lib.sws_colorspace_details(sws, table, full, table, 1, 0, 1 << 16, 1 << 16);
}

/* Narrow the 16-bit channels of an RGBA64LE picture into 10 bits each, keeping the alpha
 * as the two-bit flag Packed10 has room for. */
static void pack_ten_bit(const unsigned char *wide, unsigned char *packed, long pixels) {
    for (long i = 0; i < pixels; i++) {
        const unsigned char *texel = wide + i * 8;
        unsigned r = (unsigned)(texel[0] | (texel[1] << 8));
        unsigned g = (unsigned)(texel[2] | (texel[3] << 8));
        unsigned b = (unsigned)(texel[4] | (texel[5] << 8));
        unsigned a = (unsigned)(texel[6] | (texel[7] << 8));
        uint32_t word = (r >> 6) | ((g >> 6) << 10) | ((b >> 6) << 20) | ((a >> 14) << 30);
        packed[i * 4 + 0] = (unsigned char)(word & 0xff);
        packed[i * 4 + 1] = (unsigned char)((word >> 8) & 0xff);
        packed[i * 4 + 2] = (unsigned char)((word >> 16) & 0xff);
        packed[i * 4 + 3] = (unsigned char)((word >> 24) & 0xff);
    }
}

int cs_decode(const char *path, const unsigned char *data, long size,
              unsigned char **out, long *out_size, cs_decoded *info,
              char *error, int error_cap) {
    *out = NULL;
    *out_size = 0;
    if (!cs_open()) {
        snprintf(error, (size_t)error_cap, "%s", cs_error() ? cs_error() : "no libraries");
        return -1;
    }
    if (!info) {
        snprintf(error, (size_t)error_cap, "no place to describe the picture");
        return -1;
    }
    if (!path && (!data || size <= 0)) {
        snprintf(error, (size_t)error_cap, "nothing to decode");
        return -1;
    }
    const char *name = path ? path : "the bytes";
    int result = -1;
    AVFormatContext *format = NULL;
    AVCodecContext *ctx = NULL;
    AVPacket *packet = NULL;
    AVFrame *frame = NULL;
    AVIOContext *io = NULL;
    unsigned char *io_buffer = NULL;
    MemoryReader reader = {data, size, 0};
    struct SwsContext *sws = NULL;
    unsigned char *wide = NULL;
    unsigned char *packed = NULL;

    format = lib.format_alloc();
    if (!format) {
        snprintf(error, (size_t)error_cap, "cannot allocate a demuxer");
        goto done;
    }
    if (data && !path) {
        io_buffer = lib.av_malloc(65536);
        if (!io_buffer) {
            snprintf(error, (size_t)error_cap, "cannot allocate a read buffer");
            goto done;
        }
        io = lib.io_alloc_context(io_buffer, 65536, 0, &reader, memory_read, NULL, memory_seek);
        if (!io) {
            snprintf(error, (size_t)error_cap, "cannot open the bytes for reading");
            goto done;
        }
        /* The context outlives this function but does not free its buffer; `done` does. */
        io_buffer = NULL;
        format->pb = io;
        format->flags |= AVFMT_FLAG_CUSTOM_IO;
    }
    int code = lib.open_input(&format, path, NULL, NULL);
    if (code < 0 || !format) {
        fail_message(error, error_cap, "cannot open the picture", code);
        goto done;
    }
    code = lib.find_stream_info(format, NULL);
    if (code < 0) {
        fail_message(error, error_cap, "cannot read the picture's header", code);
        goto done;
    }
    int index = -1;
    for (unsigned i = 0; i < format->nb_streams; i++) {
        if (format->streams[i]->codecpar->codec_type == AVMEDIA_TYPE_VIDEO) {
            index = (int)i;
            break;
        }
    }
    if (index < 0) {
        snprintf(error, (size_t)error_cap, "no picture in %s", name);
        goto done;
    }
    const AVCodecParameters *parameters = format->streams[index]->codecpar;
    const AVCodec *codec = lib.find_decoder(parameters->codec_id);
    if (!codec) {
        const char *codec_name = lib.get_codec_name(parameters->codec_id);
        snprintf(error, (size_t)error_cap, "no decoder for %s",
                 codec_name ? codec_name : "this codec");
        goto done;
    }
    ctx = lib.alloc_context(codec);
    if (!ctx) {
        snprintf(error, (size_t)error_cap, "cannot allocate a decoder context");
        goto done;
    }
    code = lib.parameters_to_context(ctx, parameters);
    if (code < 0) {
        fail_message(error, error_cap, "cannot describe the stream to the decoder", code);
        goto done;
    }
    code = lib.open2(ctx, codec, NULL);
    if (code < 0) {
        fail_message(error, error_cap, "the decoder refused to open", code);
        goto done;
    }
    packet = lib.packet_alloc();
    frame = lib.frame_alloc();
    if (!packet || !frame) {
        snprintf(error, (size_t)error_cap, "cannot allocate a picture");
        goto done;
    }

    /* One pin is one still, so reading stops at the first frame; an animated file gives its
     * first, and a decoder may only release it once the stream is flushed. */
    int flushed = 0;
    for (;;) {
        code = lib.read_frame(format, packet);
        if (code == AVERROR_EOF) {
            if (flushed) {
                snprintf(error, (size_t)error_cap, "no picture in %s", name);
                goto done;
            }
            flushed = 1;
            lib.send_packet(ctx, NULL);
        } else if (code < 0) {
            fail_message(error, error_cap, "cannot read the picture", code);
            goto done;
        } else {
            int mine = packet->stream_index == index;
            code = mine ? lib.send_packet(ctx, packet) : 0;
            lib.packet_unref(packet);
            if (code < 0 && code != AVERROR(EAGAIN)) {
                fail_message(error, error_cap, "the decoder refused the picture", code);
                goto done;
            }
        }
        code = lib.receive_frame(ctx, frame);
        if (code >= 0) break;
        if (code == AVERROR_EOF) {
            /* Flushed and drained with nothing to hand over: the stream has no frame. */
            snprintf(error, (size_t)error_cap, "no picture in %s", name);
            goto done;
        }
        if (code != AVERROR(EAGAIN)) {
            fail_message(error, error_cap, "the picture could not be decoded", code);
            goto done;
        }
    }

    const AVPixFmtDescriptor *descriptor = lib.pix_fmt_desc_get((enum AVPixelFormat)frame->format);
    int depth = descriptor && descriptor->nb_components > 0 ? descriptor->comp[0].depth : 8;
    int rgb = descriptor && (descriptor->flags & AV_PIX_FMT_FLAG_RGB);
    cs_colour colour = decoded_colour(parameters, frame);
    int hdr = colour.transfer == AVCOL_TRC_SMPTE2084 || colour.transfer == AVCOL_TRC_ARIB_STD_B67;
    int deep = depth > 8 || hdr;
    const int width = frame->width;
    const int height = frame->height;
    if (width <= 0 || height <= 0) {
        snprintf(error, (size_t)error_cap, "the picture has no size");
        goto done;
    }

    enum AVPixelFormat target = deep ? AV_PIX_FMT_RGBA64LE : AV_PIX_FMT_RGBA;
    sws = lib.sws_get_context(width, height, (enum AVPixelFormat)frame->format, width, height,
                              target, SWS_BILINEAR, NULL, NULL, NULL);
    if (!sws) {
        snprintf(error, (size_t)error_cap, "cannot convert the picture's pixels");
        goto done;
    }
    set_matrix(sws, colour, rgb);
    long pixels = (long)width * (long)height;
    /* RGBA64LE is eight bytes a texel; the narrow form is four. libswscale's SIMD
     * converters write whole blocks of 16 pixels, so a last row narrower than that spills
     * up to 60 bytes past a tightly packed buffer: the tail is padding, not picture. */
    long converted = pixels * (deep ? 8 : 4);
    wide = malloc((size_t)converted + 128);
    if (!wide) {
        snprintf(error, (size_t)error_cap, "cannot allocate %ld bytes for the picture", converted);
        goto done;
    }
    uint8_t *planes[4] = {wide, NULL, NULL, NULL};
    int lines[4] = {(int)(width * (deep ? 8 : 4)), 0, 0, 0};
    if (lib.sws_scale(sws, (const uint8_t *const *)frame->data, frame->linesize, 0, height,
                      planes, lines) <= 0) {
        snprintf(error, (size_t)error_cap, "the colour conversion produced nothing");
        goto done;
    }

    long bytes = pixels * 4;
    unsigned char *pixels_out = wide;
    if (deep) {
        packed = malloc((size_t)bytes);
        if (!packed) {
            snprintf(error, (size_t)error_cap, "cannot allocate %ld bytes for the picture", bytes);
            goto done;
        }
        pack_ten_bit(wide, packed, pixels);
        pixels_out = packed;
    }
    info->width = width;
    info->height = height;
    info->storage = deep ? CS_STORAGE_PACKED10 : CS_STORAGE_RGBA;
    info->colour = colour;
    *out = pixels_out;
    *out_size = bytes;
    if (deep) {
        packed = NULL;
    } else {
        wide = NULL;
    }
    result = 0;

done:
    free(packed);
    free(wide);
    if (sws) lib.sws_free_context(sws);
    release_picture(&frame);
    if (packet) lib.packet_free(&packet);
    if (format) lib.close_input(&format);
    /* With a custom AVIOContext the format context does not close it. libavformat may have
     * replaced the buffer with one of its own, so the buffer in use is the one to free. */
    if (io) {
        lib.av_free(io->buffer);
        lib.io_context_free(&io);
    }
    if (io_buffer) lib.av_free(io_buffer);
    if (ctx) lib.free_context(&ctx);
    return result;
}

void cs_free(void *buffer) { free(buffer); }
