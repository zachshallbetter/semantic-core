import Foundation
import SemanticCore

#if canImport(AppIntents) && canImport(CoreSpotlight)
import AppIntents
import CoreSpotlight
import UniformTypeIdentifiers

/// A Thing as an App Intents entity that Spotlight can index.
///
/// The entity is built only from a viewer-projected `inspect` result: the
/// store applies its access rule for the named principal before anything is
/// read, so the attribute set can only contain what that viewer may see.
/// Nothing here reads owner-level data, audiences or hidden edges.
///
/// Registration with Spotlight (`CSSearchableIndex`) needs an app bundle and
/// is not exercised; the attribute set is the subject of the spike.
@available(macOS 15, *)
public struct ThingEntity: AppEntity, IndexedEntity, Sendable {
    public static let typeDisplayRepresentation = TypeDisplayRepresentation(name: "Thing")
    public static let defaultQuery = ThingQuery()

    /// The uniform type reported to Spotlight for a thing.
    public static let contentType = UTType("com.semantic-core.thing") ?? .item

    /// A relationship as the viewer sees it.
    public struct Relationship: Sendable, Equatable {
        public let label: String
        public let otherID: String
        public let state: String
    }

    public let id: String
    public let title: String
    /// Only relationships whose materialized state is PRESENT for the viewer.
    public let relationships: [Relationship]

    public var displayRepresentation: DisplayRepresentation {
        DisplayRepresentation(title: "\(title)")
    }

    /// Keywords: the registered predicate labels among the visible, present
    /// relationships. Extension predicates (`x-…`) are not keywords.
    public var keywords: [String] {
        var seen: [String] = []
        for relationship in relationships where !relationship.label.hasPrefix("x-") {
            if !seen.contains(relationship.label) { seen.append(relationship.label) }
        }
        return seen
    }

    public var attributeSet: CSSearchableItemAttributeSet {
        let attributes = CSSearchableItemAttributeSet(contentType: Self.contentType)
        attributes.title = title
        attributes.displayName = title
        attributes.identifier = id
        attributes.keywords = keywords
        attributes.contentDescription = relationships
            .map { "\($0.label) \($0.otherID)" }
            .joined(separator: "\n")
        // One related identifier is all the attribute set carries; the rest
        // are in the description. Both come from the viewer's projection.
        attributes.relatedUniqueIdentifier = relationships.first?.otherID
        return attributes
    }

    /// Load a thing as `principal` sees it. Returns nil when the store
    /// withholds it (which reads exactly like absence) or when the identifier
    /// is not a thing.
    public static func load(_ id: String, from store: Store, as principal: Store.Principal) throws -> ThingEntity? {
        let inspection: JSONObject
        do {
            inspection = try store.inspect(id, as: principal)
        } catch let error as SemanticCoreError where error.code == .notFound {
            return nil
        }
        guard inspection["kind"]?.string == "thing",
              let thingID = inspection["thing"]?["id"]?.string,
              let label = inspection["thing"]?["label"]?.string
        else { return nil }
        let relationships = (inspection["relationships"]?.array ?? []).compactMap { view -> Relationship? in
            guard let label = view["label"]?.string,
                  let other = view["other_id"]?.string,
                  let state = view["state"]?.string
            else { return nil }
            return Relationship(label: label, otherID: other, state: state)
        }
        .filter { $0.state == "PRESENT" }
        return ThingEntity(id: thingID, title: label, relationships: relationships)
    }

    /// The attribute set serialized with `NSKeyedArchiver`, for tests that
    /// assert what the bytes do and do not carry.
    public func archivedAttributeSet() throws -> Data {
        try NSKeyedArchiver.archivedData(withRootObject: attributeSet, requiringSecureCoding: true)
    }
}

/// The entity query App Intents requires. The spike has no app-wide store,
/// so resolution goes through a provider the host installs; without one the
/// query returns nothing.
@available(macOS 15, *)
public struct ThingQuery: EntityQuery, Sendable {
    public typealias Provider = @Sendable (String) throws -> ThingEntity?

    @MainActor public static var provider: Provider? = nil

    public init() {}

    public func entities(for identifiers: [String]) async throws -> [ThingEntity] {
        guard let provider = await MainActor.run(body: { Self.provider }) else { return [] }
        return try identifiers.compactMap(provider)
    }

    public func suggestedEntities() async throws -> [ThingEntity] {
        []
    }
}
#endif
