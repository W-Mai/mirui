package rs.mirui.android;

import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertThrows;
import static org.junit.Assert.assertTrue;

import android.content.Context;
import android.content.ContextWrapper;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.platform.app.InstrumentationRegistry;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import org.junit.Test;
import org.junit.runner.RunWith;

@RunWith(AndroidJUnit4.class)
public final class ClipboardSessionTest {
    private static final class DeniedContext extends ContextWrapper {
        final CountDownLatch entered = new CountDownLatch(1);
        final CountDownLatch release;

        DeniedContext(boolean block) {
            super(InstrumentationRegistry.getInstrumentation().getTargetContext());
            release = new CountDownLatch(block ? 1 : 0);
        }

        @Override
        public Context getApplicationContext() {
            return this;
        }

        @Override
        public Object getSystemService(String name) {
            if (!CLIPBOARD_SERVICE.equals(name)) {
                throw new AssertionError("Unexpected service lookup");
            }
            entered.countDown();
            try {
                if (!release.await(5, TimeUnit.SECONDS)) {
                    throw new AssertionError("Clipboard test did not release the worker");
                }
            } catch (InterruptedException interrupted) {
                Thread.currentThread().interrupt();
                throw new AssertionError(interrupted);
            }
            throw new SecurityException("Isolated clipboard denial");
        }
    }

    @Test
    public void failuresAreDeliveredOnceAndTheSessionCanBeReused() throws Exception {
        ClipboardSession session = new ClipboardSession(new DeniedContext(false));
        try {
            for (int operation = 0; operation < 2; operation++) {
                session.read();
                assertThrows(IllegalStateException.class, session::read);
                long deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(5);
                while (!session.isDone() && System.nanoTime() < deadline) {
                    Thread.sleep(1);
                }
                assertTrue(session.isDone());
                assertThrows(SecurityException.class, session::take);
                assertFalse(session.isDone());
                assertThrows(IllegalStateException.class, session::take);
            }
        } finally {
            session.close();
        }
    }

    @Test
    public void closeDoesNotJoinBlockedHostWorkOrDeliverItsLateResult() throws Exception {
        DeniedContext context = new DeniedContext(true);
        ClipboardSession session = new ClipboardSession(context);
        try {
            session.read();
            assertTrue(context.entered.await(5, TimeUnit.SECONDS));
            long started = System.nanoTime();
            session.close();
            assertTrue(System.nanoTime() - started < TimeUnit.SECONDS.toNanos(1));
            assertFalse(session.isDone());
            assertThrows(IllegalStateException.class, session::take);
            assertThrows(IllegalStateException.class, session::read);
            session.close();
        } finally {
            context.release.countDown();
            session.close();
        }
    }
}
