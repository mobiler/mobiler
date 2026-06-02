package dev.mobiler.barbershop

import dev.mobiler.barbershop.shared.types.PluginResponse

import android.app.Application
import android.content.Context
import org.json.JSONArray
import org.json.JSONObject

/** Free bundled plugin: on-device SQLite (no permission). Input is the SQL string, or
 *  {"sql": "…", "args": ["…"]} for bound parameters. Operates on one app database ("mobiler.db").
 *  - op "exec"  → run a statement (CREATE/INSERT/UPDATE/DELETE) → "executed".
 *  - op "query" → run a SELECT → a JSON array of row objects (column → text value). */
class SqlitePlugin(private val application: Application) : MobilerPlugin {
    override suspend fun handle(op: String, input: String): PluginResponse {
        val obj = if (input.startsWith("{")) JSONObject(input) else JSONObject().put("sql", input)
        val sql = obj.optString("sql")
        if (sql.isEmpty()) return PluginResponse(false, "no sql")
        val argsArr = obj.optJSONArray("args")
        val args = Array(argsArr?.length() ?: 0) { argsArr!!.getString(it) }
        return try {
            application.openOrCreateDatabase("mobiler.db", Context.MODE_PRIVATE, null).use { db ->
                when (op) {
                    "exec" -> {
                        db.execSQL(sql, args)
                        PluginResponse(true, "executed")
                    }
                    "query" -> {
                        val rows = JSONArray()
                        db.rawQuery(sql, args).use { c ->
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
            }
        } catch (e: Exception) {
            PluginResponse(false, e.message ?: "sqlite error")
        }
    }
}
