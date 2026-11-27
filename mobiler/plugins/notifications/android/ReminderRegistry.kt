package {{PACKAGE}}

import android.content.Context
import java.time.LocalDateTime
import java.time.ZoneId
import java.time.ZonedDateTime
import java.time.format.DateTimeFormatter
import java.time.format.ResolverStyle
import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject

// The notifications plugin's registry of pending reminders, the `schedule` input validation and the
// wall-clock rules. Everything above `ReminderRegistry` is pure (java.time + org.json only), so it
// runs in a plain JVM unit test.
//
// Registry: SharedPreferences "mobiler_notifications", key "r.<id>", one JSON object per reminder:
//   {id, title, body, fire_at_ms, wall?: "YYYY-MM-DD HH:MM:SS", payload?, actions?, late_minutes, snoozed}
// `wall` is kept for `at` reminders so a time-zone change re-computes `fire_at_ms`.

/** One scheduled reminder, as stored in the registry. `actionsJson` is the validated actions array. */
data class Reminder(
    val id: Int,
    val title: String,
    val body: String,
    val fireAtMs: Long,
    val wall: String?,
    val payload: String?,
    val actionsJson: String?,
    val lateMinutes: Int,
    val snoozed: Boolean,
) {
    fun toJson(): String {
        val o = JSONObject()
            .put("id", id)
            .put("title", title)
            .put("body", body)
            .put("fire_at_ms", fireAtMs)
        if (wall != null) o.put("wall", wall)
        if (payload != null) o.put("payload", payload)
        if (actionsJson != null) o.put("actions", JSONArray(actionsJson))
        return o.put("late_minutes", lateMinutes).put("snoozed", snoozed).toString()
    }

    companion object {
        fun fromJson(s: String): Reminder? = try {
            val o = JSONObject(s)
            Reminder(
                id = o.getInt("id"),
                title = o.optString("title", ""),
                body = o.optString("body", ""),
                fireAtMs = o.getLong("fire_at_ms"),
                wall = if (o.isNull("wall")) null else o.getString("wall"),
                payload = if (o.isNull("payload")) null else o.getString("payload"),
                actionsJson = o.optJSONArray("actions")?.toString(),
                lateMinutes = o.optInt("late_minutes", DEFAULT_LATE_MINUTES),
                snoozed = o.optBoolean("snoozed", false),
            )
        } catch (e: JSONException) {
            null
        }
    }
}

const val DEFAULT_LATE_MINUTES = 1440
private const val MAX_LATE_MINUTES = 10080L
private const val MAX_ACTIONS = 3
private const val MAX_ACTION_ID_BYTES = 64
private const val MAX_PAYLOAD_BYTES = 1024

private class ScheduleError(message: String) : IllegalArgumentException(message)
private fun invalid(detail: String): Nothing = throw ScheduleError("invalid_input: $detail")

/**
 * Validates a `schedule` input (spec §4.2) into a [Reminder]. A failure's message is the op's output:
 * `invalid_input: <detail>` or `past`. Nothing is stored or armed here.
 */
fun parseSchedule(input: String, nowMs: Long, zone: ZoneId): Result<Reminder> = try {
    Result.success(parseScheduleOrThrow(input, nowMs, zone))
} catch (e: ScheduleError) {
    Result.failure(e)
}

