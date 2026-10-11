import Foundation
import XCTest
@testable import SemanticCore

/// The Stage 1 demonstration through the Swift wrapper: link two files, move
/// one, resolve, inspect, retract, inspect the history.
final class DemonstrationTests: XCTestCase {
    func testStageOneDemonstration() throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("semantic-core-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }

        let warranty = directory.appendingPathComponent("Warranty.txt")
        let manual = directory.appendingPathComponent("Manual.txt")
        try Data("warranty for the dishwasher".utf8).write(to: warranty)
        try Data("manual for the dishwasher".utf8).write(to: manual)

        let store = try Store(path: directory.appendingPathComponent("library.sqlite").path, owner: "ava")
        XCTAssertFalse(Core.version.isEmpty)

        let thing = try store.addThing(label: "Dishwasher")
        let warrantyFile = try store.addFile(path: warranty.path, volume: "vol-robin", document: "doc-warranty")
        let manualFile = try store.addFile(path: manual.path, volume: "vol-robin", document: "doc-manual")
        XCTAssertNotEqual(warrantyFile.artifactID, manualFile.artifactID)

        let link = try store.link(source: warrantyFile.artifactID, predicate: "DOCUMENTS", target: thing)
        XCTAssertEqual(link.state, "PRESENT")
        _ = try store.link(source: manualFile.artifactID, predicate: "REFERENCES", target: warrantyFile.artifactID)

        // Move one file within the volume: same document id, new path.
        let moved = directory.appendingPathComponent("Dishwasher warranty.txt")
        try FileManager.default.moveItem(at: warranty, to: moved)
        let resolution = try store.resolve(path: moved.path, volume: "vol-robin", document: "doc-warranty")
        XCTAssertEqual(resolution.outcome, "RESOLVED")
        XCTAssertEqual(resolution.rule, "within-volume-key")
        XCTAssertEqual(resolution.instanceID, warrantyFile.instanceID)

        // Inspect: the relationship survived; the instance has the new path.
        let relationship = try store.inspect(link.relationshipID)
        XCTAssertEqual(relationship["kind"]?.string, "relationship")
        XCTAssertEqual(relationship["state"]?.string, "PRESENT")
        let instance = try store.inspect(warrantyFile.instanceID)
        XCTAssertEqual(instance["instance"]?["location"]?["path"]?.string, moved.path)
        XCTAssertEqual(try store.inspect(thing)["relationships"]?.array?.count, 1)

        // Cross-volume move: gone here, seen there with the same content.
        let gone = try store.gone(instance: warrantyFile.instanceID)
        XCTAssertEqual(gone.outcome, "UNRESOLVED")
        XCTAssertEqual(gone.rule, "known-location-gone")
        let elsewhere = directory.appendingPathComponent("Leo")
        try FileManager.default.createDirectory(at: elsewhere, withIntermediateDirectories: true)
        let archived = elsewhere.appendingPathComponent("Warranty.txt")
        try FileManager.default.moveItem(at: moved, to: archived)
        let crossVolume = try store.resolve(path: archived.path, volume: "vol-leo", document: "doc-9")
        XCTAssertEqual(crossVolume.outcome, "RESOLVED")
        XCTAssertEqual(crossVolume.rule, "content-hash-single-candidate-previous-gone")

        // The rules that fired are on record against the instance, and `why`
        // for the relationship lists the assertion with its evidence.
        let moves = try store.inspect(warrantyFile.instanceID)
        let rules = moves["resolutions"]?.array?.compactMap { $0["rule"]?.string } ?? []
        XCTAssertEqual(rules, ["within-volume-key", "known-location-gone", "content-hash-single-candidate-previous-gone"])
        let why = try store.why(relationship: link.relationshipID)
        XCTAssertEqual(why["state"]?.string, "PRESENT")
        XCTAssertEqual(why["assertions"]?.array?.count, 1)

        // Retract: a compensating event; the history keeps everything.
        let retracted = try store.retract(assertion: link.assertionID, reason: "wrong appliance")
        XCTAssertEqual(retracted.state, "UNKNOWN")
        XCTAssertTrue(retracted.compensates?.hasPrefix("sha256:") == true)
        let history = try store.history(link.relationshipID)
        let ops = history.compactMap { $0["envelope"]?["body"]?["change"]?["op"]?.string }
        XCTAssertEqual(ops, ["relationship-declared", "assertion-proposed", "assertion-accepted", "assertion-retracted"])
        XCTAssertThrowsError(try store.retract(assertion: link.assertionID)) { error in
            XCTAssertEqual((error as? SemanticCoreError)?.code, .conflict)
        }

        // Errors are codes with messages, never crashes.
        XCTAssertThrowsError(try store.inspect("not-an-id")) { error in
            XCTAssertEqual((error as? SemanticCoreError)?.code, .notFound)
            XCTAssertTrue((error as? SemanticCoreError)?.message.contains("not found") == true)
        }
        XCTAssertThrowsError(try store.link(source: thing, predicate: "DOCUMENTS", target: warrantyFile.artifactID)) { error in
            XCTAssertEqual((error as? SemanticCoreError)?.code, .validation)
        }
        store.close()
        XCTAssertThrowsError(try store.inspect(thing)) { error in
            XCTAssertEqual((error as? SemanticCoreError)?.code, .argument)
        }
    }
}
