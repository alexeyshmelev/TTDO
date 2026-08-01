import Cocoa
import FlutterMacOS
import TrustTunnelClient
import XCTest

class RunnerTests: XCTestCase {
    func testFileLoggerReplacesArchiveOnSecondRotation() throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("trusttunnel-file-logger-\(UUID().uuidString)")
        defer {
            Logger.setCallback(nil)
            try? FileManager.default.removeItem(at: directory)
        }

        let fileLogger = FileLogger(directory: directory,
                                    baseName: "rotation",
                                    maxFileSize: 256,
                                    archiveCount: 1)
        fileLogger.install()
        Logger.setLogLevel(.trace)
        let logger = Logger(category: "rotation-test")
        logger.info(String(repeating: "A", count: 150))
        logger.info(String(repeating: "B", count: 150))
        logger.info(String(repeating: "C", count: 150))

        let current = directory.appendingPathComponent("rotation.log")
        let archive = directory.appendingPathComponent("rotation.1.log")
        let deadline = Date().addingTimeInterval(5)
        while Date() < deadline {
            let currentContents = try? String(contentsOf: current, encoding: .utf8)
            let archiveContents = try? String(contentsOf: archive, encoding: .utf8)
            if currentContents?.contains(String(repeating: "C", count: 150)) == true,
               archiveContents?.contains(String(repeating: "B", count: 150)) == true {
                XCTAssertFalse(archiveContents?.contains(String(repeating: "A", count: 150)) == true)
                return
            }
            RunLoop.current.run(until: Date().addingTimeInterval(0.02))
        }

        XCTFail("the second rotation did not replace the existing archive")
    }
}