private fun parseScheduleOrThrow(input: String, nowMs: Long, zone: ZoneId): Reminder {
    val o = try { JSONObject(input) } catch (e: JSONException) { invalid("not a JSON object") }

    // A missing id keeps the old default of 1; only a present but bad one is rejected.
    val id = intField(o, "id", 1L..Int.MAX_VALUE.toLong())?.toInt() ?: 1
    val title = if (o.isNull("title")) "" else o.optString("title")
    val body = if (o.isNull("body")) "" else o.optString("body")

    val hasAt = !o.isNull("at")
    val hasAfter = !o.isNull("after_seconds")
    if (hasAt == hasAfter) invalid("give exactly one of `at` and `after_seconds`")

    var wall: String? = null
    val fireAtMs: Long
    if (hasAt) {
        val raw = o.get("at") as? String ?: invalid("`at` must be a \"YYYY-MM-DD HH:MM\" string")
        val ldt = parseWall(raw) ?: invalid("`at` must be \"YYYY-MM-DD HH:MM\" or \"YYYY-MM-DD HH:MM:SS\"")
        wall = ldt.format(WALL_OUT)
        fireAtMs = resolveWall(ldt, zone)
    } else {
        // Exactly the old behaviour: a number (or numeric string) of seconds, truncated; ≤ 0 fires now.
        val v = o.get("after_seconds")
        val secs = when (v) {
            is Number -> v.toLong()
            is String -> v.trim().toLongOrNull() ?: v.trim().toDoubleOrNull()?.toLong()
            else -> null
        } ?: invalid("`after_seconds` must be a number")
        if (secs > (Long.MAX_VALUE - nowMs) / 1000) invalid("`after_seconds` is too large")
        fireAtMs = nowMs + secs.coerceAtLeast(0) * 1000
    }

    val payload = if (o.isNull("payload")) null else {
        val p = o.get("payload") as? String ?: invalid("`payload` must be a string")
        if (utf8Len(p) > MAX_PAYLOAD_BYTES) invalid("`payload` is over $MAX_PAYLOAD_BYTES bytes")
        p
    }

    val actionsJson = if (o.isNull("actions")) null else parseActions(o.get("actions"))
    val lateMinutes = intField(o, "late_minutes", 0L..MAX_LATE_MINUTES)?.toInt() ?: DEFAULT_LATE_MINUTES

    if (hasAt && fireAtMs <= nowMs) throw ScheduleError("past")

    return Reminder(id, title, body, fireAtMs, wall, payload, actionsJson, lateMinutes, snoozed = false)
}

/** The validated, normalised actions array, or null for none. */
private fun parseActions(v: Any): String? {
    val arr = v as? JSONArray ?: invalid("`actions` must be an array")
    if (arr.length() > MAX_ACTIONS) invalid("at most $MAX_ACTIONS actions")
    if (arr.length() == 0) return null
    val out = JSONArray()
    val ids = HashSet<String>()
    for (i in 0 until arr.length()) {
        val a = arr.opt(i) as? JSONObject ?: invalid("action $i must be an object")
        val aid = a.opt("id") as? String ?: invalid("action $i needs a string `id`")
        if (aid.isEmpty() || utf8Len(aid) > MAX_ACTION_ID_BYTES) invalid("action $i `id` must be 1..=$MAX_ACTION_ID_BYTES bytes")
        if (!ids.add(aid)) invalid("action id `$aid` is repeated")
        val label = a.opt("label") as? String ?: invalid("action $i needs a string `label`")
        if (label.isEmpty()) invalid("action $i `label` is empty")
        val norm = JSONObject().put("id", aid).put("label", label)
        when (a.opt("kind")) {
            "open" -> norm.put("kind", "open")
            "snooze" -> {
                val mins = intField(a, "snooze_minutes", 1L..1440L)
                    ?: invalid("snooze action $i needs `snooze_minutes` in 1..=1440")
                norm.put("kind", "snooze").put("snooze_minutes", mins.toInt())
            }
            else -> invalid("action $i `kind` must be \"open\" or \"snooze\"")
        }
        out.put(norm)
    }
    return out.toString()
}

/** An integer field in `range`, or null when absent / null. A fraction or a non-number is rejected. */
private fun intField(o: JSONObject, key: String, range: LongRange): Long? {
    if (o.isNull(key)) return null
    val n = o.get(key) as? Number ?: invalid("`$key` must be an integer")
    val d = n.toDouble()
    if (d.isNaN() || d.isInfinite() || d != Math.floor(d)) invalid("`$key` must be an integer")
    if (d < range.first.toDouble() || d > range.last.toDouble()) invalid("`$key` must be in ${range.first}..=${range.last}")
    return n.toLong()
}

private fun utf8Len(s: String): Int = s.toByteArray(Charsets.UTF_8).size

private val WALL_IN: DateTimeFormatter =
    DateTimeFormatter.ofPattern("uuuu-MM-dd HH:mm[:ss]").withResolverStyle(ResolverStyle.STRICT)
private val WALL_OUT: DateTimeFormatter = DateTimeFormatter.ofPattern("uuuu-MM-dd HH:mm:ss")

private fun parseWall(s: String): LocalDateTime? = try {
    LocalDateTime.parse(s.trim(), WALL_IN)
} catch (e: java.time.format.DateTimeParseException) {
    null
}

/** A wall-clock time in `zone`: a spring-forward gap → the first instant after it (the transition);
 *  a fall-back overlap → the earlier instant. */
