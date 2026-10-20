package rs.mobiler.saldo

import rs.mobiler.saldo.shared.types.PluginResponse

import android.app.Application
import android.content.Context
import android.database.sqlite.SQLiteDatabase
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
    }

    override suspend fun handle(op: String, input: String): PluginResponse = withContext(lane) { run(op, input) }

    private fun open(): SQLiteDatabase = connection?.takeIf { it.isOpen }
        ?: application.openOrCreateDatabase("mobiler.db", Context.MODE_PRIVATE, null).also { db ->
            // Nothing in this process contends for the file (one connection); another opener of it is
            // waited for, up to 5s, instead of failing. PRAGMA busy_timeout returns a row, and execSQL
            // refuses statements that return rows, so it runs as a query.
            db.rawQuery("PRAGMA busy_timeout=5000", null).use { it.moveToFirst() }
            connection = db
        }

    private fun run(op: String, input: String): PluginResponse = try {
        val obj = if (input.startsWith("{")) JSONObject(input) else JSONObject().put("sql", input)
        val sql = obj.optString("sql")
        val argsArr = obj.optJSONArray("args")
        val args = Array(argsArr?.length() ?: 0) { argsArr!!.getString(it) }
        when {
            sql.isEmpty() -> PluginResponse(false, "no sql")
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
