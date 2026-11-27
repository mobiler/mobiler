package dev.mobiler.barbershop

import java.time.OffsetDateTime
import java.time.ZoneId
import java.time.ZonedDateTime
import org.json.JSONArray
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

// The notifications plugin's pure logic (ReminderRegistry.kt): input validation (spec §4.2),
// wall-clock resolution, the registry JSON (§4.3) and the re-arm decision. Runs on the plain JVM.
class ReminderLogicTest {
    private val zone: ZoneId = ZoneId.of("Europe/Belgrade")
    private val now: Long = ZonedDateTime.of(2027, 1, 1, 12, 0, 0, 0, zone).toInstant().toEpochMilli()

    private fun parse(json: String) = parseSchedule(json, now, zone)
    private fun error(json: String): String =
        parse(json).exceptionOrNull()?.message ?: error("expected a failure for $json")

    private fun actions(vararg a: String) = a.joinToString(",", "[", "]")
    private fun snooze(id: String, minutes: Int) =
        """{"id":"$id","label":"Snooze","kind":"snooze","snooze_minutes":$minutes}"""
    private fun open(id: String) = """{"id":"$id","label":"Open","kind":"open"}"""

    @Test fun missingIdDefaultsTo1() {
        val r = parse("""{"title":"t","body":"b","after_seconds":10}""").getOrThrow()
        assertEquals(1, r.id)
        assertEquals(now + 10_000, r.fireAtMs)
        assertNull(r.wall)
        assertFalse(r.snoozed)
    }

    @Test fun presentButBadIdIsInvalid() {
        assertTrue(error("""{"id":0,"after_seconds":1}""").startsWith("invalid_input"))
        assertTrue(error("""{"id":2147483648,"after_seconds":1}""").startsWith("invalid_input"))
        assertTrue(error("""{"id":1.5,"after_seconds":1}""").startsWith("invalid_input"))
        assertTrue(error("""{"id":"7","after_seconds":1}""").startsWith("invalid_input"))
        assertEquals(2147483647, parse("""{"id":2147483647,"after_seconds":1}""").getOrThrow().id)
    }

    @Test fun bothAtAndAfterIsInvalid() {
        assertTrue(error("""{"id":3,"at":"2027-01-02 09:00","after_seconds":5}""").startsWith("invalid_input"))
        assertTrue(error("""{"id":3,"title":"neither"}""").startsWith("invalid_input"))
        assertTrue(error("not json").startsWith("invalid_input"))
    }

    @Test fun atIsLocalWallClock() {
        val r = parse("""{"id":3,"at":"2027-01-02 09:45"}""").getOrThrow()
        assertEquals("2027-01-02 09:45:00", r.wall)
        assertEquals(ZonedDateTime.of(2027, 1, 2, 9, 45, 0, 0, zone).toInstant().toEpochMilli(), r.fireAtMs)
        assertEquals("2027-01-02 09:45:30", parse("""{"id":3,"at":"2027-01-02 09:45:30"}""").getOrThrow().wall)
        assertTrue(error("""{"id":3,"at":"2027-01-02T09:45"}""").startsWith("invalid_input"))
        assertTrue(error("""{"id":3,"at":"2027-02-30 09:45"}""").startsWith("invalid_input"))
    }

    @Test fun pastAtIsPast() {
        assertEquals("past", error("""{"id":3,"at":"2027-01-01 11:59"}"""))
        assertEquals("past", error("""{"id":3,"at":"2027-01-01 12:00"}"""))
    }

    @Test fun fourActionsInvalid() {
        val three = actions(open("a"), open("b"), snooze("c", 10))
        assertNotNull(parse("""{"id":3,"after_seconds":5,"actions":$three}""").getOrNull())
        val four = actions(open("a"), open("b"), snooze("c", 10), open("d"))
        assertTrue(error("""{"id":3,"after_seconds":5,"actions":$four}""").startsWith("invalid_input"))
    }

    @Test fun snoozeMinutesBounds() {
        for (bad in listOf(0, 1441)) {
            val a = actions(snooze("s", bad))
            assertTrue(error("""{"id":3,"after_seconds":5,"actions":$a}""").startsWith("invalid_input"))
        }
        for (ok in listOf(1, 1440)) {
            val a = actions(snooze("s", ok))
            assertNotNull(parse("""{"id":3,"after_seconds":5,"actions":$a}""").getOrNull())
        }
        val missing = """[{"id":"s","label":"Snooze","kind":"snooze"}]"""
        assertTrue(error("""{"id":3,"after_seconds":5,"actions":$missing}""").startsWith("invalid_input"))
    }