private fun resolveWall(ldt: LocalDateTime, zone: ZoneId): Long {
    val rules = zone.rules
    if (rules.getValidOffsets(ldt).isEmpty()) {
        // ZonedDateTime.of would shift by the gap's length (02:30 → 03:30); the spec wants 03:00.
        rules.getTransition(ldt)?.let { return it.instant.toEpochMilli() }
    }
    return ZonedDateTime.of(ldt, zone).toInstant().toEpochMilli() // overlap → the earlier offset
}

/** "YYYY-MM-DD HH:MM[:SS]" in `zone` → epoch ms (gap → after it, overlap → earlier), null if malformed. */
fun wallToEpochMs(wall: String, zone: ZoneId): Long? = parseWall(wall)?.let { resolveWall(it, zone) }

/** Epoch ms → "YYYY-MM-DD HH:MM:SS" in `zone` (listPending, events). */
fun reminderLocalTime(ms: Long, zone: ZoneId): String =
    java.time.Instant.ofEpochMilli(ms).atZone(zone).format(WALL_OUT)

enum class RearmDecision { ARM, POST, DROP }

/**
 * What a re-arm (boot, app update, time or zone change, plugin init) does with one entry. A wall-clock
 * reminder (not a snooze) is re-computed in `zone` first. Then:
 * - a future one is armed;
 * - an overdue one whose alarm is still alive (no reboot or force-stop cleared it) is merely late, as
 *   an inexact alarm may be: it is re-armed and fires at once, and `late_minutes` never applies to it;
 * - an overdue one whose alarm is gone is posted now if late by at most `late_minutes`, else dropped.
 */
fun rearmDecision(r: Reminder, nowMs: Long, zone: ZoneId, alarmAlive: Boolean): Pair<RearmDecision, Reminder> {
    val updated = if (r.wall != null && !r.snoozed) {
        wallToEpochMs(r.wall, zone)?.let { r.copy(fireAtMs = it) } ?: r
    } else r
    val decision = when {
        updated.fireAtMs > nowMs -> RearmDecision.ARM
        alarmAlive -> RearmDecision.ARM
        nowMs - updated.fireAtMs <= updated.lateMinutes * 60_000L -> RearmDecision.POST
        else -> RearmDecision.DROP
    }
    return decision to updated
}

/**
 * Whether the firing receiver posts entry `r` now. An inexact alarm never fires early, so an entry not
 * yet due belongs to a replacement scheduled after this (older) alarm was already in flight: it is left
 * for its own alarm.
 */
fun receiverShouldPost(r: Reminder, nowMs: Long): Boolean = r.fireAtMs <= nowMs

/**
 * The registry entry a snooze writes (NotificationActionReceiver): the reminder its button carries, now
 * `snoozed`, due `minutes` from `nowMs`, with no `wall` (a time-zone change never moves a snooze). The
 * firing receiver removed the entry when it posted, so an `existing` entry is a newer schedule of the id
 * (the app re-planned it): that one wins and nothing is written (null).
 */
fun snoozeEntry(existing: Reminder?, carried: Reminder, nowMs: Long, minutes: Int): Reminder? {
    if (existing != null) return null
    return carried.copy(snoozed = true, fireAtMs = nowMs + minutes * 60_000L, wall = null)
}

/** The persisted reminders. Every write is `commit()`ed (on disk before the alarm is set). */
object ReminderRegistry {
    private const val PREFS = "mobiler_notifications"
    private const val PREFIX = "r."

    private fun prefs(ctx: Context) = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    @Synchronized fun put(ctx: Context, r: Reminder) {
        prefs(ctx).edit().putString(PREFIX + r.id, r.toJson()).commit()
    }

    @Synchronized fun get(ctx: Context, id: Int): Reminder? =
        prefs(ctx).getString(PREFIX + id, null)?.let { Reminder.fromJson(it) }

    @Synchronized fun remove(ctx: Context, id: Int) {
        prefs(ctx).edit().remove(PREFIX + id).commit()
    }

    @Synchronized fun all(ctx: Context): List<Reminder> =
        prefs(ctx).all.entries
            .filter { it.key.startsWith(PREFIX) }
            .mapNotNull { (it.value as? String)?.let { s -> Reminder.fromJson(s) } }

    /** Removes every reminder entry (other keys in the file, e.g. the Task 3 event buffer, stay). */
    @Synchronized fun clear(ctx: Context) {
        val p = prefs(ctx)
        val e = p.edit()
        p.all.keys.filter { it.startsWith(PREFIX) }.forEach { e.remove(it) }
        e.commit()
    }
}
