/* sc-ffi: C ABI over sc-envelope and sc-core.
 *
 * Every function returns an int32_t code; SC_OK is 0. Results come back
 * through out-parameters. Every char * the library returns is owned by the
 * caller until sc_string_free. sc_last_error returns the message of the last
 * failing call on the current thread (caller frees) or NULL. No panic crosses
 * the boundary; a caught panic is SC_ERR_INTERNAL. Store handles are not
 * thread-safe: use one per thread and close it with sc_store_close.
 */
#ifndef SEMANTIC_CORE_SC_H
#define SEMANTIC_CORE_SC_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

enum {
    SC_OK = 0,
    SC_ERR_ARGUMENT = 1,       /* null pointer or text that is not UTF-8 */
    SC_ERR_VALIDATION = 2,     /* malformed JSON, envelope or vocabulary rule */
    SC_ERR_NOT_FOUND = 3,      /* absent or withheld; the two are not told apart */
    SC_ERR_UNAUTHORIZED = 4,
    SC_ERR_CONFLICT = 5,
    SC_ERR_INTEGRITY = 6,      /* event chain or replay failure */
    SC_ERR_STORAGE = 7,        /* SQLite */
    SC_ERR_AUTHENTICATION = 8, /* signature did not verify */
    SC_ERR_INTERNAL = 9
};

typedef struct ScStore ScStore;

/* Strings and diagnostics */
void sc_string_free(char *s);
char *sc_last_error(void);
const char *sc_version(void); /* static; do not free */

/* Envelope: the envelope is JSON text with any JSON body. */
int32_t sc_envelope_canonical(const char *envelope_json, char **out_canonical);
int32_t sc_envelope_hash(const char *envelope_json, char **out_hash);
int32_t sc_envelope_validate(const char *envelope_json);
int32_t sc_envelope_sign(const char *envelope_json, const char *key_id, const uint8_t *seed32, char **out_signed_json);
int32_t sc_envelope_verify(const char *signed_json, const char *key_id, const uint8_t *public_key32);

/* Store: every call acts as the library owner. JSON results mirror sc-core's
 * record types. */
int32_t sc_store_open(const char *path, const char *owner, ScStore **out_store);
void sc_store_close(ScStore *store);
int32_t sc_store_add_thing(ScStore *store, const char *label, char **out_id);
int32_t sc_store_add_file(ScStore *store, const char *path, const char *volume, const char *document, char **out_json);
int32_t sc_store_link(ScStore *store, const char *source_id, const char *predicate, const char *target_id, char **out_json);
int32_t sc_store_resolve(ScStore *store, const char *path, const char *volume, const char *document, char **out_json);
int32_t sc_store_gone(ScStore *store, const char *instance_id, char **out_json);
int32_t sc_store_inspect(ScStore *store, const char *id, char **out_json);
int32_t sc_store_why(ScStore *store, const char *relationship_id, char **out_json);
int32_t sc_store_history(ScStore *store, const char *id, char **out_json);
int32_t sc_store_retract(ScStore *store, const char *assertion_id, const char *reason, char **out_json);

#ifdef __cplusplus
}
#endif

#endif
