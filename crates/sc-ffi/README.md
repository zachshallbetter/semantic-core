# sc-ffi

C ABI over `sc-envelope` and `sc-core` for the Swift package. Header: `include/sc.h`. Built as a static library, a dynamic library and a Rust crate.

Implemented and tested (`cargo test -p sc-ffi`):

- Envelope: canonical bytes, canonical hash, validation pinned to `sc-event-0.1`, signing with a 32-byte seed, verification with a 32-byte public key. The envelope is passed as JSON text with any JSON body; canonical bytes and hashes also accept the Reflective Rust protocol identifier so the reference vector can be checked across the boundary, signing and verification do not.
- Store: open or create, add thing, add file, link, resolve a sighting, report gone, inspect, why, history, retract. Every call acts as the library owner.
- Conventions: integer codes, out-parameters, caller-freed strings via `sc_string_free`, `sc_last_error` per thread, panics caught and reported as `SC_ERR_INTERNAL`.

Not implemented: principals other than the owner, export and import, presentations, qualification, audience changes. Handles are not thread-safe.

Build the static library for the Swift package with `scripts/build-ffi.sh` from the repository root.
