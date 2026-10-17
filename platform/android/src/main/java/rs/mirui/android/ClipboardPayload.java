package rs.mirui.android;

import java.nio.ByteBuffer;
import java.nio.CharBuffer;
import java.nio.charset.CharacterCodingException;
import java.nio.charset.StandardCharsets;

/** Owned JNI transfer data; rows are items and columns are alternate representations. */
public final class ClipboardPayload {
    public final String[][] types;
    public final byte[][][] values;

    public ClipboardPayload(String[][] types, byte[][][] values) {
        SnapshotStore.validate(types, values);
        this.types = types;
        this.values = values;
    }

    String text(int item, String mime) throws CharacterCodingException {
        for (int i = 0; i < types[item].length; i++) {
            if (types[item][i].equalsIgnoreCase(mime)) {
                return StandardCharsets.UTF_8.newDecoder().decode(ByteBuffer.wrap(values[item][i])).toString();
            }
        }
        return null;
    }

    static byte[] encode(CharSequence text) throws CharacterCodingException {
        ByteBuffer encoded = StandardCharsets.UTF_8.newEncoder().encode(CharBuffer.wrap(text));
        byte[] bytes = new byte[encoded.remaining()];
        encoded.get(bytes);
        return bytes;
    }
}
