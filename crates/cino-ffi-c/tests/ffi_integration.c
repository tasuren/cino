#include <stddef.h>
#include <stdint.h>

typedef enum {
  CINO_STATUS_OK = 0,
  CINO_STATUS_ERR = 1,
} cino_status_t;

typedef struct cino_program_t cino_program_t;
typedef struct cino_state_t cino_state_t;
typedef struct cino_value_t cino_value_t;
typedef struct cino_actions_t cino_actions_t;
typedef struct cino_error_t cino_error_t;

extern cino_status_t cino_program_new_mock_counter(cino_program_t **out_program, cino_error_t **out_error);
extern void cino_program_destroy(cino_program_t *program);
extern cino_status_t cino_state_new(const cino_program_t *program, const cino_value_t *initial_value, cino_state_t **out_state, cino_error_t **out_error);
extern void cino_state_destroy(cino_state_t *state);
extern cino_status_t cino_update(const cino_program_t *program, const cino_state_t *state, const cino_value_t *event, cino_state_t **out_next_state, cino_actions_t **out_actions, cino_error_t **out_error);
extern cino_status_t cino_query(const cino_program_t *program, const cino_state_t *state, const cino_value_t *query, cino_value_t **out_result, cino_error_t **out_error);
extern cino_status_t cino_value_new_from_cbor(const uint8_t *data, size_t len, cino_value_t **out_value, cino_error_t **out_error);
extern void cino_value_destroy(cino_value_t *value);
extern cino_status_t cino_value_bytes(const cino_value_t *value, const uint8_t **out_ptr, size_t *out_len, cino_error_t **out_error);
extern void cino_actions_destroy(cino_actions_t *actions);
extern cino_status_t cino_actions_bytes(const cino_actions_t *actions, const uint8_t **out_ptr, size_t *out_len, cino_error_t **out_error);
extern void cino_error_destroy(cino_error_t *error);

int cino_ffi_c_integration_test(void) {
  cino_program_t *program = NULL;
  cino_state_t *state = NULL;
  cino_state_t *next_state = NULL;
  cino_value_t *initial = NULL;
  cino_value_t *event = NULL;
  cino_value_t *query = NULL;
  cino_value_t *result = NULL;
  cino_actions_t *actions = NULL;
  cino_error_t *error = NULL;

  uint8_t initial_cbor[] = {0x0a};
  uint8_t event_cbor[] = {0x07};
  uint8_t query_cbor[] = {0xf5};

  if (cino_program_new_mock_counter(&program, &error) != CINO_STATUS_OK) {
    goto fail;
  }

  if (cino_value_new_from_cbor(initial_cbor, sizeof(initial_cbor), &initial, &error) != CINO_STATUS_OK) {
    goto fail;
  }

  if (cino_state_new(program, initial, &state, &error) != CINO_STATUS_OK) {
    goto fail;
  }

  if (cino_value_new_from_cbor(event_cbor, sizeof(event_cbor), &event, &error) != CINO_STATUS_OK) {
    goto fail;
  }

  if (cino_update(program, state, event, &next_state, &actions, &error) != CINO_STATUS_OK) {
    goto fail;
  }

  if (cino_value_new_from_cbor(query_cbor, sizeof(query_cbor), &query, &error) != CINO_STATUS_OK) {
    goto fail;
  }

  if (cino_query(program, next_state, query, &result, &error) != CINO_STATUS_OK) {
    goto fail;
  }

  const uint8_t *result_ptr = NULL;
  size_t result_len = 0;
  if (cino_value_bytes(result, &result_ptr, &result_len, &error) != CINO_STATUS_OK) {
    goto fail;
  }
  if (result_len != 1 || result_ptr[0] != 0x11) {
    goto fail;
  }

  const uint8_t *actions_ptr = NULL;
  size_t actions_len = 0;
  if (cino_actions_bytes(actions, &actions_ptr, &actions_len, &error) != CINO_STATUS_OK) {
    goto fail;
  }
  if (actions_len != 2 || actions_ptr[0] != 0x81 || actions_ptr[1] != 0x07) {
    goto fail;
  }

  cino_value_destroy(result);
  cino_value_destroy(query);
  cino_actions_destroy(actions);
  cino_state_destroy(next_state);
  cino_value_destroy(event);
  cino_state_destroy(state);
  cino_value_destroy(initial);
  cino_program_destroy(program);
  return 0;

fail:
  cino_value_destroy(result);
  cino_value_destroy(query);
  cino_actions_destroy(actions);
  cino_state_destroy(next_state);
  cino_value_destroy(event);
  cino_state_destroy(state);
  cino_value_destroy(initial);
  cino_program_destroy(program);
  cino_error_destroy(error);
  return 1;
}
