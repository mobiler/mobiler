import SharedTypes
import Foundation
import SQLite3

/// On-device SQLite — matches the Android contract: op "exec" runs a statement (→ "executed"),
/// op "query" returns rows as a JSON array of `{column: stringValue}` objects (all values strings).
/// Input is a plain SQL string or `{"sql": "...", "args": ["..."]}`. One shared DB file
/// (`mobiler.db`) in the app's Documents directory. Uses the system SQLite3 module (no extra link).
enum SqlitePlugin {
    // SQLite needs to copy bound text (the Swift String is transient), so bind with TRANSIENT.
    private static let transient = unsafeBitCast(-1, to: sqlite3_destructor_type.self)

    static func handle(op: String, input: String) async -> PluginResponse {
        let (sql, args) = parse(input)
        if sql.isEmpty { return PluginResponse(ok: false, output: "no sql") }

        let path = (try? FileManager.default.url(for: .documentDirectory, in: .userDomainMask, appropriateFor: nil, create: true))?
            .appendingPathComponent("mobiler.db").path ?? ":memory:"
        var db: OpaquePointer?
        guard sqlite3_open(path, &db) == SQLITE_OK else {
            let msg = db.map { String(cString: sqlite3_errmsg($0)) } ?? "open failed"
            sqlite3_close(db)
            return PluginResponse(ok: false, output: msg)
        }
        defer { sqlite3_close(db) }

        var stmt: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &stmt, nil) == SQLITE_OK else {
            return PluginResponse(ok: false, output: String(cString: sqlite3_errmsg(db)))
        }
        defer { sqlite3_finalize(stmt) }
        for (i, a) in args.enumerated() {
            sqlite3_bind_text(stmt, Int32(i + 1), a, -1, transient)
        }

        switch op {
        case "exec":
            let rc = sqlite3_step(stmt)
            if rc == SQLITE_DONE || rc == SQLITE_ROW {
                return PluginResponse(ok: true, output: "executed")
            }
            return PluginResponse(ok: false, output: String(cString: sqlite3_errmsg(db)))
        case "query":
            var rows: [[String: String]] = []
            while sqlite3_step(stmt) == SQLITE_ROW {
                var row: [String: String] = [:]
                for c in 0..<sqlite3_column_count(stmt) {
                    let name = String(cString: sqlite3_column_name(stmt, c))
                    if let v = sqlite3_column_text(stmt, c) {
                        row[name] = String(cString: v)
                    } else {
                        row[name] = ""
                    }
                }
                rows.append(row)
            }
            let data = (try? JSONSerialization.data(withJSONObject: rows)) ?? Data("[]".utf8)
            return PluginResponse(ok: true, output: String(data: data, encoding: .utf8) ?? "[]")
        default:
            return PluginResponse(ok: false, output: "unknown op '\(op)'")
        }
    }

    /// Plain SQL string, or `{"sql": "...", "args": [...]}` (args coerced to strings, like Android).
    private static func parse(_ input: String) -> (String, [String]) {
        if input.hasPrefix("{"),
           let obj = (try? JSONSerialization.jsonObject(with: Data(input.utf8))) as? [String: Any] {
            let sql = (obj["sql"] as? String) ?? ""
            let args = (obj["args"] as? [Any])?.map { "\($0)" } ?? []
            return (sql, args)
        }
        return (input, [])
    }
}