    @Test fun badActionFieldsInvalid() {
        val unknownKind = """[{"id":"x","label":"X","kind":"reply"}]"""
        assertTrue(error("""{"id":3,"after_seconds":5,"actions":$unknownKind}""").startsWith("invalid_input"))
        val emptyLabel = """[{"id":"x","label":"","kind":"open"}]"""
        assertTrue(error("""{"id":3,"after_seconds":5,"actions":$emptyLabel}""").startsWith("invalid_input"))
        val emptyId = """[{"id":"","label":"X","kind":"open"}]"""
        assertTrue(error("""{"id":3,"after_seconds":5,"actions":$emptyId}""").startsWith("invalid_input"))
        val longId = actions(open("é".repeat(32) + "a")) // 65 UTF-8 bytes
        assertTrue(error("""{"id":3,"after_seconds":5,"actions":$longId}""").startsWith("invalid_input"))
        assertNotNull(parse("""{"id":3,"after_seconds":5,"actions":${actions(open("é".repeat(32)))}}""").getOrNull())
    }

    @Test fun duplicateActionIdInvalid() {
        val dup = actions(open("a"), snooze("a", 10))
        assertTrue(error("""{"id":3,"after_seconds":5,"actions":$dup}""").startsWith("invalid_input"))
    }

    @Test fun payloadOver1024BytesInvalid() {
        val ok = "é".repeat(512) // 1024 UTF-8 bytes, 512 chars
        assertEquals(ok, parse("""{"id":3,"after_seconds":5,"payload":"$ok"}""").getOrThrow().payload)
        val tooLong = ok + "a" // 1025 bytes
        assertTrue(error("""{"id":3,"after_seconds":5,"payload":"$tooLong"}""").startsWith("invalid_input"))
        assertTrue(error("""{"id":3,"after_seconds":5,"payload":42}""").startsWith("invalid_input"))
    }

    @Test fun lateMinutesDefault1440() {
        assertEquals(1440, parse("""{"id":3,"after_seconds":5}""").getOrThrow().lateMinutes)
        assertEquals(0, parse("""{"id":3,"after_seconds":5,"late_minutes":0}""").getOrThrow().lateMinutes)
        assertEquals(10080, parse("""{"id":3,"after_seconds":5,"late_minutes":10080}""").getOrThrow().lateMinutes)
        assertTrue(error("""{"id":3,"after_seconds":5,"late_minutes":10081}""").startsWith("invalid_input"))
        assertTrue(error("""{"id":3,"after_seconds":5,"late_minutes":-1}""").startsWith("invalid_input"))
    }

    @Test fun gapResolvesAfter() {
        // Belgrade springs forward 02:00 → 03:00 on 2027-03-28: 02:30 doesn't exist, fires at 03:00.
        val expected = ZonedDateTime.of(2027, 3, 28, 3, 0, 0, 0, zone).toInstant().toEpochMilli()
        assertEquals(expected, wallToEpochMs("2027-03-28 02:30", zone))
        assertEquals(expected, wallToEpochMs("2027-03-28 02:30:00", zone))
    }

    @Test fun overlapResolvesEarlier() {
        // Belgrade falls back 03:00 → 02:00 on 2027-10-31: 02:30 happens twice, the +02:00 one first.
        val expected = OffsetDateTime.parse("2027-10-31T02:30:00+02:00").toInstant().toEpochMilli()
        assertEquals(expected, wallToEpochMs("2027-10-31 02:30", zone))
    }

    @Test fun reminderJsonRoundTripsQuotesNewlinesNonAscii() {
        val tricky = "He said \"hi\"\\ \n\tnew line — čćžšđ 日本 🚀   </script>"
        val acts = JSONArray().put(
            org.json.JSONObject().put("id", "o\"1").put("label", tricky).put("kind", "open")
        ).toString()
        val r = Reminder(
            id = 42, title = tricky, body = "b$tricky", fireAtMs = now + 5, wall = "2027-01-02 09:45:00",
            payload = tricky, actionsJson = acts, lateMinutes = 30, snoozed = true,
        )
        val back = Reminder.fromJson(r.toJson())
        assertEquals(r, back)
        assertEquals(tricky, back!!.payload)
        // Through the parser too: a JSON-escaped payload comes out byte-exact.
        val input = org.json.JSONObject().put("id", 5).put("after_seconds", 3).put("payload", tricky)
            .put("title", tricky).toString()
        val parsed = parse(input).getOrThrow()
        assertEquals(tricky, parsed.payload)
        assertEquals(tricky, parsed.title)
        assertEquals(parsed, Reminder.fromJson(parsed.toJson()))
        // Optional fields absent stay absent.
        val bare = Reminder(7, "", "", 1L, null, null, null, 1440, false)
        assertEquals(bare, Reminder.fromJson(bare.toJson()))
        assertNull(Reminder.fromJson("{broken"))
    }

    @Test fun actionsStoredAsData() {
        val a = actions(snooze("snooze", 10), open("open"))
        val r = parse("""{"id":3,"after_seconds":5,"actions":$a}""").getOrThrow()
        val stored = JSONArray(r.actionsJson)
        assertEquals(2, stored.length())
        assertEquals(10, stored.getJSONObject(0).getInt("snooze_minutes"))
        assertEquals("open", stored.getJSONObject(1).getString("kind"))
        assertNull(parse("""{"id":3,"after_seconds":5}""").getOrThrow().actionsJson)
    }

    // ---- re-arm decision (boot, update, time / zone change, plugin init) ----

