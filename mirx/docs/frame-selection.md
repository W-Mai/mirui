# Frame candidate selection

`FrameSelector` applies sequence recovery and resource policy before comparing already-encoded frame candidates. It does not run codecs, allocate buffers, infer target capabilities or inspect pixels. Generators retain ownership of those operations and offer only candidates that are semantically valid for the source frame.

```rust
use mirx::{CodingId, FramePolicy};
use mirx::payload::frames::{FrameCandidate, FrameSelector};

let policy = FramePolicy::new(2)
    .with_max_workspace_bytes(512 * 1024)
    .with_max_decode_work(1_000_000);
let mut selector = FrameSelector::new(policy);

let first = selector.select(&[
    FrameCandidate::keyframe(CodingId::RAW, 307_236),
    FrameCandidate::keyframe(CodingId::LZ4, 41_312).with_decode_cost(307_200, 307_200),
]).unwrap();
assert_eq!(first.candidate().coding(), Some(CodingId::LZ4));

let second = selector.select(&[
    FrameCandidate::sparse(CodingId::PIXEL, 4_148).with_decode_cost(3_072, 768),
    FrameCandidate::delta(872).with_decode_cost(307_200, 307_200),
]).unwrap();
assert_eq!(second.candidate(), FrameCandidate::delta(872).with_decode_cost(307_200, 307_200));
```

Every `stored_bytes` value is the candidate's complete incremental wire contribution:

```text
stored bytes
├─ new coding parameters not already active
├─ UNIT_GROUP records
├─ UNIT_INDEX / sparse selection bytes
├─ DATA alignment gaps
├─ encoded DATA
└─ per-candidate integrity metadata
```

Codec-body size alone is not comparable. A tiny tile body may require more group/index metadata than a full independent block, and activating a new profile can add shared coding parameters once.

Admission rejects dependent storage for frame zero, chains beyond `max_delta_frames`, lossy candidates under the default lossless policy, and candidates above stored-byte, decode-work or workspace limits. `admission(candidate)` returns the exact rejection reason for reports. Failed selection leaves frame and dependency state unchanged.

Admitted candidates are ordered by complete stored bytes, independent recovery, decode work, workspace, storage kind and coding ID. The order is deterministic and independent of slice order. A keyframe resets dependency distance; omitted, sparse and delta candidates increment it. Equal-cost independent frames therefore reset recovery distance automatically.

`FramePolicy::allow_lossy` permits lossy candidates but does not define a quality floor. A generator must first reject candidates that violate its visual-quality policy, then pass their measured wire/runtime costs to the selector.
