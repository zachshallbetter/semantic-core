import CryptoKit
import Foundation
import XCTest
@testable import SemanticCore

/// The golden-vector proof both ways: through the FFI, and independently in
/// Swift with the JCS canonicalizer, SHA-256 and CryptoKit's Curve25519.
final class GoldenVectorTests: XCTestCase {
    static let expectedReferenceHash = "sha256:7c98ac356aa59dd29e466c6f6e5927eb5afd2ad85f283d59d920de3f7bf05a18"

    struct VectorFile: Decodable {
        struct SignatureVector: Decodable {
            let key_id: String
            let seed_hex: String
            let verifying_key_hex: String
            let signature: String
        }
        let description: String
        let envelope: JSONObject
        let canonical_json: String
        let canonical_hash: String
        let signature: SignatureVector?
        let expect: String?
    }

    static var vectorsDirectory: URL {
        // Tests/SemanticCoreTests/<file> -> repository root is four levels up from the package.
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()  // SemanticCoreTests
            .deletingLastPathComponent()  // Tests
            .deletingLastPathComponent()  // SemanticCore
            .deletingLastPathComponent()  // swift
            .deletingLastPathComponent()  // repository root
            .appendingPathComponent("fixtures/vectors")
    }

    /// The vector file and the raw JSON text of its `envelope` member, so the
    /// bytes handed to both paths are the file's own, not a re-serialization.
    func load(_ name: String) throws -> (VectorFile, String) {
        let url = Self.vectorsDirectory.appendingPathComponent(name)
        let data = try Data(contentsOf: url)
        let file = try JSONDecoder().decode(VectorFile.self, from: data)
        let whole = try JCS.parse(String(decoding: data, as: UTF8.self))
        guard case .object(let members) = whole, let envelope = members.first(where: { $0.0 == "envelope" })?.1 else {
            XCTFail("\(name) has no envelope")
            throw SemanticCoreError(code: .argument, rawCode: 1, message: "no envelope")
        }
        // Hand the FFI a non-canonical rendering (members in file order, no sorting).
        return (file, render(envelope))
    }

    /// JSON text in file order, unsorted, so the core does the canonical work.
    private func render(_ value: JCS.Value) -> String {
        switch value {
        case .null: return "null"
        case .bool(let b): return b ? "true" : "false"
        case .integer(let i): return String(i)
        case .string(let s): return JCS.canonicalize(.string(s))
        case .array(let items): return "[" + items.map(render).joined(separator: ",") + "]"
        case .object(let members):
            return "{" + members.map { JCS.canonicalize(.string($0.0)) + ":" + render($0.1) }.joined(separator: ",") + "}"
        }
    }

    // MARK: Through the FFI

    func testReferenceVectorHashesAcrossTheBoundary() throws {
        let (file, envelopeJSON) = try load("rr-rsp-0.1-empty-type.json")
        XCTAssertEqual(try Envelope.canonicalJSON(of: envelopeJSON), file.canonical_json)
        XCTAssertEqual(try Envelope.hash(of: envelopeJSON), Self.expectedReferenceHash)
        XCTAssertEqual(file.canonical_hash, Self.expectedReferenceHash)
        // The reference protocol canonicalizes but is not an sc-event.
        XCTAssertThrowsError(try Envelope.validate(envelopeJSON)) { error in
            XCTAssertEqual((error as? SemanticCoreError)?.code, .validation)
        }
    }

    func testEventVectorsHashAndVerifyAcrossTheBoundary() throws {
        for name in ["sc-event-0.1-declared.json", "sc-event-0.1-observed.json", "sc-event-0.1-derived-signed.json"] {
            let (file, envelopeJSON) = try load(name)
            XCTAssertEqual(try Envelope.canonicalJSON(of: envelopeJSON), file.canonical_json, name)
            XCTAssertEqual(try Envelope.hash(of: envelopeJSON), file.canonical_hash, name)
            XCTAssertNoThrow(try Envelope.validate(envelopeJSON), name)
        }
        let (signed, envelopeJSON) = try load("sc-event-0.1-derived-signed.json")
        let vector = try XCTUnwrap(signed.signature)
        // Re-sign with the published seed: the same signature bytes come out.
        let seed = try XCTUnwrap(Data(hex: vector.seed_hex))
        let signedJSON = try Signature.sign(envelopeJSON: envelopeJSON, keyID: vector.key_id, seed: seed)
        let produced = try JSONDecoder().decode(JSONObject.self, from: Data(signedJSON.utf8))
        XCTAssertEqual(produced["authentication"]?["signature"]?.string, vector.signature)
        let publicKey = try XCTUnwrap(Data(hex: vector.verifying_key_hex))
        XCTAssertNoThrow(try Signature.verify(signedJSON: signedJSON, keyID: vector.key_id, publicKey: publicKey))
        // The file's own signature verifies too.
        let authenticated = authenticatedJSON(envelopeJSON: envelopeJSON, keyID: vector.key_id, signature: vector.signature)
        XCTAssertNoThrow(try Signature.verify(signedJSON: authenticated, keyID: vector.key_id, publicKey: publicKey))
    }

