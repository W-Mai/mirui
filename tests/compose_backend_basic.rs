//! S1 scope: verify that compose_backend! parses syntax and emits a struct
//! declaration with one generic per field. DrawBackend impl comes in S2.

use mirui_macros::compose_backend;

compose_backend! {
    pub struct Hybrid {
        sw: u8,
        gpu: u16,
    }
    route {
        default => sw,
        blit => gpu,
    }
}

#[test]
fn hybrid_is_constructible() {
    // Just needs to type-check; sw/gpu are concrete enough to instantiate.
    let h: Hybrid<u8, u16> = Hybrid { sw: 1, gpu: 2 };
    assert_eq!(h.sw, 1);
    assert_eq!(h.gpu, 2);
}
