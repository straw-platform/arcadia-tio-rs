use super::*;

pub(crate) struct NativeOutput<T> {
    pub(crate) raw: T,
    pub(crate) free: unsafe extern "C" fn(*mut T),
}

impl<T> NativeOutput<T> {
    pub(crate) fn new(raw: T, free: unsafe extern "C" fn(*mut T)) -> Self {
        Self { raw, free }
    }

    pub(crate) fn as_mut_ptr(&mut self) -> *mut T {
        &mut self.raw
    }

    pub(crate) fn as_ref(&self) -> &T {
        &self.raw
    }
}

impl<T> Drop for NativeOutput<T> {
    fn drop(&mut self) {
        // SAFETY: The guard uniquely owns the native output and invokes its matching free function
        // exactly once, including when status or Rust-side conversion returns early.
        unsafe { (self.free)(&mut self.raw) };
    }
}

pub(crate) struct NativeArrayOutput<T> {
    pub(crate) ptr: *mut T,
    pub(crate) len: usize,
    pub(crate) free: unsafe extern "C" fn(*mut T, usize),
}

impl<T> NativeArrayOutput<T> {
    pub(crate) fn new(free: unsafe extern "C" fn(*mut T, usize)) -> Self {
        Self {
            ptr: ptr::null_mut(),
            len: 0,
            free,
        }
    }

    pub(crate) fn ptr_out(&mut self) -> *mut *mut T {
        &mut self.ptr
    }

    pub(crate) fn len_out(&mut self) -> *mut usize {
        &mut self.len
    }

    pub(crate) fn parts(&self) -> (*mut T, usize) {
        (self.ptr, self.len)
    }
}

impl<T> Drop for NativeArrayOutput<T> {
    fn drop(&mut self) {
        // SAFETY: The guard uniquely owns this native pointer/length pair and calls the matching
        // array free function once. Native array free functions accept their empty null/zero form.
        unsafe { (self.free)(self.ptr, self.len) };
    }
}

pub(crate) struct NativePointerOutput<T> {
    pub(crate) ptr: *mut T,
    pub(crate) free: unsafe extern "C" fn(*mut T),
}

impl<T> NativePointerOutput<T> {
    pub(crate) fn new(free: unsafe extern "C" fn(*mut T)) -> Self {
        Self {
            ptr: ptr::null_mut(),
            free,
        }
    }

    pub(crate) fn out(&mut self) -> *mut *mut T {
        &mut self.ptr
    }

    pub(crate) fn get(&self) -> *mut T {
        self.ptr
    }

    #[cfg(any(feature = "format-ocb", test))]
    pub(crate) fn take(&mut self) -> *mut T {
        mem::replace(&mut self.ptr, ptr::null_mut())
    }
}

impl<T> Drop for NativePointerOutput<T> {
    fn drop(&mut self) {
        // SAFETY: The guard uniquely owns the pointer and invokes its matching free function once.
        if !self.ptr.is_null() {
            unsafe { (self.free)(self.ptr) };
        }
    }
}

pub(crate) fn path_to_cstring(path: impl AsRef<Path>) -> Result<CString> {
    let path = path.as_ref();
    let text = path
        .to_str()
        .ok_or_else(|| TioError::invalid_argument("path must be valid UTF-8 for the C ABI"))?;
    let path = CString::new(text)
        .map_err(|_| TioError::invalid_argument("path contains an interior NUL byte"))?;
    ensure_native_abi()?;
    Ok(path)
}

pub(crate) fn string_to_cstring(value: &str, label: &str) -> Result<CString> {
    CString::new(value)
        .map_err(|_| TioError::invalid_argument(format!("{label} contains an interior NUL byte")))
}

pub(crate) fn optional_c_string(ptr: *const c_char) -> Result<Option<String>> {
    if ptr.is_null() {
        Ok(None)
    } else {
        // SAFETY: Native metadata strings are documented as NUL-terminated C strings owned by the
        // metadata object while it is alive. The wrapper copies them immediately.
        let value = unsafe { CStr::from_ptr(ptr) }.to_str().map_err(|_| {
            TioError::conversion("native output returned a string that is not valid UTF-8")
        })?;
        Ok(Some(value.to_owned()))
    }
}

pub(crate) fn required_c_string(ptr: *const c_char, label: &str) -> Result<String> {
    optional_c_string(ptr)?
        .ok_or_else(|| TioError::conversion(format!("native output returned null {label}")))
}

pub(crate) unsafe fn checked_slice<'a, T>(
    ptr: *const T,
    len: usize,
    label: &str,
) -> Result<&'a [T]> {
    if len == 0 {
        return Ok(&[]);
    }
    if ptr.is_null() {
        return Err(TioError::conversion(format!(
            "native output returned null {label} with non-zero length {len}"
        )));
    }
    if !(ptr as usize).is_multiple_of(mem::align_of::<T>()) {
        return Err(TioError::conversion(format!(
            "native output returned misaligned {label} pointer"
        )));
    }
    let byte_len = len.checked_mul(mem::size_of::<T>()).ok_or_else(|| {
        TioError::conversion(format!("native output {label} byte length overflows"))
    })?;
    if byte_len > isize::MAX as usize || (ptr as usize).checked_add(byte_len).is_none() {
        return Err(TioError::conversion(format!(
            "native output {label} byte range is not representable"
        )));
    }
    // SAFETY: The caller owns the C-ABI validity obligation. The checks above establish the
    // null/length, alignment, byte-size, and address-range preconditions that the wrapper can
    // validate before borrowing the native allocation.
    Ok(unsafe { slice::from_raw_parts(ptr, len) })
}

pub(crate) unsafe fn copy_checked_slice<T: Copy>(
    ptr: *const T,
    len: usize,
    label: &str,
) -> Result<Vec<T>> {
    let source = unsafe { checked_slice(ptr, len, label) }?;
    let mut values = Vec::new();
    values.try_reserve_exact(source.len()).map_err(|_| {
        TioError::conversion(format!(
            "could not allocate {} native output entries for {label}",
            source.len()
        ))
    })?;
    values.extend_from_slice(source);
    Ok(values)
}
