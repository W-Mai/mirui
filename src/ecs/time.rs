/// Delta time between frames (seconds)
pub struct DeltaTime(pub f32);

/// Delta time in integer milliseconds — avoids floating point in
/// animation hot paths on targets without an FPU.
pub struct DeltaTimeMs(pub u16);

/// Total elapsed time since app start (seconds)
pub struct ElapsedTime(pub f32);
