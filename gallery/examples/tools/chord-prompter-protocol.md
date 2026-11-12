# Chord Prompter device protocol

The example uses the BLE service `7e4b0001-8c20-4b6d-a52b-9f7021cf4420`, a read/write control characteristic ending in `0002`, a read-only status characteristic ending in `0003`, and a write-only upload characteristic ending in `0004`. All characteristic values are 20 bytes and all integers are little-endian. Writes use GATT responses; the application receipt is read separately from the control characteristic. Status reads return a coherent sample of device time and playback position.

## Status

| Bytes | Value |
| --- | --- |
| 0 | Protocol identifier, 1 |
| 1 | Low bits: playing, pending control, armed start, receiving upload; high nibble: active rhythm index |
| 2 | Counted beats per bar |
| 3 | Meter denominator |
| 4–7 | Active score generation |
| 8–11 | Musical position, 960 ticks per counted beat |
| 12–15 | Tempo in thousandths of BPM |
| 16–19 | Device monotonic milliseconds, modulo 2³² |

One counted beat lasts `60/BPM` seconds. The meter denominator identifies the notated unit; it does not silently convert the tempo to a dotted-quarter pulse. Four-beat rhythm patterns scale to the whole bar and continue across chord changes.

## Requests

Control and upload writes share one sequence stream. Bytes 0, 1–2 and 3–6 contain the opcode, sequence and expected score generation. Byte 7 selects immediate, next-beat or next-bar execution using 0, 1 or 2. Bytes 8–11 contain the argument. Bytes 12–15 optionally specify an absolute execution tick for a timed control; zero selects the boundary rule. Remaining bytes are zero. A fresh connection starts at sequence zero and reads the active generation before issuing controls.

| Opcode | Argument | Behavior |
| --- | --- | --- |
| 1 | Start delay in milliseconds, 0–5000 | Start or resume; an already running session is unchanged |
| 2 | Zero | Pause at the sampled position |
| 3 | Zero | Stop and reset the position and pending controls |
| 4 | Thousandths of BPM | Change tempo at the requested boundary or tick |
| 5 | Zero-based destination bar, or `0xFFFFFFFF` | Seek at the requested boundary or tick; `0xFFFFFFFF` cancels a queued seek |
| 6 | Rhythm index, or 255 | Override rhythm, or restore section-defined rhythms |
| 16 | Upload byte length | Begin upload; bytes 12–15 contain its checksum |
| 18 | Zero | Validate and activate the complete uploaded score |

Rhythm indices are whole, quarter, pop, poprock, ballad, folk, rock, punk, reggae, shuffle, country and arpeggio, starting at zero. A seek cancels controls pending in the retired musical coordinate range. A newer seek replaces a queued seek together with the controls queued after it, and a newer tempo or rhythm change for the same tick replaces the queued one, so repeated input cannot fill the queue. Controls that arrive during a delayed start take effect at that start without moving it. Late explicit tick requests are rejected. A queued tempo or rhythm change preserves the running bar phase. The display's next page shows the destination of a seek queued for the coming bar line, and its previous page keeps the bar that was played.

Upload chunks use opcode 17, sequence at bytes 1–2, exact byte offset at 3–4, payload length at 5 and up to 14 payload bytes at 6–19. Offsets refer to UTF-8 bytes, so a chunk may split a code point. UTF-8 and the song sheet are validated only after the complete byte stream and checksum are verified. The checksum is 32-bit FNV-1a. A new score becomes active only after successful validation; an incomplete, corrupt or invalid upload preserves the existing score. Successful activation increments the generation and resets playback.

The device accepts up to 8192 source bytes, 128 bars, 256 chord slots and 16 sections. Titles and section labels are limited to 64 bytes. Lyric fragments share the source-byte allowance; their rendered size does not reject a score. Complete bars wrap into additional rows and vertically scroll when needed. Temporary upload storage is prepared once. The old score, upload bytes and parsed replacement coexist during validation, so peak load memory exceeds steady playback memory. No claim of unbounded score storage or a whole-device memory limit follows from these bounds.

## Receipts and connection lifecycle

| Bytes | Value |
| --- | --- |
| 0 | Result: success, sequence, generation, packet, capacity, transfer, checksum, score, control or unsupported glyph error, starting at zero |
| 1 | Request opcode |
| 2–3 | Request sequence |
| 4–7 | Active generation after processing |
| 8–11 | Accepted execution tick, or current position on failure |
| 12–15 | Device start time in milliseconds for a start request |
| 16–17 | Next expected sequence |
| 18–19 | Reserved |

Retrying the most recently accepted packet returns its original receipt without repeating the operation. A different packet cannot reuse that sequence. Application failures consume a valid sequence and are reported in the receipt; out-of-order packets do not advance the expected sequence. The client serializes requests and checks both receipt sequence and result.

Disconnect discards incomplete uploads and connection-local sequence state. The loaded score and playback continue on the board. Reconnection obtains current device state and starts a new request stream, without replaying old controls. Boot identities must change on a device restart; a hardware random value can seed the score generation. Browser clock mappings are discarded on reconnect and must handle the wrapping millisecond field.

Browser audio should schedule future events against calibrated device/audio clocks, using the acknowledged start time and execution ticks. A received status packet is not a cue to play an immediate click. Audio-device latency, BLE transfer time and display presentation are separate measurements.

## Browser synchronization

The device advances playback from its local monotonic clock. The browser projects between device samples using its host clock and the confirmed tempo. Visible steady playback reads status every five seconds; paused or stopped playback has no periodic status reads. Hidden pages and browser-preview mode suspend sampling. Returning to the foreground confirms current state once, including while paused. Radio connection events are independent of these application reads.

Control receipts request an immediate state confirmation. An acknowledged delayed start maps its wrapping device-millisecond timestamp to the sampled host clock; zero is a valid wrapped timestamp. A queued control schedules confirmation after its accepted execution tick. The receipt of a queued seek gives its execution tick; with the requested destination, the projection continues across the jump until a status sample shows it, so the display does not briefly show the bar it skips. Canonical section-start ticks exported by the Rust score parser also schedule confirmation because the device can change rhythm at those boundaries. The existing GATT queue serializes these reads with controls and upload chunks; repeated requests share one running read loop and a trailing confirmation.

If a reconnect exposes an armed start without its receipt, temporary 250ms confirmations resolve that transition. Pending controls whose targets are unknown are confirmed at counted-beat boundaries until resolved. Stable paused queues do not generate polling. Delayed or failed reads do not create a rapid retry loop, and closed connection epochs retire late results.

Uploads suspend background calibration; explicit playback controls can still confirm their result between chunks. Upload completion or failure obtains fresh state. Device samples older than fifteen seconds are marked stale without the former short extrapolation freeze. Known score bounds clamp the presentation at the final tick and restore the Play action. Local projection is an estimate between authoritative samples, not an audio or LCD presentation timestamp.

## Display font

The display uses a bundled 12px one-bit MIRX font with 271 mapped characters. ASCII and a fixed Chinese subset are supported; the exact repertoire is in `src/gallery/demos/assets/prompter-charset.txt`, and font provenance and license references are in `prompter-font-source.json`. Uploads containing unsupported visible title or lyric characters return result 9 and preserve the active score. The atlas is borrowed from static storage rather than expanded into a second bitmap buffer.

The browser serializes each packet and receipt pair. Long uploads yield this same bounded stream to status reads and playback commands between chunks, so the active song remains controllable during transfer. Controls retain the generation selected when requested; an operation queued across score replacement cannot silently target a different song. Only one upload runs per browser connection.
