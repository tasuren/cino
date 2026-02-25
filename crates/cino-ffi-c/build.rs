fn main() {
    cc::Build::new()
        .file("tests/ffi_integration.c")
        .compile("cino_ffi_integration");
}
