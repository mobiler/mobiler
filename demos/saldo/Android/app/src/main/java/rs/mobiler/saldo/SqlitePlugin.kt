package rs.mobiler.saldo

import rs.mobiler.saldo.shared.types.PluginResponse

import android.app.Application
import android.content.Context
import android.database.sqlite.SQLiteDatabase
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject

/** Free bundled plugin: on-device SQLite (no permission). Input is the SQL string, or
 *  {"sql": "…", "args": ["…"]} for bound parameters. Operates on one app database ("mobiler.db").
 *  - op "exec"  → run a statement (CREATE/INSERT/UPDATE/DELETE) → "executed".
 *  - op "query" → run a SELECT → a JSON array of row objects (column → text value).
 *  Calls run one at a time, in the order the core issued them, on one connection (ADR-0065): an
 *  `exec` answers ok:true once its statement has run, so the write is committed before the next
 *  call starts, and a `query` after it sees it. */
class SqlitePlugin(private val application: Application) : MobilerPlugin {
    private companion object {
        // One thread, off the main one: the shell starts plugin calls in the order the core issued
        // them (Core.kt's launches run FIFO on the main dispatcher), each reaches this lane before it
        // first suspends, and the lane runs them one by one in that order. One per process, so a
        // relaunched activity's new plugin queues behind the old one's calls.
        val lane = java.util.concurrent.Executors.newSingleThreadExecutor().asCoroutineDispatcher()

        // The one connection, opened on first use and kept for the life of the process. Touched only on `lane`.
        var connection: SQLiteDatabase? = null

        // A statement that opens, ends or marks a transaction is refused: on the kept connection a
        // transaction would outlive its call. Android's session sees only some of them (not SAVEPOINT or
        // a comment-prefixed BEGIN), so this check, not inTransaction(), is what stops them.
        val transactionControl = setOf("BEGIN", "COMMIT", "END", "ROLLBACK", "SAVEPOINT", "RELEASE")

        /** The statement's first keyword, past leading whitespace and `--` / `/* */` comments, upper-cased. */
        fun firstKeyword(sql: String): String {
            var i = 0
            while (i < sql.length) {
                i = when {
                    sql[i].isWhitespace() -> i + 1
                    sql.startsWith("--", i) -> sql.indexOf('\n', i).let { if (it < 0) sql.length else it + 1 }
                    sql.startsWith("/*", i) -> sql.indexOf("*/", i + 2).let { if (it < 0) sql.length else it + 2 }
                    else -> break
                }
            }
            return sql.substring(i).takeWhile { it.isLetter() }.uppercase()
        }
    }

    // NonCancellable: a call already queued still runs when the screen's scope is cancelled (the activity
    // finishing), so a write the core issued is never dropped from the lane.
    override suspend fun handle(op: String, input: String): PluginResponse = withContext(NonCancellable + lane) { run(op, input) }

    private fun run(op: String, input: String): PluginResponse {
        val response = execute(op, input)
        // The connection is kept, so a transaction left open (a `BEGIN` with no `COMMIT`) would hold every
        // later write uncommitted while each answered ok:true. A transaction can't span calls: roll it back.
        // inTransaction() sees an exec'd "BEGIN" because SQLiteSession runs BEGIN/COMMIT/ROLLBACK as its own
        // transaction calls, scoped to the calling thread; every call runs on the one lane thread.
        val db = connection
        if (db != null && db.isOpen && db.inTransaction()) {
            try {
                while (db.inTransaction()) db.endTransaction()
            } catch (e: Exception) {
                // Nothing more to undo here; the call still answers that it failed.
            }
            return PluginResponse(false, "a transaction can't span calls: rolled back")
        }
        return response
    }

    private fun open(): SQLiteDatabase = connection?.takeIf { it.isOpen }
        ?: application.openOrCreateDatabase("mobiler.db", Context.MODE_PRIVATE, null).also { db ->
            // Nothing in this process contends for the file (one connection); another opener of it is
            // waited for, up to 5s, instead of failing. PRAGMA busy_timeout returns a row, and execSQL
            // refuses statements that return rows, so it runs as a query.
            db.rawQuery("PRAGMA busy_timeout=5000", null).use { it.moveToFirst() }
            connection = db
        }

    private fun execute(op: String, input: String): PluginResponse = try {
        val obj = if (input.startsWith("{")) JSONObject(input) else JSONObject().put("sql", input)
        val sql = obj.optString("sql")
        val argsArr = obj.optJSONArray("args")
        val args = Array(argsArr?.length() ?: 0) { argsArr!!.getString(it) }
        when {
            sql.isEmpty() -> PluginResponse(false, "no sql")
            firstKeyword(sql) in transactionControl -> PluginResponse(false, "a transaction can't span calls")
            op == "exec" -> {
                open().execSQL(sql, args)
                PluginResponse(true, "executed")
            }
            op == "query" -> {
                val rows = JSONArray()
                open().rawQuery(sql, args).use { c ->
                    while (c.moveToNext()) {
                        val row = JSONObject()
                        for (i in 0 until c.columnCount) {
                            row.put(c.getColumnName(i), c.getString(i))
                        }
                        rows.put(row)
                    }
                }
                PluginResponse(true, rows.toString())
            }
            else -> PluginResponse(false, "unknown op '$op'")
        }
    } catch (e: Exception) {
        PluginResponse(false, e.message ?: "sqlite error")
    }
}
