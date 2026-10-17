package rs.mirui.android;

import static org.junit.Assert.assertArrayEquals;
import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertThrows;
import static org.junit.Assert.assertTrue;

import java.io.File;
import java.io.FileInputStream;
import java.io.IOException;
import java.nio.file.Files;
import java.util.UUID;
import org.junit.Rule;
import org.junit.Test;
import org.junit.rules.TemporaryFolder;

public final class SnapshotStoreTest {
    @Rule public final TemporaryFolder temporary = new TemporaryFolder();

    @Test
    public void immutableItemsAndRepresentationsSurviveStoreRecreation() throws Exception {
        File root = temporary.newFolder("snapshots");
        SnapshotStore store = new SnapshotStore(root);
        String[][] types = {{"text/plain", "application/x-test"}, {"image/png"}};
        byte[][][] values = {{{65}, {0, -1, 0}}, {{}}};
        String token = store.prepare(types, values);
        values[0][1][1] = 42;
        types[0][1] = "text/html";
        SnapshotStore restarted = new SnapshotStore(root);
        assertArrayEquals(new String[] {"text/plain", "application/x-test"}, restarted.types(token, 0));
        assertArrayEquals(new String[] {"image/png"}, restarted.types(token, 1));
        try (FileInputStream file = restarted.open(token, 0, "APPLICATION/X-TEST")) {
            assertArrayEquals(new byte[] {0, -1, 0}, file.readAllBytes());
        }
        try (FileInputStream file = restarted.open(token, 1, "image/*")) {
            assertEquals(-1, file.read());
        }
    }

    @Test
    public void retirementPreservesOpenDescriptorsAndOnlyRemovesOwnedSnapshots() throws Exception {
        File root = temporary.newFolder("snapshots");
        SnapshotStore store = new SnapshotStore(root);
        String old = store.prepare(new String[][] {{"text/plain"}}, new byte[][][] {{{1, 2, 3}}});
        String current = store.prepare(new String[][] {{"text/plain"}}, new byte[][][] {{{9}}});
        File unrelated = new File(root, "unrelated");
        Files.write(unrelated.toPath(), new byte[] {7});
        File incomplete = new File(root, ".prepare-" + UUID.randomUUID());
        Files.createDirectory(incomplete.toPath());
        try (FileInputStream accepted = store.open(old, 0, "text/plain")) {
            store.retainOnly(current);
            assertArrayEquals(new byte[] {1, 2, 3}, accepted.readAllBytes());
            assertThrows(IOException.class, () -> store.open(old, 0, "text/plain"));
        }
        assertTrue(unrelated.exists());
        assertFalse(incomplete.exists());
        assertArrayEquals(new String[] {"text/plain"}, store.types(current, 0));
        store.retainOnly(null);
        assertFalse(new File(root, current).exists());
    }

    @Test
    public void allRepresentationsAreValidatedBeforeAnyStorageChange() throws Exception {
        File root = new File(temporary.getRoot(), "not-created");
        SnapshotStore store = new SnapshotStore(root);
        assertThrows(IllegalArgumentException.class, () -> store.prepare(
                new String[][] {{"text/plain"}, {"text/html", "TEXT/HTML"}},
                new byte[][][] {{{1}}, {{2}, {3}}}));
        assertFalse(root.exists());
        assertThrows(IllegalArgumentException.class, () -> store.prepare(
                new String[][] {{"text/*"}}, new byte[][][] {{{1}}}));
        assertThrows(IllegalArgumentException.class, () -> store.prepare(
                new String[][] {{"text/plain"}}, new byte[][][] {}));
        assertFalse(root.exists());
    }

    @Test
    public void unpublishedSnapshotsSurviveRestartUntilSuccessfulReplacement() throws Exception {
        File root = temporary.newFolder("snapshots");
        SnapshotStore store = new SnapshotStore(root);
        String active = store.prepare(new String[][] {{"text/plain"}}, new byte[][][] {{{1}}});
        String uncertain = store.prepare(new String[][] {{"text/plain"}}, new byte[][][] {{{2}}});
        SnapshotStore restarted = new SnapshotStore(root);
        assertArrayEquals(new String[] {"text/plain"}, restarted.types(active, 0));
        assertArrayEquals(new String[] {"text/plain"}, restarted.types(uncertain, 0));
        String replacement = restarted.prepare(new String[][] {{"text/plain"}}, new byte[][][] {{{3}}});
        restarted.retainOnly(replacement);
        assertEquals(1, root.list().length);
        assertEquals(replacement, root.list()[0]);
    }

    @Test
    public void pathsAndCorruptMetadataCannotSelectOtherFiles() throws Exception {
        File root = temporary.newFolder("snapshots");
        SnapshotStore store = new SnapshotStore(root);
        String token = store.prepare(new String[][] {{"text/plain"}}, new byte[][][] {{{1}}});
        assertThrows(IOException.class, () -> store.open("../outside", 0, "*/*"));
        assertThrows(IOException.class, () -> store.open(".prepare-" + token, 0, "*/*"));
        assertThrows(IOException.class, () -> store.open(token, -1, "*/*"));
        assertThrows(IOException.class, () -> store.open(token, 0, "../types"));
        assertThrows(IOException.class, () -> store.retainOnly("../outside"));
        assertThrows(IOException.class, () -> store.retainOnly(UUID.randomUUID().toString()));
        assertEquals(1, root.list().length);
        Files.write(new File(root, token + "/0/types").toPath(), new byte[] {127, -1, -1, -1});
        assertThrows(IOException.class, () -> store.types(token, 0));
    }

    @Test
    public void retirementDoesNotTraverseSymbolicLinks() throws Exception {
        File root = temporary.newFolder("snapshots");
        File outside = temporary.newFolder("outside");
        File retained = new File(outside, "data");
        Files.write(retained.toPath(), new byte[] {1});
        String token = UUID.randomUUID().toString();
        Files.createSymbolicLink(new File(root, token).toPath(), outside.toPath());
        SnapshotStore store = new SnapshotStore(root);
        assertThrows(IOException.class, () -> store.open(token, 0, "*/*"));
        store.retainOnly(null);
        assertTrue(retained.exists());
        assertFalse(new File(root, token).exists());
    }
}
