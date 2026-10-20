import SharedTypes
import Foundation
import SQLite3

/// On-device SQLite — matches the Android contract: op "exec" runs a statement (→ "executed"),
/// op "query" returns rows as a JSON array of `{column: stringValue}` objects (all values strings).
/// Input is a plain SQL string or `{"sql": "...", "args": ["..."]}`. One shared DB file
/// (`mobiler.db`) in the app's Documents directory. Uses the system SQLite3 module (no extra link).
///
/// Calls run one at a time, in the order the core issued them, on one connection (ADR-0065): the
/// shell starts a `sqlite` call only once the call before it has answered (`Plugins.inOrder` in
/// Core.swift), and this lane runs them one by one off the main thread. An `exec` answers ok:true
/// only once its statement has run to the end, so the write is committed before the next call starts.
enum SqlitePlugin {
    // SQLite needs to copy bound text (the Swift String is transient), so bind with TRANSIENT.
    private static let transient = unsafeBitCast(-1, to: sqlite3_destructor_type.self)

    // The lane: one serial queue, the only code that touches the connection. It runs its blocks one at a
    // time in the order they were queued; the SQLite work blocks a GCD thread, never the main one.
    private static let lane = DispatchQueue(label: "mobiler.sqlite")
    // The one connection, opened on first use and kept for the life of the process. Touched only on
    // `lane`, which is what makes `nonisolated(unsafe)` sound here.
    nonisolated(unsafe) private static var connection: OpaquePointer?

    static func handle(op: String, input: String) async -> PluginResponse {
        await withCheckedContinuation { cont in
            lane.async { cont.resume(returning: run(op: op, input: input)) }
        }
    }

    /// The connection, opened on first use (on `lane`); `nil` and the reason when it can't be.
    private static func open() -> (OpaquePointer?, String) {
        if let db = connection { return (db, "") }
        let path = (try? FileManager.default.url(for: .documentDirectory, in: .userDomainMask, appropriateFor: nil, create: true))?
            .appendingPathComponent("mobiler.db").path ?? ":memory:"
        var db: OpaquePointer?
        guard sqlite3_open(path, &db) == SQLITE_OK else {
            let msg = db.map { String(cString: sqlite3_errmsg($0)) } ?? "open failed"
            sqlite3_close(db)
            return (nil, msg)
        }
        // Nothing in this process contends for the file (one connection); another opener of it is
        // waited for, up to 5s, instead of failing with SQLITE_BUSY.
        sqlite3_busy_timeout(db, 5000)
        connection = db
        return (db, "")
    }

    private static func run(op: String, input: String) -> PluginResponse {
        let (sql, args) = parse(input)
        if sql.isEmpty { return PluginResponse(ok: false, output: "no sql") }
        if transactionControl.contains(firstKeyword(sql)) {
            return PluginResponse(ok: false, output: "a transaction can't span calls")
        }
        let (opened, failure) = open()
        guard let db = opened else { return PluginResponse(ok: false, output: failure) }
        let response = execute(db: db, op: op, sql: sql, args: args)
        // The connection is kept, so a transaction left open (a `BEGIN` with no `COMMIT`) would hold every
        // later write uncommitted while each answered ok:true. A transaction can't span calls: roll it back.
        if sqlite3_get_autocommit(db) == 0 {
            sqlite3_exec(db, "ROLLBACK", nil, nil, nil)
            return PluginResponse(ok: false, output: "a transaction can't span calls: rolled back")
        }
        return response
    }

    /// Runs one statement on `db` (its statement is finalized before this returns).
    private static func execute(db: OpaquePointer, op: String, sql: String, args: [String]) -> PluginResponse {
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
            // Run the statement to the end (past any rows it returns, as a PRAGMA does), so the write
            // is committed when this answers and a failure on a later row is reported.
            var rc = sqlite3_step(stmt)
            while rc == SQLITE_ROW { rc = sqlite3_step(stmt) }
            if rc == SQLITE_DONE {
                return PluginResponse(ok: true, output: "executed")
            }
            return PluginResponse(ok: false, output: String(cString: sqlite3_errmsg(db)))
        case "query":
            var rows: [[String: String]] = []
            var rc = sqlite3_step(stmt)
            while rc == SQLITE_ROW {
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
                rc = sqlite3_step(stmt)
            }
            // A step that fails part-way is an error, never a short list of rows.
            if rc != SQLITE_DONE {
                return PluginResponse(ok: false, output: String(cString: sqlite3_errmsg(db)))
            }
            let data = (try? JSONSerialization.data(withJSONObject: rows)) ?? Data("[]".utf8)
            return PluginResponse(ok: true, output: String(data: data, encoding: .utf8) ?? "[]")
        default:
            return PluginResponse(ok: false, output: "unknown op '\(op)'")
        }
    }

    // A statement that opens, ends or marks a transaction is refused: on the kept connection a transaction
    // would outlive its call (the open-transaction check in `run` is the backstop).
    private static let transactionControl: Set<String> = ["BEGIN", "COMMIT", "END", "ROLLBACK", "SAVEPOINT", "RELEASE"]

    /// The statement's first keyword, past leading whitespace and `--` / `/* */` comments, upper-cased.
    private static func firstKeyword(_ sql: String) -> String {
        var rest = Substring(sql)
        while true {
            rest = rest.drop(while: { $0.isWhitespace })
            if rest.hasPrefix("--") {
                rest = rest.firstIndex(where: { $0.isNewline }).map { rest[rest.index(after: $0)...] } ?? ""
            } else if rest.hasPrefix("/*") {
                let body = rest.dropFirst(2)
                rest = body.range(of: "*/").map { body[$0.upperBound...] } ?? ""
            } else {
                break
            }
        }
        return String(rest.prefix(while: { $0.isLetter })).uppercased()
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
