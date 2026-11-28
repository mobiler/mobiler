package dev.mobiler.barbershop

import java.util.Collections
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.joinAll
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// The notifications plugin's lane (ReminderLane in ReminderRegistry.kt): the registry ops run off the
// caller's thread, one at a time, in the order they were started. Core.kt starts each plugin call on
// the main dispatcher in issue order (ADR-0065); runBlocking's single-thread event loop stands in.
class ReminderLaneTest {
    @Test fun callsRunOnOneLaneThreadInLaunchOrder() = runBlocking {
        val caller = Thread.currentThread()
        val order = Collections.synchronizedList(ArrayList<Int>())
        val threads = Collections.synchronizedSet(HashSet<Thread>())
        // A post (plugin init's re-arm) queued first runs before every call started after it.
        ReminderLane.post { order.add(0) }
        val jobs = (1..200).map { i ->
            launch {
                val answer = ReminderLane.run {
                    threads.add(Thread.currentThread())
                    if (i % 7 == 0) Thread.sleep(1) // a slow call must not let a later one overtake it
                    order.add(i)
                    i * 2
                }
                assertEquals(i * 2, answer)
            }
        }
        jobs.joinAll()
        assertEquals((0..200).toList(), order)
        assertEquals("one lane thread", 1, threads.size)
        assertFalse("never the caller's (main) thread", threads.contains(caller))
    }

    @Test fun aQueuedCallStillRunsWhenTheScopeIsCancelled() {
        val gate = CountDownLatch(1)
        ReminderLane.post { gate.await(5, TimeUnit.SECONDS) } // hold the lane so the calls below queue
        val order = Collections.synchronizedList(ArrayList<Int>())
        // A re-plan: cancelAll then 40 schedules, each entering the lane before its first suspension
        // (UNDISPATCHED, as Main.immediate does), then the screen's scope is cancelled.
        val scope = CoroutineScope(Job() + Dispatchers.Unconfined)
        for (i in 0..40) scope.launch(start = CoroutineStart.UNDISPATCHED) { ReminderLane.run { order.add(i) } }
        scope.coroutineContext[Job]!!.cancel()
        gate.countDown()
        val drained = CountDownLatch(1)
        ReminderLane.post { drained.countDown() }
        assertTrue(drained.await(5, TimeUnit.SECONDS))
        assertEquals("every queued call ran, in order", (0..40).toList(), order)
    }
}
