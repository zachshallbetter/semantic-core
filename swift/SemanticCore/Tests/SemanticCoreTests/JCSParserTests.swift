import Foundation
import XCTest
@testable import SemanticCore

final class JCSParserTests: XCTestCase {
    private func assertSyntaxError(_ text: String, file: StaticString = #filePath, line: UInt = #line) {
        for operation in [{ _ = try JCS.parse(text) }, { _ = try JCS.canonicalize(text) }] {
            XCTAssertThrowsError(try operation(), text.debugDescription, file: file, line: line) { error in
                guard case JCS.Error.syntax = error else {
                    XCTFail("Expected a JSON syntax error, got \(error)", file: file, line: line)
                    return
                }
            }
        }
    }

    func testRejectsEveryUnescapedControlInStringsAndKeys() throws {
        for value in UInt32(0)...UInt32(0x1f) {
            let scalar = try XCTUnwrap(Unicode.Scalar(value))
            let string = "\"before" + String(scalar) + "after\""
            assertSyntaxError(string)
            assertSyntaxError("[\(string)]")
            assertSyntaxError("{\(string):0}")
            assertSyntaxError("{\"key\":\(string)}")
        }
    }

    func testAcceptsEscapedControls() throws {
        for value in UInt32(0)...UInt32(0x1f) {
            let scalar = try XCTUnwrap(Unicode.Scalar(value))
            let escaped = "\"" + String(format: "\\u%04x", value) + "\""
            let decoded = String(scalar)
            XCTAssertEqual(try JCS.parse(escaped), .string(decoded))
            XCTAssertEqual(try JCS.canonicalize(escaped), JCS.canonicalize(.string(decoded)))
            XCTAssertEqual(try JCS.canonicalize("{\(escaped):0}"),
                           "{" + JCS.canonicalize(.string(decoded)) + ":0}")
        }
        XCTAssertEqual(try JCS.canonicalize(#""\b\t\n\f\r""#), #""\b\t\n\f\r""#)
        XCTAssertEqual(try JCS.parse(#""\b\t\n\f\r""#), .string("\u{08}\t\n\u{0c}\r"))
        XCTAssertEqual(try JCS.canonicalize("\"\u{20}\u{7f}\""), "\"\u{20}\u{7f}\"")
    }

    func testRejectsLeadingZeroNumbers() {
        for number in ["00", "01", "0123", "-00", "-01", "-0123", "00.1", "-01.5", "01e2", "-01E2"] {
            assertSyntaxError(number)
            assertSyntaxError("[\(number)]")
            assertSyntaxError("{\"n\":\(number)}")
        }
    }

    func testAcceptsValidIntegerFormsAndPreservesBounds() throws {
        for number in ["0", "-0", "1", "-1", "10", "-10", "9007199254740992", "-9007199254740992"] {
            let value = try XCTUnwrap(Int64(number))
            let canonical = String(value)
            XCTAssertEqual(try JCS.parse(number), .integer(value))
            XCTAssertEqual(try JCS.canonicalize(number), canonical)
            XCTAssertEqual(try JCS.canonicalize("[\(number)]"), "[\(canonical)]")
            XCTAssertEqual(try JCS.canonicalize(" { \"n\" : \(number) } "), "{\"n\":\(canonical)}")
        }
        for number in ["9007199254740993", "-9007199254740993", "0.1", "-0.1", "0e1", "-0E1"] {
            XCTAssertThrowsError(try JCS.canonicalize(number)) { error in
                guard case JCS.Error.number = error else {
                    XCTFail("Expected the existing number restriction, got \(error)")
                    return
                }
            }
        }
    }
}
