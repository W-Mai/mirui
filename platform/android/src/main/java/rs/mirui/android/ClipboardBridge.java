package rs.mirui.android;

import android.content.ClipData;
import android.content.ClipDescription;
import android.content.ClipboardManager;
import android.content.ContentResolver;
import android.content.Context;
import android.content.res.AssetFileDescriptor;
import android.net.Uri;
import android.os.CancellationSignal;
import android.os.SystemClock;
import android.system.ErrnoException;
import android.system.Os;
import android.system.OsConstants;
import android.system.StructPollfd;
import android.util.Log;
import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.util.ArrayList;
import java.util.LinkedHashSet;

/** Blocking host operations invoked only by the native clipboard worker. */
public final class ClipboardBridge {
    private static final Object WRITER = new Object();

    private ClipboardBridge() {}

    public static void write(Context context, ClipboardPayload payload, CancellationSignal cancellation)
            throws IOException {
        synchronized (WRITER) {
            cancellation.throwIfCanceled();
            ClipboardManager clipboard = manager(context);
            String[] text = new String[payload.types.length];
            String[] html = new String[payload.types.length];
            LinkedHashSet<String> types = new LinkedHashSet<>();
            for (int item = 0; item < payload.types.length; item++) {
                text[item] = payload.text(item, "text/plain");
                html[item] = payload.text(item, "text/html");
                for (String type : payload.types[item]) {
                    types.add(type);
                }
            }
            SnapshotStore store = ClipboardProvider.store(context);
            String token = store.prepare(payload.types, payload.values);
            ClipDescription description = new ClipDescription("", types.toArray(new String[0]));
            ClipData clip = null;
            for (int item = 0; item < payload.types.length; item++) {
                // Android requires plain text alongside a direct HTML representation.
                ClipData.Item value = new ClipData.Item(text[item], text[item] == null ? null : html[item],
                        null, ClipboardProvider.uri(context, token, item));
                if (clip == null) {
                    clip = new ClipData(description, value);
                } else {
                    clip.addItem(value);
                }
            }
            cancellation.throwIfCanceled();
            clipboard.setPrimaryClip(clip);
            ClipData accepted = clipboard.getPrimaryClip();
            if (accepted == null || accepted.getItemCount() != payload.types.length) {
                throw new IOException("Clipboard publication was not confirmed");
            }
            for (int item = 0; item < payload.types.length; item++) {
                if (!ClipboardProvider.uri(context, token, item).equals(accepted.getItemAt(item).getUri())) {
                    throw new IOException("Clipboard publication was replaced or rejected");
                }
            }
            try {
                store.retainOnly(token);
            } catch (IOException cleanup) {
                Log.w("mirui.clipboard", "Cannot retire old clipboard snapshot files", cleanup);
            }
        }
    }

    public static ClipboardPayload read(Context context, CancellationSignal cancellation) throws IOException {
        cancellation.throwIfCanceled();
        ClipData clip = manager(context).getPrimaryClip();
        if (clip == null || clip.getItemCount() == 0) {
            throw new IOException("Clipboard is unavailable");
        }
        return readSnapshot(context.getContentResolver(), clip, cancellation);
    }

    static ClipboardPayload readSnapshot(ContentResolver resolver, ClipData clip, CancellationSignal cancellation)
            throws IOException {
        long deadline = SystemClock.elapsedRealtime() + 10_000;
        String[][] types = new String[clip.getItemCount()][];
        byte[][][] values = new byte[clip.getItemCount()][][];
        for (int index = 0; index < clip.getItemCount(); index++) {
            cancellation.throwIfCanceled();
            ClipData.Item item = clip.getItemAt(index);
            ArrayList<String> names = new ArrayList<>();
            ArrayList<byte[]> bytes = new ArrayList<>();
            Uri uri = item.getUri();
            if (uri != null && ContentResolver.SCHEME_CONTENT.equals(uri.getScheme())) {
                String[] streams = resolver.getStreamTypes(uri, "*/*");
                if (streams == null || streams.length == 0) {
                    String type = resolver.getType(uri);
                    if (type != null && !type.startsWith("vnd.android.cursor.")) {
                        streams = new String[] {type};
                    }
                }
                if (streams == null || streams.length == 0) {
                    throw new UnsupportedOperationException("Content URI has no byte representation");
                }
                for (String type : streams) {
                    if (type == null) {
                        throw new IOException("Clipboard MIME type is unavailable");
                    }
                    if (!contains(names, type)) {
                        try (AssetFileDescriptor file = resolver.openTypedAssetFileDescriptor(uri, type, null, cancellation)) {
                            if (file == null) {
                                throw new IOException("Clipboard representation is unavailable");
                            }
                            names.add(type);
                            bytes.add(readBytes(file, cancellation, deadline));
                        }
                    }
                }
            } else if (uri != null) {
                names.add("text/uri-list");
                bytes.add(ClipboardPayload.encode(uri.toString()));
            }
            if (item.getText() != null && !contains(names, "text/plain")) {
                names.add("text/plain");
                bytes.add(ClipboardPayload.encode(item.getText()));
            }
            if (item.getHtmlText() != null && !contains(names, "text/html")) {
                names.add("text/html");
                bytes.add(ClipboardPayload.encode(item.getHtmlText()));
            }
            if (names.isEmpty()) {
                throw new UnsupportedOperationException("Clipboard item has no byte representation");
            }
            types[index] = names.toArray(new String[0]);
            values[index] = bytes.toArray(new byte[0][]);
        }
        cancellation.throwIfCanceled();
        return new ClipboardPayload(types, values);
    }

    private static byte[] readBytes(AssetFileDescriptor file, CancellationSignal cancellation, long deadline)
            throws IOException {
        try (InputStream input = file.createInputStream()) {
            ByteArrayOutputStream output = new ByteArrayOutputStream();
            byte[] buffer = new byte[8192];
            StructPollfd ready = new StructPollfd();
            ready.fd = file.getFileDescriptor();
            ready.events = (short) OsConstants.POLLIN;
            StructPollfd[] poll = {ready};
            while (file.getDeclaredLength() < 0 || output.size() < file.getDeclaredLength()) {
                cancellation.throwIfCanceled();
                long remaining = deadline - SystemClock.elapsedRealtime();
                if (remaining <= 0) {
                    throw new IOException("Clipboard transfer timed out");
                }
                try {
                    if (Os.poll(poll, (int) Math.min(remaining, 50)) == 0) {
                        continue;
                    }
                } catch (ErrnoException failure) {
                    if (failure.errno == OsConstants.EINTR) {
                        continue;
                    }
                    throw new IOException("Clipboard stream poll failed", failure);
                }
                int count = input.read(buffer);
                if (count == -1) {
                    break;
                }
                output.write(buffer, 0, count);
            }
            cancellation.throwIfCanceled();
            if (file.getDeclaredLength() >= 0 && output.size() != file.getDeclaredLength()) {
                throw new IOException("Incomplete clipboard representation");
            }
            return output.toByteArray();
        }
    }

    private static boolean contains(ArrayList<String> types, String type) {
        for (String present : types) {
            if (present.equalsIgnoreCase(type)) {
                return true;
            }
        }
        return false;
    }

    private static ClipboardManager manager(Context context) throws IOException {
        ClipboardManager clipboard = context.getSystemService(ClipboardManager.class);
        if (clipboard == null) {
            throw new IOException("Clipboard service is unavailable");
        }
        return clipboard;
    }
}
