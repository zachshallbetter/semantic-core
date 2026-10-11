import CryptoKit
import Foundation

/// `sha256:<hex>` over bytes, the shape the envelope uses for identities.
public enum Hash {
    public static func sha256(_ data: Data) -> String {
        "sha256:" + SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    }

    public static func sha256(_ text: String) -> String {
        sha256(Data(text.utf8))
    }
}

/// A minimal RFC 8785 (JCS) canonicalizer written independently of the core,
/// so the Swift side can prove the canonical bytes without calling across the
/// boundary. Scope: the envelope's value space. Object members are sorted by
/// UTF-16 code units; no whitespace; strings escaped per RFC 8785; integers
/// only, within 2^53 in magnitude. Fractions and exponents are refused, since
/// no envelope carries them.
public enum JCS {
    public enum Error: Swift.Error, CustomStringConvertible {
        case syntax(String)
        case number(String)

        public var description: String {
            switch self {
            case .syntax(let message): return "jcs: \(message)"
            case .number(let message): return "jcs: \(message)"
            }
        }
    }

    public indirect enum Value: Equatable {
        case object([(String, Value)])
        case array([Value])
        case string(String)
        case integer(Int64)
        case bool(Bool)
        case null

        public static func == (lhs: Value, rhs: Value) -> Bool {
            canonicalize(lhs) == canonicalize(rhs)
        }
    }

    public static let maxInteger: Int64 = 1 << 53

    /// Canonical bytes of JSON text.
    public static func canonicalize(_ text: String) throws -> String {
        canonicalize(try parse(text))
    }

    /// Canonical bytes of a parsed value.
    public static func canonicalize(_ value: Value) -> String {
        var out = ""
        write(value, into: &out)
        return out
    }

    private static func write(_ value: Value, into out: inout String) {
        switch value {
        case .null: out += "null"
        case .bool(let b): out += b ? "true" : "false"
        case .integer(let i): out += String(i)
        case .string(let s): writeString(s, into: &out)
        case .array(let items):
            out += "["
            for (index, item) in items.enumerated() {
                if index > 0 { out += "," }
                write(item, into: &out)
            }
            out += "]"
        case .object(let members):
            let sorted = members.sorted { lhs, rhs in
                Array(lhs.0.utf16).lexicographicallyPrecedes(Array(rhs.0.utf16))
            }
            out += "{"
            for (index, (key, item)) in sorted.enumerated() {
                if index > 0 { out += "," }
                writeString(key, into: &out)
                out += ":"
                write(item, into: &out)
            }
            out += "}"
        }
    }

    private static func writeString(_ s: String, into out: inout String) {
        out += "\""
        for scalar in s.unicodeScalars {
            switch scalar {
            case "\"": out += "\\\""
            case "\\": out += "\\\\"
            case "\u{08}": out += "\\b"
            case "\u{09}": out += "\\t"
            case "\u{0A}": out += "\\n"
            case "\u{0C}": out += "\\f"
            case "\u{0D}": out += "\\r"
            default:
                if scalar.value < 0x20 {
                    out += String(format: "\\u%04x", scalar.value)
                } else {
                    out.unicodeScalars.append(scalar)
                }
            }
        }
        out += "\""
    }

    // ------------------------------------------------------------ parser

    public static func parse(_ text: String) throws -> Value {
        var parser = Parser(scalars: Array(text.unicodeScalars))
        parser.skipWhitespace()
        let value = try parser.parseValue()
        parser.skipWhitespace()
        guard parser.atEnd else { throw Error.syntax("trailing characters") }
        return value
    }

    private struct Parser {
        let scalars: [Unicode.Scalar]
        var index = 0

        var atEnd: Bool { index >= scalars.count }

        mutating func skipWhitespace() {
            while !atEnd, [" ", "\t", "\n", "\r"].contains(scalars[index]) { index += 1 }
        }

        mutating func parseValue() throws -> Value {
            guard !atEnd else { throw Error.syntax("unexpected end") }
            switch scalars[index] {
            case "{": return try parseObject()
            case "[": return try parseArray()
            case "\"": return .string(try parseString())
            case "t": try expect("true"); return .bool(true)
            case "f": try expect("false"); return .bool(false)
            case "n": try expect("null"); return .null
            default: return try parseNumber()
            }
        }

        mutating func expect(_ word: String) throws {
            for scalar in word.unicodeScalars {
                guard !atEnd, scalars[index] == scalar else { throw Error.syntax("expected \(word)") }
                index += 1
            }
        }

