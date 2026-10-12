import Foundation
import XCTest
@testable import SemanticCore

final class JSONObjectTests: XCTestCase {
    private func decode(_ text: String) throws -> JSONObject {
        try JSONDecoder().decode(JSONObject.self, from: Data(text.utf8))
    }

    func testSignedIntegersRemainExact() throws {
        for text in ["0", "-1", "9007199254740991", "9007199254740992", "9007199254740993",
                     "-9007199254740993", "9223372036854775807", "-9223372036854775808"] {
            let value = try decode(text)
            XCTAssertEqual(value.integer, Int64(text), text)
            guard case .integer = value else { XCTFail("Integer decoded through Double: \(text)"); continue }
        }
    }

    func testNestedRecordsKeepLargeIntegers() throws {
        let record = try decode(#"{"size":9007199254740993,"history":[-9223372036854775808,9223372036854775807]}"#)
        XCTAssertEqual(record["size"]?.integer, 9007199254740993)
        XCTAssertEqual(record["history"]?.array?.first?.integer, Int64.min)
        XCTAssertEqual(record["history"]?.array?.last?.integer, Int64.max)
    }

    func testFractionalNumbersKeepDoubleRepresentation() throws {
        for text in ["1.5", "-0.25", "1e-3"] {
            guard case .number(let value) = try decode(text) else {
                XCTFail("Expected Double for \(text)"); continue
            }
            XCTAssertEqual(value, try XCTUnwrap(Double(text)))
        }
        guard case .number = try decode("9223372036854775808") else {
            return XCTFail("Values outside Int64 retain the existing Double fallback")
        }
    }

    func testOtherJSONTypesRemainDistinct() throws {
        guard case .bool(true) = try decode("true") else { return XCTFail("Boolean became a number") }
        guard case .null = try decode("null") else { return XCTFail("Null changed") }
        XCTAssertEqual(try decode(#""9007199254740993""#).string, "9007199254740993")
    }
}
