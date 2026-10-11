import CSemanticCore
import Foundation

/// A library handle. Every call acts as the library owner. Not thread-safe:
/// keep one instance per thread or actor.
public final class Store {
    private var handle: OpaquePointer?

    /// Open the library at `path`, creating it for `owner` when it does not exist.
    public init(path: String, owner: String) throws {
        var out: OpaquePointer? = nil
        let code = withUnsafeMutablePointer(to: &out) { sc_store_open(path, owner, $0) }
        try check(code)
        handle = out
    }

    deinit {
        sc_store_close(handle)
    }

    /// Close early. Later calls fail with `.argument`.
    public func close() {
        sc_store_close(handle)
        handle = nil
    }

    public struct FileRecord: Decodable, Sendable {
        public let artifactID: String
        public let revisionID: String
        public let instanceID: String

        enum Keys: String, CodingKey { case artifact, revision, instance }
        enum IDKey: String, CodingKey { case id }

        public init(from decoder: Decoder) throws {
            let container = try decoder.container(keyedBy: Keys.self)
            artifactID = try container.nestedContainer(keyedBy: IDKey.self, forKey: .artifact).decode(String.self, forKey: .id)
            revisionID = try container.nestedContainer(keyedBy: IDKey.self, forKey: .revision).decode(String.self, forKey: .id)
            instanceID = try container.nestedContainer(keyedBy: IDKey.self, forKey: .instance).decode(String.self, forKey: .id)
        }
    }

    public struct LinkRecord: Decodable, Sendable {
        public let relationshipID: String
        public let assertionID: String
        public let state: String

        enum Keys: String, CodingKey { case relationship, assertion, state }
        enum IDKey: String, CodingKey { case id }

        public init(from decoder: Decoder) throws {
            let container = try decoder.container(keyedBy: Keys.self)
            relationshipID = try container.nestedContainer(keyedBy: IDKey.self, forKey: .relationship).decode(String.self, forKey: .id)
            assertionID = try container.nestedContainer(keyedBy: IDKey.self, forKey: .assertion).decode(String.self, forKey: .id)
            state = try container.decode(String.self, forKey: .state)
        }
    }

    public struct Resolution: Decodable, Sendable {
        public let id: String
        public let outcome: String
        public let rule: String
        public let instanceID: String?
        public let candidates: [String]

        enum CodingKeys: String, CodingKey {
            case id, outcome, rule, candidates
            case instanceID = "instance_id"
        }
    }

    public struct RetractRecord: Decodable, Sendable {
        public let state: String
        public let compensates: String?

        enum Keys: String, CodingKey { case state, event }

        public init(from decoder: Decoder) throws {
            let container = try decoder.container(keyedBy: Keys.self)
            state = try container.decode(String.self, forKey: .state)
            // The compensated hash sits inside the event body's change data.
            let event = try container.decode(JSONObject.self, forKey: .event)
            compensates = event["envelope"]?["body"]?["change"]?["data"]?["compensates"]?.string
        }
    }

    private func decode<T: Decodable>(_ type: T.Type, from json: String) throws -> T {
        try JSONDecoder().decode(T.self, from: Data(json.utf8))
    }

    public func addThing(label: String) throws -> String {
        try call { out in sc_store_add_thing(handle, label, out) }
    }

    public func addFile(path: String, volume: String, document: String) throws -> FileRecord {
        try decode(FileRecord.self, from: try call { out in sc_store_add_file(handle, path, volume, document, out) })
    }

    public func link(source: String, predicate: String, target: String) throws -> LinkRecord {
        try decode(LinkRecord.self, from: try call { out in sc_store_link(handle, source, predicate, target, out) })
    }

    public func resolve(path: String, volume: String, document: String) throws -> Resolution {
        try decode(Resolution.self, from: try call { out in sc_store_resolve(handle, path, volume, document, out) })
    }

    public func gone(instance: String) throws -> Resolution {
        try decode(Resolution.self, from: try call { out in sc_store_gone(handle, instance, out) })
    }

    /// The inspection as JSON text, mirroring the core's `Inspection`.
    public func inspectJSON(_ id: String) throws -> String {
        try call { out in sc_store_inspect(handle, id, out) }
    }

    public func inspect(_ id: String) throws -> JSONObject {
        try decode(JSONObject.self, from: try inspectJSON(id))
    }

    public func whyJSON(relationship: String) throws -> String {
        try call { out in sc_store_why(handle, relationship, out) }
    }

    public func why(relationship: String) throws -> JSONObject {
        try decode(JSONObject.self, from: try whyJSON(relationship: relationship))
    }

    public func history(_ id: String) throws -> [JSONObject] {
        try decode([JSONObject].self, from: try call { out in sc_store_history(handle, id, out) })
    }

    public func retract(assertion: String, reason: String? = nil) throws -> RetractRecord {
        try decode(RetractRecord.self, from: try call { out in sc_store_retract(handle, assertion, reason, out) })
    }
}

/// A loosely typed JSON object for inspecting core records without mirroring
/// every type in Swift.
public indirect enum JSONObject: Decodable, Sendable {
    case object([String: JSONObject])
    case array([JSONObject])
    case string(String)
    case number(Double)
    case bool(Bool)
    case null

    public init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        if container.decodeNil() {
            self = .null
        } else if let value = try? container.decode(Bool.self) {
            self = .bool(value)
        } else if let value = try? container.decode(Double.self) {
            self = .number(value)
        } else if let value = try? container.decode(String.self) {
            self = .string(value)
        } else if let value = try? container.decode([JSONObject].self) {
            self = .array(value)
        } else {
            self = .object(try container.decode([String: JSONObject].self))
        }
    }

    public subscript(key: String) -> JSONObject? {
        if case .object(let members) = self { return members[key] }
        return nil
    }

    public var string: String? {
        if case .string(let value) = self { return value }
        return nil
    }

    public var array: [JSONObject]? {
        if case .array(let items) = self { return items }
        return nil
    }
}
