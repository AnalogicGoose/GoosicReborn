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
int32_t goosic_early_switch_counted(const char *requested_video_id, const char *page_video_id,
                                    const char *page_title, uint32_t adoptions,
                                    const char *loaded_video_id, const char *chosen_title);
bool goosic_seek_settle(double confirmed, double target, double elapsed_seconds, double *shown);

/* Page scripts and limits a host injects or obeys. */
char *goosic_bridge_active_media_element_script(void);
char *goosic_bridge_automix_off_script(void);
uint32_t goosic_bridge_automix_attempts(void);
char *goosic_bridge_safari_user_agent_suffix(void);
char *goosic_login_completion_script(void);
double goosic_login_completion_timeout_seconds(void);
bool goosic_login_is_google_sign_in_host(const char *host);

/* Artwork, the playing cover's shape and an artist page's hero. */
bool goosic_artwork_is_allowed(const char *url);
char *goosic_artwork_cache_key(const char *remote);
uint64_t goosic_artwork_max_bytes(void);
uint32_t goosic_artwork_max_concurrent_fetches(void);
double goosic_cover_aspect(double pixel_width, double pixel_height);
void goosic_cover_fit(double aspect, double max_width, double max_height, double *width,
                      double *height);
double goosic_hero_height(double width);
bool goosic_hero_tint(const uint8_t *rgba, size_t length, size_t width, size_t height,
                      uint8_t *rgb);

/* Resuming content an advertisement left paused. Zero one per load. */
typedef struct GoosicAdRecovery {
    bool awaiting_content;
    bool resume_requested;
} GoosicAdRecovery;
bool goosic_ad_recovery_observe(GoosicAdRecovery *recovery, bool advertisement,
                                const char *state, bool listener_paused);

/* When a play's natural end moves the queue. Zero one, or call begin, for every load. */
typedef struct GoosicEndGuard {
    bool heard;
    bool handled;
    bool awaiting_content;
    bool resume_requested;
} GoosicEndGuard;
void goosic_end_guard_begin(GoosicEndGuard *guard);
bool goosic_end_guard_observe(GoosicEndGuard *guard, bool is_current, bool advertisement,
                              const char *state, double position, double duration,
                              bool listener_paused, bool *resume);
bool goosic_end_guard_page_ended(GoosicEndGuard *guard, bool is_current);

/* The system media controls, projected from confirmed state. */
typedef struct GoosicMediaInput {
    bool has_track;
    bool is_ready;
    bool has_owner;
    double current_time;
    double duration;
    bool is_paused;
    bool is_advertisement;
    bool has_queue;
    bool is_idle;
} GoosicMediaInput;
/* playback_state: 0 stopped, 1 paused, 2 playing. */
typedef struct GoosicMediaProjection {
    bool is_active;
    uint8_t playback_state;
    double elapsed_time;
    double duration;
    double playback_rate;
    bool play;
    bool pause;
    bool toggle_play_pause;
    bool next;
    bool previous;
    bool change_position;
    bool stop;
    bool change_volume;
} GoosicMediaProjection;
void goosic_media_project(const GoosicMediaInput *input, GoosicMediaProjection *output);

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
