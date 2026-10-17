use alloc::{rc::Rc, vec::Vec};
use core::{marker::PhantomData, ptr::NonNull};
use windows_sys::Win32::{
    Foundation::{GetLastError, GlobalFree, HANDLE, HGLOBAL, HWND, SetLastError},
    System::{
        DataExchange::{
            CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
        },
        Memory::{GMEM_MOVEABLE, GMEM_ZEROINIT, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock},
    },
};

use crate::ui::clipboard::ClipboardError;

pub(super) struct Memory(HGLOBAL);

impl Memory {
    pub(super) fn copy(bytes: &[u8], exact: bool) -> Result<Self, ClipboardError> {
        if bytes.is_empty() {
            return Err(ClipboardError::Unsupported);
        }
        let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, bytes.len()) };
        if handle.is_null() {
            return Err(ClipboardError::Capacity);
        }
        let memory = Self(handle);
        let length = unsafe { GlobalSize(handle) };
        if length < bytes.len() || (exact && length != bytes.len()) {
            return Err(ClipboardError::Unsupported);
        }
        let locked = Locked::new(handle)?;
        // The owned allocation is at least bytes.len() and remains locked for the copy.
        unsafe {
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), locked.data.as_ptr(), bytes.len());
        }
        drop(locked);
        Ok(memory)
    }
}

impl Drop for Memory {
    fn drop(&mut self) {
        unsafe {
            GlobalFree(self.0);
        }
    }
}

struct Locked {
    handle: HGLOBAL,
    data: NonNull<u8>,
}

impl Locked {
    fn new(handle: HGLOBAL) -> Result<Self, ClipboardError> {
        let data =
            NonNull::new(unsafe { GlobalLock(handle) }.cast()).ok_or(ClipboardError::Platform)?;
        Ok(Self { handle, data })
    }
}

impl Drop for Locked {
    fn drop(&mut self) {
        unsafe {
            GlobalUnlock(self.handle);
        }
    }
}

pub(super) struct Session {
    open: bool,
    thread: PhantomData<Rc<()>>,
}

impl Session {
    pub(super) fn open(window: HWND) -> Result<Self, ClipboardError> {
        if unsafe { OpenClipboard(window) } == 0 {
            return Err(ClipboardError::Unavailable);
        }
        Ok(Self {
            open: true,
            thread: PhantomData,
        })
    }

    pub(super) fn clear(&self) -> Result<(), ClipboardError> {
        if unsafe { EmptyClipboard() } == 0 {
            Err(ClipboardError::Platform)
        } else {
            Ok(())
        }
    }

    pub(super) fn put(&self, format: u32, memory: Memory) -> Result<(), ClipboardError> {
        if unsafe { SetClipboardData(format, memory.0 as HANDLE) }.is_null() {
            return Err(ClipboardError::Platform);
        }
        core::mem::forget(memory);
        Ok(())
    }

    pub(super) fn get(&self, format: u32) -> Result<Vec<u8>, ClipboardError> {
        let handle = unsafe { GetClipboardData(format) };
        if handle.is_null() {
            return Err(ClipboardError::Platform);
        }
        let length = unsafe { GlobalSize(handle) };
        if length == 0 || length > isize::MAX as usize {
            return Err(ClipboardError::InvalidData);
        }
        let locked = Locked::new(handle)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(length)
            .map_err(|_| ClipboardError::Capacity)?;
        // The clipboard is open and the global block locked until this owned copy is complete.
        bytes.extend_from_slice(unsafe {
            core::slice::from_raw_parts(locked.data.as_ptr(), length)
        });
        Ok(bytes)
    }

    pub(super) fn finish<T>(
        mut self,
        result: Result<T, ClipboardError>,
    ) -> Result<T, ClipboardError> {
        if unsafe { CloseClipboard() } == 0 {
            return Err(result.err().unwrap_or(ClipboardError::Platform));
        }
        self.open = false;
        result
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if self.open {
            unsafe {
                CloseClipboard();
            }
        }
    }
}

pub(super) fn enumeration_end() -> Result<(), ClipboardError> {
    if unsafe { GetLastError() } == 0 {
        Ok(())
    } else {
        Err(ClipboardError::Platform)
    }
}

pub(super) fn clear_error() {
    unsafe {
        SetLastError(0);
    }
}
