import CSemanticCore
import Foundation

/// Envelope operations through the core. The envelope is JSON text with any
/// JSON body; the envelope's own fields reject unknown members.
public enum Envelope {
    /// RFC 8785 canonical bytes, as the core computes them. Structural
    /// validation only, so the Reflective Rust reference vector canonicalizes.
    public static func canonicalJSON(of envelopeJSON: String) throws -> String {
        let json = try boundary(envelopeJSON, "envelope")
        return try call { out in sc_envelope_canonical(json, out) }
    }

    /// `sha256:<hex>` of the canonical bytes.
    public static func hash(of envelopeJSON: String) throws -> String {
        let json = try boundary(envelopeJSON, "envelope")
        return try call { out in sc_envelope_hash(json, out) }
    }

    /// Full validation, pinned to `sc-event-0.1`.
    public static func validate(_ envelopeJSON: String) throws {
        try check(sc_envelope_validate(try boundary(envelopeJSON, "envelope")))
    }
}
