package rs.mirui.android;

import android.content.Context;
import android.os.CancellationSignal;
import java.io.IOException;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.ExecutionException;
import java.util.concurrent.FutureTask;
import java.util.concurrent.ThreadPoolExecutor;
import java.util.concurrent.TimeUnit;

/** One native Surface's asynchronous clipboard operation. */
public final class ClipboardSession {
    private final Context context;
    private final ThreadPoolExecutor executor;
    private FutureTask<ClipboardPayload> pending;
    private CancellationSignal cancellation;
    private boolean closed;

    public ClipboardSession(Context context) {
        this.context = context.getApplicationContext();
        executor = new ThreadPoolExecutor(0, 1, 30, TimeUnit.SECONDS, new ArrayBlockingQueue<>(1), runnable -> {
            Thread thread = new Thread(runnable, "mirui-clipboard");
            thread.setDaemon(true);
            return thread;
        });
    }

    public void write(ClipboardPayload data) {
        start(data, true);
    }

    public void read() {
        start(null, false);
    }

    private void start(ClipboardPayload data, boolean write) {
        if (closed || pending != null) {
            throw new IllegalStateException("Clipboard session is unavailable");
        }
        CancellationSignal signal = new CancellationSignal();
        FutureTask<ClipboardPayload> task = new FutureTask<>(() -> {
            signal.throwIfCanceled();
            if (write) {
                ClipboardBridge.write(context, data, signal);
                return null;
            }
            return ClipboardBridge.read(context, signal);
        });
        executor.execute(task);
        cancellation = signal;
        pending = task;
    }

    public boolean isDone() {
        return pending != null && pending.isDone();
    }

    public ClipboardPayload take() throws IOException {
        if (!isDone()) {
            throw new IllegalStateException("Clipboard operation has not completed");
        }
        FutureTask<ClipboardPayload> task = pending;
        pending = null;
        cancellation = null;
        try {
            return task.get();
        } catch (InterruptedException interrupted) {
            Thread.currentThread().interrupt();
            throw new IOException("Clipboard operation interrupted", interrupted);
        } catch (ExecutionException failure) {
            Throwable cause = failure.getCause();
            if (cause instanceof IOException) {
                throw (IOException) cause;
            }
            if (cause instanceof RuntimeException) {
                throw (RuntimeException) cause;
            }
            if (cause instanceof Error) {
                throw (Error) cause;
            }
            throw new IOException("Clipboard operation failed", cause);
        }
    }

    public void close() {
        closed = true;
        if (cancellation != null) {
            cancellation.cancel();
        }
        if (pending != null) {
            pending.cancel(false);
        }
        pending = null;
        cancellation = null;
        executor.shutdown();
    }
}
