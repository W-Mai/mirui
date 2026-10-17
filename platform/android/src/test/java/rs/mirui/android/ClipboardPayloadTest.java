package rs.mirui.android;

import static org.junit.Assert.assertArrayEquals;
import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertNull;
import static org.junit.Assert.assertThrows;

import java.nio.charset.CharacterCodingException;
import org.junit.Test;

public final class ClipboardPayloadTest {
    @Test
    public void textConversionPreservesUnicodeNewlinesAndEmbeddedZero() throws Exception {
        String original = "a\r\n你\u0000\ud83d\ude42";
        byte[] bytes = ClipboardPayload.encode(original);
        ClipboardPayload payload = new ClipboardPayload(new String[][] {{"TEXT/PLAIN"}}, new byte[][][] {{bytes}});
        assertEquals(original, payload.text(0, "text/plain"));
        assertNull(payload.text(0, "text/html"));
        assertArrayEquals(bytes, ClipboardPayload.encode(payload.text(0, "text/plain")));
    }

    @Test
    public void malformedTextIsRejectedInsteadOfLossyReplacement() {
        ClipboardPayload payload = new ClipboardPayload(new String[][] {{"text/plain"}}, new byte[][][] {{{-1}}});
        assertThrows(CharacterCodingException.class, () -> payload.text(0, "text/plain"));
        assertThrows(CharacterCodingException.class, () -> ClipboardPayload.encode("\ud800"));
    }
}
