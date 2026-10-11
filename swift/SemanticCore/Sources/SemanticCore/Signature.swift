import CSemanticCore
import Foundation

/// Ed25519 signatures over the domain-separated payload
/// `semantic-core:sc-event:ed25519:v1\0 || key_id || 0x00 || canonical`,
/// computed by the core.
public enum Signature {
    public static let domain: [UInt8] = Array("semantic-core:sc-event:ed25519:v1".utf8) + [0]

    /// Sign an `sc-event-0.1` envelope with a 32-byte seed; returns the
    /// authenticated envelope as JSON.
    public static func sign(envelopeJSON: String, keyID: String, seed: Data) throws -> String {
        let bytes = try thirtyTwo(seed, "seed")
        let json = try boundary(envelopeJSON, "envelope")
        let id = try boundary(keyID, "key id")
        return try call { out in
            bytes.withUnsafeBufferPointer { seedPointer in
                sc_envelope_sign(json, id, seedPointer.baseAddress, out)
            }
        }
    }

    /// Verify an authenticated envelope (JSON) under `keyID` with a 32-byte
    /// public key. Throws `.authentication` when the signature does not hold.
    public static func verify(signedJSON: String, keyID: String, publicKey: Data) throws {
        let bytes = try thirtyTwo(publicKey, "public key")
        let json = try boundary(signedJSON, "signed envelope")
        let id = try boundary(keyID, "key id")
        let code = bytes.withUnsafeBufferPointer { keyPointer in
            sc_envelope_verify(json, id, keyPointer.baseAddress)
        }
        try check(code)
    }

    /// The exact bytes that are signed, for an independent check.
    public static func payload(keyID: String, canonical: String) -> Data {
        var payload = Data(domain)
        payload.append(contentsOf: Array(keyID.utf8))
        payload.append(0)
        payload.append(contentsOf: Array(canonical.utf8))
        return payload
    }

    /// base64url without padding, as the envelope encodes signatures.
    public static func decodeBase64URL(_ text: String) -> Data? {
        var standard = text.replacingOccurrences(of: "-", with: "+").replacingOccurrences(of: "_", with: "/")
        while standard.count % 4 != 0 { standard.append("=") }
        return Data(base64Encoded: standard)
    }
}