    private fun wallReminder(wall: String, late: Int = 1440, snoozed: Boolean = false) =
        Reminder(9, "t", "b", wallToEpochMs(wall, zone)!!, wall, null, null, late, snoozed)

    @Test fun rearmRecomputesWallInNewZone() {
        val r = wallReminder("2027-01-02 09:00:00")
        val london = ZoneId.of("Europe/London")
        val (decision, updated) = rearmDecision(r, now, london, alarmAlive = false)
        assertEquals(RearmDecision.ARM, decision)
        assertEquals(ZonedDateTime.of(2027, 1, 2, 9, 0, 0, 0, london).toInstant().toEpochMilli(), updated.fireAtMs)
        assertEquals(r.wall, updated.wall)
    }

    @Test fun rearmPostsLateWithinWindowAndDropsTooLate() {
        val r = wallReminder("2027-01-01 11:40:00", late = 30) // 20 minutes late
        assertEquals(RearmDecision.POST, rearmDecision(r, now, zone, alarmAlive = false).first)
        val old = wallReminder("2027-01-01 11:00:00", late = 30) // 60 minutes late
        assertEquals(RearmDecision.DROP, rearmDecision(old, now, zone, alarmAlive = false).first)
        val zero = wallReminder("2027-01-01 11:59:00", late = 0)
        assertEquals(RearmDecision.DROP, rearmDecision(zero, now, zone, alarmAlive = false).first)
    }

    @Test fun rearmKeepsSnoozedAndRelativeTimes() {
        val london = ZoneId.of("Europe/London")
        val snoozed = wallReminder("2027-01-01 11:00:00").copy(fireAtMs = now + 600_000, snoozed = true)
        val (d, u) = rearmDecision(snoozed, now, london, alarmAlive = false)
        assertEquals(RearmDecision.ARM, d)
        assertEquals(now + 600_000, u.fireAtMs)
        val relative = Reminder(4, "t", "b", now + 1000, null, null, null, 1440, false)
        assertEquals(relative, rearmDecision(relative, now, london, alarmAlive = false).second)
    }

    @Test fun rearmLeavesALiveOverdueAlarmToFire() {
        // Merely delayed (Doze, inexact): the alarm survived, so late_minutes doesn't apply; re-armed.
        val late = wallReminder("2027-01-01 11:00:00", late = 0) // 60 minutes past, window 0
        assertEquals(RearmDecision.ARM, rearmDecision(late, now, zone, alarmAlive = true).first)
        assertEquals(RearmDecision.DROP, rearmDecision(late, now, zone, alarmAlive = false).first)
        val relative = Reminder(4, "t", "b", now - 1000, null, null, null, 0, false)
        assertEquals(RearmDecision.ARM, rearmDecision(relative, now, zone, alarmAlive = true).first)
        // A live wall alarm is still re-computed for the new zone and re-armed at the new instant.
        val london = ZoneId.of("Europe/London")
        val future = wallReminder("2027-01-02 09:00:00")
        val (d, u) = rearmDecision(future, now, london, alarmAlive = true)
        assertEquals(RearmDecision.ARM, d)
        assertEquals(ZonedDateTime.of(2027, 1, 2, 9, 0, 0, 0, london).toInstant().toEpochMilli(), u.fireAtMs)
    }

    @Test fun receiverIgnoresAnEntryNotYetDue() {
        val due = Reminder(4, "t", "b", now, null, null, null, 1440, false)
        assertTrue(receiverShouldPost(due, now))
        assertTrue(receiverShouldPost(due.copy(fireAtMs = now - 3_600_000), now)) // Doze-delayed: still posts
        assertFalse(receiverShouldPost(due.copy(fireAtMs = now + 1), now))       // a replacement, later
    }

    @Test fun localTimeFormat() {
        assertEquals("2027-01-01 12:00:00", reminderLocalTime(now, zone))
    }

    // A snooze (NotificationActionReceiver) writes a new entry from the reminder its button carries.
    @Test fun snoozeEntryIsASnoozedCopyDueInMinutes() {
        val carried = parse("""{"id":9,"title":"T","body":"B","at":"2027-01-02 09:00","payload":"p\n\"q\"","late_minutes":30,""" +
            """"actions":${actions(snooze("s", 5), open("o"))}}""").getOrThrow()
        val s = snoozeEntry(null, carried, now, 5)!!
        assertEquals(carried.copy(snoozed = true, fireAtMs = now + 5 * 60_000L, wall = null), s)
        // A snooze isn't a wall-clock reminder: a re-arm (zone change) never moves it.
        assertEquals(RearmDecision.ARM to s, rearmDecision(s, now, ZoneId.of("Europe/London"), alarmAlive = false))
    }

    @Test fun snoozeLeavesANewerScheduleOfTheIdAlone() {
        val carried = parse("""{"id":9,"after_seconds":1}""").getOrThrow()
        val replanned = parse("""{"id":9,"after_seconds":3600}""").getOrThrow()
        assertNull(snoozeEntry(replanned, carried, now, 5))
    }
}
