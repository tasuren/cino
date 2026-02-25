unsafe extern "C" {
    fn cino_ffi_c_integration_test() -> i32;
}

#[test]
fn c_integration_can_call_update_and_query() {
    // SAFETY: this function is defined in tests/ffi_integration.c and has no parameters.
    let status = unsafe { cino_ffi_c_integration_test() };
    assert_eq!(status, 0);
}