    func testTamperedVectorFailsAcrossTheBoundary() throws {
        let (file, envelopeJSON) = try load("sc-event-0.1-tampered.json")
        XCTAssertEqual(file.expect, "invalid-signature")
        let vector = try XCTUnwrap(file.signature)
        let publicKey = try XCTUnwrap(Data(hex: vector.verifying_key_hex))
        let authenticated = authenticatedJSON(envelopeJSON: envelopeJSON, keyID: vector.key_id, signature: vector.signature)
        XCTAssertThrowsError(try Signature.verify(signedJSON: authenticated, keyID: vector.key_id, publicKey: publicKey)) { error in
            XCTAssertEqual((error as? SemanticCoreError)?.code, .authentication)
        }
        // A rotated key does not verify the original signature either.
        let (good, goodJSON) = try load("sc-event-0.1-derived-signed.json")
        let rotated = try load("sc-event-0.1-rotated-key.json").0
        let goodVector = try XCTUnwrap(good.signature)
        let rotatedVector = try XCTUnwrap(rotated.signature)
        let rotatedKey = try XCTUnwrap(Data(hex: rotatedVector.verifying_key_hex))
        let stale = authenticatedJSON(envelopeJSON: goodJSON, keyID: rotatedVector.key_id, signature: goodVector.signature)
        XCTAssertThrowsError(try Signature.verify(signedJSON: stale, keyID: rotatedVector.key_id, publicKey: rotatedKey))
    }

    private func authenticatedJSON(envelopeJSON: String, keyID: String, signature: String) -> String {
        "{\"envelope\":\(envelopeJSON),\"authentication\":{\"algorithm\":\"ed25519\",\"key_id\":\(JCS.canonicalize(.string(keyID))),\"signature\":\(JCS.canonicalize(.string(signature)))}}"
    }

    // MARK: Independently in Swift

    func testReferenceVectorReproducesInPureSwift() throws {
        let (file, envelopeJSON) = try load("rr-rsp-0.1-empty-type.json")
        let canonical = try JCS.canonicalize(envelopeJSON)
        XCTAssertEqual(canonical, file.canonical_json)
        XCTAssertEqual(Hash.sha256(canonical), Self.expectedReferenceHash)
    }

    func testEventVectorsReproduceInPureSwiftAndCryptoKitVerifies() throws {
        for name in ["sc-event-0.1-declared.json", "sc-event-0.1-observed.json", "sc-event-0.1-derived-signed.json"] {
            let (file, envelopeJSON) = try load(name)
            let canonical = try JCS.canonicalize(envelopeJSON)
            XCTAssertEqual(canonical, file.canonical_json, name)
            XCTAssertEqual(Hash.sha256(canonical), file.canonical_hash, name)
        }
        let (file, envelopeJSON) = try load("sc-event-0.1-derived-signed.json")
        let vector = try XCTUnwrap(file.signature)
        let canonical = try JCS.canonicalize(envelopeJSON)
        let payload = Signature.payload(keyID: vector.key_id, canonical: canonical)
        let publicKey = try Curve25519.Signing.PublicKey(rawRepresentation: try XCTUnwrap(Data(hex: vector.verifying_key_hex)))
        let signature = try XCTUnwrap(Signature.decodeBase64URL(vector.signature))
        XCTAssertTrue(publicKey.isValidSignature(signature, for: payload))
        // The private key from the published seed signs to the same bytes.
        let privateKey = try Curve25519.Signing.PrivateKey(rawRepresentation: try XCTUnwrap(Data(hex: vector.seed_hex)))
        XCTAssertEqual(privateKey.publicKey.rawRepresentation.hexString, vector.verifying_key_hex)
        XCTAssertTrue(privateKey.publicKey.isValidSignature(try privateKey.signature(for: payload), for: payload))
        // Without the domain, or with another key id, the signature does not hold.
        XCTAssertFalse(publicKey.isValidSignature(signature, for: Data(canonical.utf8)))
        XCTAssertFalse(publicKey.isValidSignature(signature, for: Signature.payload(keyID: "other", canonical: canonical)))
    }

    func testTamperedVectorFailsInPureSwift() throws {
        let (file, envelopeJSON) = try load("sc-event-0.1-tampered.json")
        let vector = try XCTUnwrap(file.signature)
        let canonical = try JCS.canonicalize(envelopeJSON)
        XCTAssertEqual(Hash.sha256(canonical), file.canonical_hash)
        let payload = Signature.payload(keyID: vector.key_id, canonical: canonical)
        let publicKey = try Curve25519.Signing.PublicKey(rawRepresentation: try XCTUnwrap(Data(hex: vector.verifying_key_hex)))
        let signature = try XCTUnwrap(Signature.decodeBase64URL(vector.signature))
        XCTAssertFalse(publicKey.isValidSignature(signature, for: payload))
    }

    func testCanonicalizerRules() throws {
        // Key order by UTF-16 code units, escapes per RFC 8785, integers bounded.
        XCTAssertEqual(try JCS.canonicalize(#"{"b":1, "a":[true,null,"xA\n\u001f"], "é":2, "Z":3}"#),
                       #"{"Z":3,"a":[true,null,"xA\n\u001f"],"b":1,"é":2}"#)
        XCTAssertEqual(try JCS.canonicalize(" { \"n\" : -9007199254740992 } "), #"{"n":-9007199254740992}"#)
        XCTAssertThrowsError(try JCS.canonicalize(#"{"n":9007199254740993}"#))
        XCTAssertThrowsError(try JCS.canonicalize(#"{"n":1.5}"#))
        XCTAssertThrowsError(try JCS.canonicalize(#"{"a":1,"a":2}"#))
        // Surrogate pairs decode to one scalar and are emitted raw.
        XCTAssertEqual(try JCS.canonicalize(#""😀""#), "\"\u{1F600}\"")
    }
}
