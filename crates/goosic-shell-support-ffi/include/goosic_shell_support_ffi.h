/*
 * The C ABI of goosic-shell-support-ffi, for shells that are not written in Rust.
 *
 * Kept by hand beside src/lib.rs, src/playback.rs and src/queue.rs; a test in that crate checks that every
 * exported function is declared here. Every function returning `char *` hands the caller a
 * NUL-terminated UTF-8 buffer that must be given back to goosic_string_free exactly once.
 */
#ifndef GOOSIC_SHELL_SUPPORT_FFI_H
#define GOOSIC_SHELL_SUPPORT_FFI_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

void goosic_string_free(char *value);

/* The official playback bridge. */
char *goosic_bridge_allowed_host(void);
char *goosic_bridge_handler_name(void);
int64_t goosic_bridge_event_version(void);
size_t goosic_bridge_max_body_bytes(void);
bool goosic_bridge_is_valid_video_id(const char *video_id);
char *goosic_bridge_observer_script(const char *token, unsigned long long generation,
                                    const char *video_id);
char *goosic_bridge_media_session_guard_script(void);
char *goosic_bridge_js_string_literal(const char *value);
char *goosic_bridge_validate_event(const char *body, const char *expected_token,
                                   unsigned long long expected_generation,
                                   const char *expected_video_id,
                                   unsigned long long last_sequence);

/* Account sign-in. */
bool goosic_login_is_allowed_url(const char *url);
bool goosic_login_is_completion_origin(const char *url);
char *goosic_login_make_result(const char *account_id, const char *profile_id,
                               const char *metadata, const char *page_url);

/* Playback rules asked on every sample, drag and frame. A NaN reads as "not known". */
double goosic_volume_gain(double position);
double goosic_volume_position(double gain);
char *goosic_time_text(double seconds);
bool goosic_track_has_finished(const char *state, double position, double duration,
                               bool listener_paused);
bool goosic_track_heard_playing(const char *state, double duration, bool is_advertisement);
bool goosic_is_same_song(const char *requested, const char *page);
/* 0 ignore, 1 refused, 2 adopt the page's video as the chosen song. */
int32_t goosic_early_switch(const char *requested_video_id, const char *page_video_id,
                            const char *page_title, bool may_adopt,
                            const char *loaded_video_id, const char *chosen_title);
bool goosic_seek_settle(double confirmed, double target, double elapsed_seconds, double *shown);

/* The shared listening queue: JSON commands in, {"ok": Outcome} or {"error": "..."} out. */
typedef struct GoosicQueue GoosicQueue;
GoosicQueue *goosic_queue_new(void);
void goosic_queue_free(GoosicQueue *queue);
char *goosic_queue_handle(GoosicQueue *queue, const char *command);

/* Stateless shell rules by name: JSON arguments in, {"ok": value} or {"error": "..."} out. */
char *goosic_rules_call(const char *name, const char *input);

#ifdef __cplusplus
}
#endif

#endif /* GOOSIC_SHELL_SUPPORT_FFI_H */
