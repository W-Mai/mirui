package rs.mirui.android;

import static org.junit.Assert.assertArrayEquals;
import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertThrows;

import android.content.ClipData;
import android.content.ClipDescription;
import android.content.ContentResolver;
import android.content.Context;
import android.content.res.AssetFileDescriptor;
import android.net.Uri;
import android.os.CancellationSignal;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import androidx.test.platform.app.InstrumentationRegistry;
import java.io.FileNotFoundException;
import java.io.InputStream;
import org.junit.Test;
import org.junit.runner.RunWith;

@RunWith(AndroidJUnit4.class)
public final class ClipboardProviderTest {
    private final Context context = InstrumentationRegistry.getInstrumentation().getTargetContext();

    @Test
    public void resolverPreservesItemsAndDoesNotImportTheTransportUri() throws Exception {
        SnapshotStore store = ClipboardProvider.store(context);
        String[][] types = {{"text/plain", "application/x-test"}, {"image/png"}};
        byte[][][] values = {{{65}, {0, -1, 0}}, {{}}};
        String token = store.prepare(types, values);
        try {
            Uri first = ClipboardProvider.uri(context, token, 0);
            Uri second = ClipboardProvider.uri(context, token, 1);
            ClipData clip = new ClipData(new ClipDescription("", new String[] {"text/plain", "application/x-test", "image/png"}),
                    new ClipData.Item("A", null, null, first));
            clip.addItem(new ClipData.Item(second));
            ClipboardPayload result = ClipboardBridge.readSnapshot(context.getContentResolver(), clip, new CancellationSignal());
            assertArrayEquals(types[0], result.types[0]);
            assertArrayEquals(types[1], result.types[1]);
            assertArrayEquals(values[0][1], result.values[0][1]);
            assertArrayEquals(values[1][0], result.values[1][0]);
            assertThrows(FileNotFoundException.class, () -> context.getContentResolver().openFileDescriptor(first, "rw"));
        } finally {
            store.retainOnly(null);
        }
    }

    @Test
    public void resolverStreamsRetainFilesAfterReplacement() throws Exception {
        SnapshotStore store = ClipboardProvider.store(context);
        String token = store.prepare(new String[][] {{"application/x-test"}}, new byte[][][] {{{1, 0, 2}}});
        Uri uri = ClipboardProvider.uri(context, token, 0);
        ContentResolver resolver = context.getContentResolver();
        try (AssetFileDescriptor descriptor = resolver.openTypedAssetFileDescriptor(uri, "application/x-test", null);
                InputStream input = descriptor.createInputStream()) {
            store.retainOnly(null);
            assertEquals(3, descriptor.getLength());
            assertEquals(1, input.read());
            assertEquals(0, input.read());
            assertEquals(2, input.read());
            assertEquals(-1, input.read());
            assertThrows(FileNotFoundException.class, () -> resolver.openTypedAssetFileDescriptor(uri, "*/*", null));
        }
    }

    @Test
    public void oneMissingRepresentationRejectsTheSnapshot() throws Exception {
        ClipData clip = new ClipData("", new String[] {"text/plain"}, new ClipData.Item("kept only if complete"));
        clip.addItem(new ClipData.Item(ClipboardProvider.uri(context, "00000000-0000-0000-0000-000000000000", 0)));
        assertThrows(UnsupportedOperationException.class,
                () -> ClipboardBridge.readSnapshot(context.getContentResolver(), clip, new CancellationSignal()));
    }
}
