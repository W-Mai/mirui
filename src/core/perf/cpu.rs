/// Accumulated CPU time on the calling thread. It includes user and kernel execution, not time while the thread is descheduled.
pub(crate) fn now_ns() -> Option<u64> {
    #[cfg(all(
        feature = "std",
        any(
            target_os = "linux",
            target_os = "android",
            target_os = "macos",
            target_os = "ios"
        )
    ))]
    {
        let mut time = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        // SAFETY: the pointer names a writable timespec and the clock belongs to this thread.
        if unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut time) } != 0 {
            return None;
        }
        let secs = u64::try_from(time.tv_sec).ok()?;
        let nanos = u64::try_from(time.tv_nsec)
            .ok()
            .filter(|&n| n < 1_000_000_000)?;
        secs.checked_mul(1_000_000_000)?.checked_add(nanos)
    }
    #[cfg(all(feature = "std", target_os = "windows"))]
    {
        use windows_sys::Win32::{
            Foundation::FILETIME,
            System::Threading::{GetCurrentThread, GetThreadTimes},
        };
        let mut creation = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let mut exit = creation;
        let mut kernel = creation;
        let mut user = creation;
        // SAFETY: the pseudo-handle identifies this thread and all outputs are writable.
        if unsafe {
            GetThreadTimes(
                GetCurrentThread(),
                &mut creation,
                &mut exit,
                &mut kernel,
                &mut user,
            )
        } == 0
        {
            return None;
        }
        let ticks = |v: FILETIME| (u64::from(v.dwHighDateTime) << 32) | u64::from(v.dwLowDateTime);
        ticks(kernel).checked_add(ticks(user))?.checked_mul(100)
    }
    #[cfg(not(all(
        feature = "std",
        any(
            target_os = "linux",
            target_os = "android",
            target_os = "macos",
            target_os = "ios",
            target_os = "windows"
        )
    )))]
    {
        None
    }
}

pub(crate) fn capability() -> &'static str {
    if cfg!(all(
        feature = "std",
        any(
            target_os = "linux",
            target_os = "android",
            target_os = "macos",
            target_os = "ios",
            target_os = "windows"
        )
    )) {
        "optional-thread-clock"
    } else {
        "unavailable"
    }
}

#[cfg(feature = "std")]
pub(super) fn thread_id() -> Option<u64> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        // SAFETY: gettid takes no pointers and returns this thread's kernel identity.
        u64::try_from(unsafe { libc::syscall(libc::SYS_gettid) }).ok()
    }
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        let mut id = 0;
        // SAFETY: a null thread selects the current thread; id is writable.
        (unsafe { libc::pthread_threadid_np(0, &mut id) } == 0).then_some(id)
    }
    #[cfg(target_os = "windows")]
    {
        // SAFETY: no arguments or borrowed storage.
        Some(u64::from(unsafe {
            windows_sys::Win32::System::Threading::GetCurrentThreadId()
        }))
    }
    #[cfg(not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "windows"
    )))]
    {
        None
    }
}
