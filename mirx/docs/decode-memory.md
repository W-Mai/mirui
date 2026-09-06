# Decode execution and memory

`ImageGroups::decode_plan` is the compact CPU-reconstruction API. `decode_plan_for` accepts a `DecodeRequest` when encoded input, decoded output, or reusable workspace has a device-facing memory contract.

```rust
use mirx::{PayloadLimits, image::{
    DecodeRequest, MemoryPlacement, SurfaceRequirements,
}};

# fn plan(groups: mirx::image::ImageGroups<'_, '_>) -> Result<(), mirx::image::DecodeError> {
let request = DecodeRequest::new(
    SurfaceRequirements::new()
        .with_base_alignment(64)
        .with_plane_alignment(64)
        .with_width_multiple(64)
        .with_stride_multiple(64),
)
.with_input(MemoryPlacement::Flash)
.with_output(MemoryPlacement::SharedNoncoherent)
.with_workspace(MemoryPlacement::SharedCoherent)
.with_workspace_alignment(64);

let plan = groups.decode_plan_for(request, &PayloadLimits::EMBEDDED)?;
assert_eq!(plan.workspace_requirements().base_alignment(), 64);
# Ok(())
# }
```

The request is a caller assertion about storage supplied outside MIRX. A byte slice cannot prove that its allocator is DMA-safe, device-visible, or backed by memory-mapped Flash. Runtime byte length and base address are still checked when output and workspace are bound.

## Placement matrix

| Placement | CPU read | CPU write | device-visible | CPU reconstruction input | output | workspace |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `Cpu` | yes | yes | no promise | yes | yes | yes |
| `Flash` | yes | no | no promise | yes | no | no |
| `SharedCoherent` | yes | yes | yes | yes | yes | yes |
| `SharedNoncoherent` | yes | yes | yes | yes, after invalidate | yes, clean before device read | yes |
| `Device` | no | no | yes | no | no | no |

`CacheSync::InvalidateBeforeRead` means non-coherent input must be invalidated before `decode_plan_for`, because preflight reads coding bytes and verifies integrity. `CacheSync::CleanAfterWrite` means reconstructed output must be cleaned before a device consumes it. MIRX reports these boundaries through `DecodeRequest` and `ImageDecodePlan`; platform cache operations remain the caller's responsibility.

## Three execution modes

- `Reconstruct` produces ordinary sample planes and is implemented by the built-in decoder.
- `Compute` describes reconstruction by a compute or firmware backend. It is distinct from direct upload because it still produces decoded samples.
- `DirectUpload` preserves a backend-native compressed surface. It requires a matching stored coding and layout rather than ordinary pixel reconstruction.

The built-in slice decoder rejects `Compute`, `DirectUpload`, and device-only buffers before image preflight. This is a capability result, not an automatic fallback. A device adapter must expose its own execution path and validate coding revision, sample layout, block geometry, memory placement, alignment, coherence, lifetime, and failure atomicity.

## Alignment remains independent

The following values must not be collapsed:

1. Stored unit input alignment is a file or Flash address promise.
2. `input_addresses_are_aligned` checks the actual selected byte-slice addresses.
3. `SurfaceRequirements` defines decoded base, plane, width, height, and stride constraints.
4. `workspace_alignment` defines the reusable codec workspace address constraint.

A 64-byte stored input start does not imply a 64-byte heap address. A 64-byte output base does not imply a 64-byte row stride. Every required property is planned and checked at its own boundary.