        mutating func parseObject() throws -> Value {
            index += 1
            var members: [(String, Value)] = []
            skipWhitespace()
            if !atEnd, scalars[index] == "}" { index += 1; return .object(members) }
            while true {
                skipWhitespace()
                guard !atEnd, scalars[index] == "\"" else { throw Error.syntax("expected object key") }
                let key = try parseString()
                skipWhitespace()
                guard !atEnd, scalars[index] == ":" else { throw Error.syntax("expected ':'") }
                index += 1
                skipWhitespace()
                let value = try parseValue()
                // Duplicates are judged by code units, not by canonical
                // equivalence: "é" and "e\u{301}" are different keys.
                let units = Array(key.utf16)
                if members.contains(where: { Array($0.0.utf16) == units }) { throw Error.syntax("duplicate key \(key)") }
                members.append((key, value))
                skipWhitespace()
                guard !atEnd else { throw Error.syntax("unterminated object") }
                if scalars[index] == "," { index += 1; continue }
                if scalars[index] == "}" { index += 1; return .object(members) }
                throw Error.syntax("expected ',' or '}'")
            }
        }

        mutating func parseArray() throws -> Value {
            index += 1
            var items: [Value] = []
            skipWhitespace()
            if !atEnd, scalars[index] == "]" { index += 1; return .array(items) }
            while true {
                skipWhitespace()
                items.append(try parseValue())
                skipWhitespace()
                guard !atEnd else { throw Error.syntax("unterminated array") }
                if scalars[index] == "," { index += 1; continue }
                if scalars[index] == "]" { index += 1; return .array(items) }
                throw Error.syntax("expected ',' or ']'")
            }
        }

        mutating func parseString() throws -> String {
            index += 1
            var out = String.UnicodeScalarView()
            while true {
                guard !atEnd else { throw Error.syntax("unterminated string") }
                let scalar = scalars[index]
                index += 1
                if scalar == "\"" { return String(out) }
                if scalar != "\\" {
                    out.append(scalar)
                    continue
                }
                guard !atEnd else { throw Error.syntax("unterminated escape") }
                let escape = scalars[index]
                index += 1
                switch escape {
                case "\"": out.append("\"")
                case "\\": out.append("\\")
                case "/": out.append("/")
                case "b": out.append("\u{08}")
                case "f": out.append("\u{0C}")
                case "n": out.append("\u{0A}")
                case "r": out.append("\u{0D}")
                case "t": out.append("\u{09}")
                case "u":
                    let first = try parseHex4()
                    if (0xD800...0xDBFF).contains(first) {
                        guard index + 1 < scalars.count, scalars[index] == "\\", scalars[index + 1] == "u" else {
                            throw Error.syntax("lone high surrogate")
                        }
                        index += 2
                        let second = try parseHex4()
                        guard (0xDC00...0xDFFF).contains(second) else { throw Error.syntax("bad low surrogate") }
                        let combined = 0x10000 + ((first - 0xD800) << 10) + (second - 0xDC00)
                        guard let value = Unicode.Scalar(combined) else { throw Error.syntax("bad surrogate pair") }
                        out.append(value)
                    } else {
                        guard let value = Unicode.Scalar(first) else { throw Error.syntax("lone low surrogate") }
                        out.append(value)
                    }
                default: throw Error.syntax("bad escape \\\(escape)")
                }
            }
        }

        mutating func parseHex4() throws -> UInt32 {
            guard index + 4 <= scalars.count else { throw Error.syntax("short \\u escape") }
            var value: UInt32 = 0
            for _ in 0..<4 {
                let scalar = scalars[index]
                index += 1
                guard let digit = scalar.properties.numericType != nil ? UInt32(String(scalar)) : hexDigit(scalar) else {
                    throw Error.syntax("bad hex digit")
                }
                value = value << 4 | digit
            }
            return value
        }

        func hexDigit(_ scalar: Unicode.Scalar) -> UInt32? {
            switch scalar {
            case "a"..."f": return scalar.value - 97 + 10
            case "A"..."F": return scalar.value - 65 + 10
            default: return nil
            }
        }

        mutating func parseNumber() throws -> Value {
            let start = index
            if !atEnd, scalars[index] == "-" { index += 1 }
            guard !atEnd, ("0"..."9").contains(scalars[index]) else { throw Error.syntax("expected a value") }
            while !atEnd, ("0"..."9").contains(scalars[index]) { index += 1 }
            if !atEnd, [".", "e", "E"].contains(scalars[index]) {
                throw Error.number("fractions and exponents are outside the envelope's value space")
            }
            let text = String(String.UnicodeScalarView(scalars[start..<index]))
            guard let value = Int64(text) else { throw Error.number("\(text) does not fit") }
            guard value.magnitude <= UInt64(JCS.maxInteger) else {
                throw Error.number("\(text) exceeds 2^53 and cannot round-trip through a double")
            }
            return .integer(value)
        }
    }
}
