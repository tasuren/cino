//! Canonical CBOR Encoding and Decoding Format
//!
//! ## Background
//! Data exchange is fixed to CBOR to guarantee interoperability.
//!
//! ## Scope
//! - `event` / `query` inputs use canonical CBOR decoding.
//! - `action` / `result` outputs are encoded with canonical CBOR.
//!
//! ## Canonicalization Rules
//! This crate applies strictly canonical CBOR formatting specified by RFC 8949 (Core Deterministic Encoding Requirements):
//! 1. Maps are sorted strictly in byte-wise lexicographic order.
//! 2. Unnecessary lengths and indefinite-length sequences are forbidden.
//! 3. Integers are represented in their shortest lengths.
//!
//! ## Compatibility Policy
//! - Currently, the encoder produces only valid canonical CBOR maps arrays.
//! - Invalid or unrecognized schema tags will return `CodecError::InvalidFormat`.
//! - Versioning will be performed via enveloping around these types or through explicit tag declarations. At the moment, unknown maps or keys without `$tag`/`$tuple` keys are mapped directly to `VmValue::Map`.
