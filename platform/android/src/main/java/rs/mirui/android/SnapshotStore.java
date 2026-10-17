package rs.mirui.android;

import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileNotFoundException;
import java.io.FileOutputStream;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.LinkOption;
import java.nio.file.StandardCopyOption;
import java.util.HashSet;
import java.util.Locale;
import java.util.UUID;
import java.util.regex.Pattern;

/** Private immutable files backing explicitly published clipboard items. */
final class SnapshotStore {
    private static final Pattern TOKEN = Pattern.compile(
            "[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}");
    private static final Pattern MIME = Pattern.compile(
            "[a-zA-Z0-9!#$%&'+.^_`|~-]+/[a-zA-Z0-9!#$%&'+.^_`|~-]+");
    private final File root;

    SnapshotStore(File root) {
        this.root = root;
    }

    static void validate(String[][] types, byte[][][] values) {
        if (types == null || values == null || types.length == 0 || types.length != values.length) {
            throw new IllegalArgumentException("Invalid clipboard items");
        }
        for (int item = 0; item < types.length; item++) {
            if (types[item] == null || values[item] == null || types[item].length == 0
                    || types[item].length != values[item].length) {
                throw new IllegalArgumentException("Invalid clipboard representations");
            }
            HashSet<String> seen = new HashSet<>();
            for (int format = 0; format < types[item].length; format++) {
                String type = types[item][format];
                if (type == null || type.length() > 65535 || !MIME.matcher(type).matches()
                        || !seen.add(type.toLowerCase(Locale.ROOT)) || values[item][format] == null) {
                    throw new IllegalArgumentException("Invalid clipboard MIME data");
                }
            }
        }
    }

    synchronized String prepare(String[][] types, byte[][][] values) throws IOException {
        validate(types, values);
        Files.createDirectories(root.toPath());
        String token = UUID.randomUUID().toString();
        File staging = new File(root, ".prepare-" + token);
        Files.createDirectory(staging.toPath());
        try {
            for (int item = 0; item < types.length; item++) {
                File directory = new File(staging, Integer.toString(item));
                Files.createDirectory(directory.toPath());
                for (int format = 0; format < types[item].length; format++) {
                    try (FileOutputStream stream = new FileOutputStream(new File(directory, Integer.toString(format)))) {
                        stream.write(values[item][format]);
                        stream.getFD().sync();
                    }
                }
                try (FileOutputStream file = new FileOutputStream(new File(directory, "types"));
                        DataOutputStream stream = new DataOutputStream(file)) {
                    stream.writeInt(types[item].length);
                    for (String type : types[item]) {
                        stream.writeUTF(type);
                    }
                    stream.flush();
                    file.getFD().sync();
                }
            }
            Files.move(staging.toPath(), new File(root, token).toPath(), StandardCopyOption.ATOMIC_MOVE);
            return token;
        } catch (IOException | RuntimeException failure) {
            try {
                remove(staging);
            } catch (IOException cleanup) {
                failure.addSuppressed(cleanup);
            }
            throw failure;
        }
    }

    synchronized String[] types(String token, int item) throws IOException {
        File directory = itemDirectory(token, item);
        try (FileInputStream file = new FileInputStream(new File(directory, "types"));
                DataInputStream stream = new DataInputStream(file)) {
            int count = stream.readInt();
            if (count <= 0 || count > (file.getChannel().size() - 4) / 5) {
                throw new IOException("Invalid clipboard MIME table");
            }
            String[] types = new String[count];
            HashSet<String> seen = new HashSet<>();
            for (int i = 0; i < count; i++) {
                types[i] = stream.readUTF();
                if (!MIME.matcher(types[i]).matches() || !seen.add(types[i].toLowerCase(Locale.ROOT))) {
                    throw new IOException("Invalid clipboard MIME table");
                }
            }
            if (stream.read() != -1) {
                throw new IOException("Trailing clipboard MIME data");
            }
            return types;
        }
    }

    synchronized FileInputStream open(String token, int item, String filter) throws IOException {
        String[] types = types(token, item);
        for (int i = 0; i < types.length; i++) {
            if (matches(types[i], filter)) {
                File file = new File(itemDirectory(token, item), Integer.toString(i));
                if (!Files.isRegularFile(file.toPath(), LinkOption.NOFOLLOW_LINKS)) {
                    throw new FileNotFoundException("Clipboard data is unavailable");
                }
                return new FileInputStream(file);
            }
        }
        throw new FileNotFoundException("Clipboard MIME type is unavailable");
    }

    synchronized void retainOnly(String token) throws IOException {
        if (token != null && (!TOKEN.matcher(token).matches()
                || !Files.isDirectory(new File(root, token).toPath(), LinkOption.NOFOLLOW_LINKS))) {
            throw new FileNotFoundException("Clipboard snapshot is unavailable");
        }
        File[] entries = root.listFiles();
        if (entries == null) {
            if (root.exists()) {
                throw new IOException("Cannot enumerate clipboard storage");
            }
            return;
        }
        for (File entry : entries) {
            String name = entry.getName();
            if (!name.equals(token) && (TOKEN.matcher(name).matches()
                    || (name.startsWith(".prepare-") && TOKEN.matcher(name.substring(9)).matches()))) {
                remove(entry);
            }
        }
    }

    static boolean matches(String type, String filter) {
        if (filter == null) {
            return false;
        }
        int slash = type.indexOf('/');
        return type.equalsIgnoreCase(filter) || filter.equals("*/*")
                || (slash > 0 && filter.equalsIgnoreCase(type.substring(0, slash) + "/*"));
    }

    private File itemDirectory(String token, int item) throws IOException {
        if (token == null || !TOKEN.matcher(token).matches() || item < 0) {
            throw new FileNotFoundException("Invalid clipboard item");
        }
        File snapshot = new File(root, token);
        File directory = new File(snapshot, Integer.toString(item));
        if (!Files.isDirectory(snapshot.toPath(), LinkOption.NOFOLLOW_LINKS)
                || !Files.isDirectory(directory.toPath(), LinkOption.NOFOLLOW_LINKS)) {
            throw new FileNotFoundException("Clipboard item is unavailable");
        }
        return directory;
    }

    private static void remove(File entry) throws IOException {
        if (Files.isDirectory(entry.toPath(), LinkOption.NOFOLLOW_LINKS)) {
            File[] children = entry.listFiles();
            if (children == null) {
                throw new IOException("Cannot enumerate clipboard snapshot");
            }
            for (File child : children) {
                remove(child);
            }
        }
        Files.deleteIfExists(entry.toPath());
    }
}
