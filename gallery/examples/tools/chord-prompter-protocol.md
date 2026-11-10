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
| 5 | Zero-based destination bar | Seek at the requested boundary or tick |
| 6 | Rhythm index, or 255 | Override rhythm, or restore section-defined rhythms |
| 16 | Upload byte length | Begin upload; bytes 12–15 contain its checksum |
| 18 | Zero | Validate and activate the complete uploaded score |

Rhythm indices are whole, quarter, pop, poprock, ballad, folk, rock, punk, reggae, shuffle, country and arpeggio, starting at zero. A seek cancels controls pending in the retired musical coordinate range. Late explicit tick requests are rejected. A queued tempo or rhythm change preserves the running bar phase.

Upload chunks use opcode 17, sequence at bytes 1–2, exact byte offset at 3–4, payload length at 5 and up to 14 payload bytes at 6–19. Offsets refer to UTF-8 bytes, so a chunk may split a code point. UTF-8 and the song sheet are validated only after the complete byte stream and checksum are verified. The checksum is 32-bit FNV-1a. A new score becomes active only after successful validation; an incomplete, corrupt or invalid upload preserves the existing score. Successful activation increments the generation and resets playback.

The device accepts up to 8192 source bytes, 128 bars, 256 chord slots and 16 sections. Each lyric fragment is limited to 128 source bytes and each title or section label to 64 bytes, bounding text-layout preparation as well as source storage. Temporary upload storage is prepared once. The old score, upload bytes and parsed replacement coexist during validation, so peak load memory exceeds steady playback memory. No claim of unbounded score storage or a whole-device memory limit follows from these bounds.

## Receipts and connection lifecycle

| Bytes | Value |
| --- | --- |
| 0 | Result: success, sequence, generation, packet, capacity, transfer, checksum, score or control error, starting at zero |
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
