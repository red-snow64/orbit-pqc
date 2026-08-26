use crate::error::{CryptoError, Result};
use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;
use zeroize::Zeroize;

/// A memory-locked, heap-allocated byte buffer that zeroizes on drop.
pub struct SecBuffer {
    ptr: NonNull<u8>,
    layout: Layout,
    len: usize,
}

unsafe impl Send for SecBuffer {}
unsafe impl Sync for SecBuffer {}

impl SecBuffer {
    /// Allocates `size` zeroed bytes locked in physical RAM.
    pub fn new(size: usize) -> Result<Self> {
        if size == 0 {
            return Err(CryptoError::AllocationError(
                "Cannot allocate 0 bytes".into(),
            ));
        }

        let layout =
            Layout::array::<u8>(size).map_err(|e| CryptoError::AllocationError(e.to_string()))?;

        let raw_ptr = unsafe { alloc_zeroed(layout) };
        let ptr = NonNull::new(raw_ptr)
            .ok_or_else(|| CryptoError::AllocationError("Null pointer returned".into()))?;

        #[cfg(unix)]
        {
            let res = unsafe { libc::mlock(ptr.as_ptr() as *const libc::c_void, size) };
            if res != 0 {
                unsafe { dealloc(ptr.as_ptr(), layout) };
                return Err(CryptoError::MemoryLockError(
                    std::io::Error::last_os_error().to_string(),
                ));
            }
        }

        #[cfg(windows)]
        {
            let res = unsafe {
                windows_sys::Win32::System::Memory::VirtualLock(
                    ptr.as_ptr() as *const core::ffi::c_void,
                    size,
                )
            };
            if res == 0 {
                unsafe { dealloc(ptr.as_ptr(), layout) };
                return Err(CryptoError::MemoryLockError(
                    std::io::Error::last_os_error().to_string(),
                ));
            }
        }

        Ok(Self {
            ptr,
            layout,
            len: size,
        })
    }

    /// Initializes a secure buffer with copy of existing data.
    pub fn from_slice(data: &[u8]) -> Result<Self> {
        let mut buf = Self::new(data.len())?;
        buf.as_mut_slice().copy_from_slice(data);
        Ok(buf)
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }
}

impl Deref for SecBuffer {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl DerefMut for SecBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut_slice()
    }
}

impl Drop for SecBuffer {
    fn drop(&mut self) {
        // Zero out memory prior to unlocking and deallocation
        self.as_mut_slice().zeroize();

        #[cfg(unix)]
        unsafe {
            libc::munlock(self.ptr.as_ptr() as *const libc::c_void, self.len);
        }

        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::System::Memory::VirtualUnlock(
                self.ptr.as_ptr() as *const core::ffi::c_void,
                self.len,
            );
        }

        unsafe {
            dealloc(self.ptr.as_ptr(), self.layout);
        }
    }
}

/// A generic secure wrapper for fixed-size cryptographic structures.
pub struct SecBox<T: Zeroize> {
    inner: Option<T>,
}

impl<T: Zeroize> SecBox<T> {
    pub fn new(val: T) -> Self {
        Self { inner: Some(val) }
    }

    pub fn get(&self) -> &T {
        self.inner.as_ref().expect("SecBox corrupted")
    }

    pub fn get_mut(&mut self) -> &mut T {
        self.inner.as_mut().expect("SecBox corrupted")
    }
}

impl<T: Zeroize> Deref for SecBox<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.get()
    }
}

impl<T: Zeroize> DerefMut for SecBox<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.get_mut()
    }
}

impl<T: Zeroize> Drop for SecBox<T> {
    fn drop(&mut self) {
        if let Some(mut val) = self.inner.take() {
            val.zeroize();
        }
    }
}
