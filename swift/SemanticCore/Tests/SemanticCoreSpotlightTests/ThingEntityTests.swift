import Foundation
import XCTest
import SemanticCore
@testable import SemanticCoreSpotlight

#if canImport(AppIntents) && canImport(CoreSpotlight)
import CoreSpotlight

/// The non-disclosure proof: an indexed entity carries only what its viewer
/// may see. Built through the FFI store surface.
final class ThingEntityTests: XCTestCase {
    struct Scenario {
        let store: Store
        let thing: String
        let manualArtifact: String
        let taxArtifact: String
        let directory: URL
    }

    /// A shared dishwasher with a shared manual and an owner-only tax return,
    /// both linked to it. Labels are chosen so a leak is a plain substring.
    func scenario() throws -> Scenario {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("semantic-core-spotlight-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let manual = directory.appendingPathComponent("Dishwasher manual.txt")
        let tax = directory.appendingPathComponent("Tax return 2025.txt")
        try Data("manual".utf8).write(to: manual)
        try Data("tax".utf8).write(to: tax)
        let store = try Store(path: directory.appendingPathComponent("library.sqlite").path, owner: "ava")
        let thing = try store.addThing(label: "Dishwasher")
        try store.setAudience(thing, audiences: ["adults", "kids"])
        let manualFile = try store.addFile(path: manual.path, volume: "vol-robin", document: "doc-manual")
        try store.setAudience(manualFile.artifactID, audiences: ["adults", "kids"])
        let taxFile = try store.addFile(path: tax.path, volume: "vol-robin", document: "doc-tax")
        // The tax return stays owner-only.
        _ = try store.link(source: manualFile.artifactID, predicate: "DOCUMENTS", target: thing)
        _ = try store.link(source: taxFile.artifactID, predicate: "REFERENCES", target: thing)
        return Scenario(store: store, thing: thing, manualArtifact: manualFile.artifactID, taxArtifact: taxFile.artifactID, directory: directory)
    }

    func testOwnerSeesTheThingAndItsRelationships() throws {
        guard #available(macOS 15, *) else { throw XCTSkip("IndexedEntity needs macOS 15") }
        let s = try scenario()
        defer { try? FileManager.default.removeItem(at: s.directory) }
        let entity = try XCTUnwrap(ThingEntity.load(s.thing, from: s.store, as: .init(actor: "ava")))
        XCTAssertEqual(entity.id, s.thing)
        XCTAssertEqual(entity.title, "Dishwasher")
        XCTAssertEqual(Set(entity.relationships.map(\.otherID)), [s.manualArtifact, s.taxArtifact])
        XCTAssertEqual(Set(entity.keywords), ["DOCUMENTED_BY", "REFERENCED_BY"])
        let attributes = entity.attributeSet
        XCTAssertEqual(attributes.title, "Dishwasher")
        XCTAssertEqual(attributes.displayName, "Dishwasher")
        XCTAssertEqual(attributes.identifier, s.thing)
        XCTAssertEqual(Set(attributes.keywords ?? []), ["DOCUMENTED_BY", "REFERENCED_BY"])
        XCTAssertEqual(attributes.contentType, ThingEntity.contentType.identifier)
        let description = try XCTUnwrap(attributes.contentDescription)
        XCTAssertTrue(description.contains(s.manualArtifact))
        XCTAssertTrue(description.contains(s.taxArtifact))
        let archived = String(decoding: try entity.archivedAttributeSet(), as: UTF8.self)
        XCTAssertTrue(archived.contains(s.taxArtifact))
    }

    func testRestrictedViewerGetsNothingFromTheHiddenArtifact() throws {
        guard #available(macOS 15, *) else { throw XCTSkip("IndexedEntity needs macOS 15") }
        let s = try scenario()
        defer { try? FileManager.default.removeItem(at: s.directory) }
        let leo = Store.Principal(actor: "leo", audiences: ["kids"])
        // Leo cannot see the tax return at all.
        XCTAssertThrowsError(try s.store.inspect(s.taxArtifact, as: leo)) { error in
            XCTAssertEqual((error as? SemanticCoreError)?.code, .notFound)
        }
        let entity = try XCTUnwrap(ThingEntity.load(s.thing, from: s.store, as: leo))
        XCTAssertEqual(entity.title, "Dishwasher")
        XCTAssertEqual(entity.relationships.map(\.otherID), [s.manualArtifact])
        XCTAssertEqual(entity.keywords, ["DOCUMENTED_BY"])
        let attributes = entity.attributeSet
        XCTAssertEqual(attributes.keywords, ["DOCUMENTED_BY"])
        XCTAssertEqual(attributes.relatedUniqueIdentifier, s.manualArtifact)
        XCTAssertEqual(attributes.contentDescription, "DOCUMENTED_BY \(s.manualArtifact)")

        // Nothing from the hidden artifact reaches the serialized attribute
        // set: not its identifier, label, path, predicate, nor a count that
        // would betray it.
        let archived = String(decoding: try entity.archivedAttributeSet(), as: UTF8.self)
        for hidden in [s.taxArtifact, "Tax return", "Tax return 2025", "REFERENCED_BY", "REFERENCES", "doc-tax"] {
            XCTAssertFalse(archived.contains(hidden), "hidden value \(hidden) reached the attribute set")
        }
        XCTAssertTrue(archived.contains(s.manualArtifact))
        XCTAssertTrue(archived.contains("Dishwasher"))
        XCTAssertFalse(archived.contains("adults"), "audience labels are owner-level data")
        XCTAssertFalse(archived.contains("kids"))
        XCTAssertFalse(archived.contains("ava"))
        XCTAssertEqual(entity.relationships.count, 1)
        // The raw viewer inspect itself carries neither the owner name nor
        // any audience label; the owner's view still does.
        let raw = try s.store.inspectJSON(s.thing, as: leo)
        for withheld in ["\"ava\"", "adults", "kids", "\"owner\""] {
            XCTAssertFalse(raw.contains(withheld), "viewer inspect carries \(withheld)")
        }
        let ownerRaw = try s.store.inspectJSON(s.thing)
        XCTAssertTrue(ownerRaw.contains("\"owner\":\"ava\""))
        XCTAssertTrue(ownerRaw.contains("adults"))
        // The owner's view of the same thing is wider, so the difference is
        // the projection, not the data.
        let owner = try XCTUnwrap(ThingEntity.load(s.thing, from: s.store, as: .init(actor: "ava")))
        XCTAssertEqual(owner.relationships.count, 2)
    }

    func testAThingTheViewerCannotSeeProducesNoEntity() throws {
        guard #available(macOS 15, *) else { throw XCTSkip("IndexedEntity needs macOS 15") }
        let s = try scenario()
        defer { try? FileManager.default.removeItem(at: s.directory) }
        let hale = Store.Principal(actor: "hale")
        XCTAssertNil(try ThingEntity.load(s.thing, from: s.store, as: hale))
        // An absent identifier reads the same way.
        XCTAssertNil(try ThingEntity.load("01J00000000000000000000000", from: s.store, as: hale))
        XCTAssertNil(try ThingEntity.load("01J00000000000000000000000", from: s.store, as: .init(actor: "ava")))
        // An artifact is not a thing, so it is not an entity either.
        XCTAssertNil(try ThingEntity.load(s.manualArtifact, from: s.store, as: .init(actor: "ava")))
    }

    func testQueryResolvesThroughTheInstalledProvider() async throws {
        guard #available(macOS 15, *) else { throw XCTSkip("IndexedEntity needs macOS 15") }
        let s = try scenario()
        defer { try? FileManager.default.removeItem(at: s.directory) }
        let leo = Store.Principal(actor: "leo", audiences: ["kids"])
        let visible = try XCTUnwrap(ThingEntity.load(s.thing, from: s.store, as: leo))
        let query = ThingQuery()
        let none = try await query.entities(for: [s.thing])
        XCTAssertTrue(none.isEmpty, "no provider, no entities")
        let title = visible.title
        let id = visible.id
        await MainActor.run {
            ThingQuery.provider = { requested in
                requested == id ? ThingEntity(id: id, title: title, relationships: []) : nil
            }
        }
        let found = try await query.entities(for: [s.thing, "other"])
        XCTAssertEqual(found.map(\.id), [s.thing])
        await MainActor.run { ThingQuery.provider = nil }
    }
}
#endif
