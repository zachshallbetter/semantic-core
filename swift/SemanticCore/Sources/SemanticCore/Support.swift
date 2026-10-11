import CSemanticCore
import Foundation

/// A failure reported by the core: the code from `sc.h` and the message of
/// the last failing call.
public struct SemanticCoreError: Error, CustomStringConvertible, Sendable {
    public enum Code: Int32, Sendable {
        case argument = 1
        case validation = 2
        case notFound = 3
        case unauthorized = 4
        case conflict = 5
        case integrity = 6
        case storage = 7
        case authentication = 8
        case internalFailure = 9
        case unknown = -1
    }

    public let code: Code
    public let rawCode: Int32
    public let message: String

    public var description: String { "\(code) (\(rawCode)): \(message)" }
}

/// Convert a code into a thrown error, reading and freeing the last message.
func check(_ code: Int32) throws {
    guard code != SC_OK else { return }
    let message: String
    if let pointer = sc_last_error() {
        message = String(cString: pointer)
        sc_string_free(pointer)
    } else {
        message = "no message"
    }
    throw SemanticCoreError(
        code: SemanticCoreError.Code(rawValue: code) ?? .unknown,
        rawCode: code,
        message: message
    )
}

/// Take ownership of a string the core returned and free it.
func take(_ pointer: UnsafeMutablePointer<CChar>?) -> String {
    guard let pointer else { return "" }
    defer { sc_string_free(pointer) }
    return String(cString: pointer)
}

/// Run a core call that returns one string.
func call(_ body: (UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>) -> Int32) throws -> String {
    var out: UnsafeMutablePointer<CChar>? = nil
    let code = withUnsafeMutablePointer(to: &out) { body($0) }
    try check(code)
    return take(out)
}

/// 32 raw bytes for a key or seed, from a hex string or `Data`.
func thirtyTwo(_ data: Data, _ what: String) throws -> [UInt8] {
    guard data.count == 32 else {
        throw SemanticCoreError(code: .argument, rawCode: 1, message: "\(what) must be 32 bytes, got \(data.count)")
    }
    return [UInt8](data)
}

public extension Data {
    /// Parse lowercase or uppercase hex.
    init?(hex: String) {
        let characters = Array(hex.utf8)
        guard characters.count % 2 == 0 else { return nil }
        var bytes: [UInt8] = []
        bytes.reserveCapacity(characters.count / 2)
        var index = 0
        while index < characters.count {
            guard let high = hexValue(characters[index]), let low = hexValue(characters[index + 1]) else { return nil }
            bytes.append(high << 4 | low)
            index += 2
        }
        self.init(bytes)
    }

    var hexString: String {
        map { String(format: "%02x", $0) }.joined()
    }
}

private func hexValue(_ byte: UInt8) -> UInt8? {
    switch byte {
    case UInt8(ascii: "0")...UInt8(ascii: "9"): return byte - UInt8(ascii: "0")
    case UInt8(ascii: "a")...UInt8(ascii: "f"): return byte - UInt8(ascii: "a") + 10
    case UInt8(ascii: "A")...UInt8(ascii: "F"): return byte - UInt8(ascii: "A") + 10
    default: return nil
    }
}

/// The version of the linked core.
public enum Core {
    public static var version: String {
        String(cString: sc_version())
    }
}
