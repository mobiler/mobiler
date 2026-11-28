package dev.mobiler.barbershop

import java.time.ZoneId
import java.time.ZonedDateTime
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

// The notifications plugin's event buffer (ReminderEvents.kt, spec §4.5): a persisted, ordered list of
// at most 100 events, flushed in order into the attached sink, each removed only after the sink returns.
// Runs on the plain JVM with an in-memory store standing in for SharedPreferences.
class ReminderEventsTest {
    /** The SharedPreferences stand-in: one string value, as the real store keeps it under "events". */
    private class FakeStore : EventStore {
        val map = HashMap<String, String>()
        override fun load(): String? = map["events"]
        override fun save(value: String) { map["events"] = value }
        fun stored(): List<String> = map["events"]?.let { s ->
            val a = JSONArray(s)
            (0 until a.length()).map { a.getString(it) }
        } ?: emptyList()
    }

    @Test fun bufferCapsAt100DropsOldest() {
        val store = FakeStore()
        val buf = EventBuffer(store)
        for (i in 1..105) buf.emit("e$i")
        val stored = store.stored()
        assertEquals(100, stored.size)
        assertEquals("e6", stored.first())
        assertEquals("e105", stored.last())

        // A fresh buffer over the same store (a new process) sees the same 100, in order.
        val got = ArrayList<String>()
        EventBuffer(store).attach { got.add(it) }
        assertEquals((6..105).map { "e$it" }, got)
    }

    @Test fun flushInOrderThenEmpty() {
        val store = FakeStore()
        val buf = EventBuffer(store)
        buf.emit("a"); buf.emit("b"); buf.emit("c")
        val got = ArrayList<String>()
        val id = buf.attach { got.add(it) }
        assertEquals(listOf("a", "b", "c"), got)
        assertTrue(store.stored().isEmpty())

        // While attached, events go straight to the sink and nothing stays buffered.
        buf.emit("d")
        assertEquals(listOf("a", "b", "c", "d"), got)
        assertTrue(store.stored().isEmpty())

        // After detach, events buffer again; a second attach gets only those (no duplicates).
        buf.detach(id)
        buf.emit("e")
        assertEquals(listOf("e"), store.stored())
        val again = ArrayList<String>()
        buf.attach { again.add(it) }
        assertEquals(listOf("e"), again)
    }

    @Test fun secondSubscriberReplacesFirst() {
        val buf = EventBuffer(FakeStore())
        val first = ArrayList<String>()
        val second = ArrayList<String>()
        val id1 = buf.attach { first.add(it) }
        buf.attach { second.add(it) }
        buf.emit("x")
        // The replaced subscriber's late detach must not detach the new one.
        buf.detach(id1)
        buf.emit("y")
        assertTrue(first.isEmpty())
        assertEquals(listOf("x", "y"), second)
    }

    @Test fun crashMidFlushRedeliversRemainder() {
        val store = FakeStore()
        val buf = EventBuffer(store)
        listOf("1", "2", "3", "4").forEach { buf.emit(it) }
        val got = ArrayList<String>()
        try {
            buf.attach {
                if (got.size == 2) throw IllegalStateException("process died")
                got.add(it)
            }
            fail("the sink's failure should propagate")
        } catch (e: IllegalStateException) {
            // expected
        }
        assertEquals(listOf("1", "2"), got)
        // The two delivered are gone; the one the sink failed on and the rest remain, in order.
        assertEquals(listOf("3", "4"), store.stored())

        // The failed sink is detached: a new event buffers instead of reaching it.
        buf.emit("5")
        assertEquals(listOf("1", "2"), got)

        // The next process re-delivers the remainder.
        val next = ArrayList<String>()
        EventBuffer(store).attach { next.add(it) }
        assertEquals(listOf("3", "4", "5"), next)
        assertTrue(store.stored().isEmpty())
    }

    @Test fun liveSinkFailureKeepsTheEventBuffered() {
        val store = FakeStore()
        val buf = EventBuffer(store)
        buf.attach { throw IllegalStateException("channel closed") }
        buf.emit("kept") // must not throw into a receiver
        assertEquals(listOf("kept"), store.stored())
    }

    @Test fun eventJsonShape() {
        val zone = ZoneId.of("Europe/Belgrade")
        val at = ZonedDateTime.of(2027, 3, 4, 9, 5, 7, 0, zone).toInstant().toEpochMilli()

        val full = JSONObject(reminderEventJson("snoozed", 7, "snooze", "p \"q\"\nčž", at, zone))
        assertEquals(setOf("type", "id", "action_id", "payload", "at"), full.keys().asSequence().toSet())
        assertEquals("snoozed", full.getString("type"))
        assertEquals(7, full.getInt("id"))
        assertEquals("snooze", full.getString("action_id"))
        assertEquals("p \"q\"\nčž", full.getString("payload"))
        assertEquals("2027-03-04 09:05:07", full.getString("at"))
        assertTrue(Regex("""\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}""").matches(full.getString("at")))

        val bare = JSONObject(reminderEventJson("opened", 3, null, null, at, zone))
        assertEquals(setOf("type", "id", "at"), bare.keys().asSequence().toSet())
        assertFalse(bare.has("action_id"))
        assertFalse(bare.has("payload"))
        // An empty payload is still a payload (the app passed one).
        assertTrue(JSONObject(reminderEventJson("dismissed", 3, null, "", at, zone)).has("payload"))
    }
}
