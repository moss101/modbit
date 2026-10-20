//! Owned CoreFoundation values and conversions. The only place that frees
//! what the C API hands out ("create"/"copy" rule); everything else holds a
//! [`Cf`] and drops it.
#![allow(unsafe_code)]

use std::ffi::{CString, c_char, c_void};

use super::ffi::{
    CFArrayGetCount, CFArrayGetTypeID, CFArrayGetValueAtIndex, CFBooleanGetTypeID,
    CFBooleanGetValue, CFDictionaryGetTypeID, CFDictionaryGetValue, CFGetTypeID, CFNumberGetTypeID,
    CFNumberGetValue, CFRelease, CFRetain, CFStringCreateWithCString, CFStringGetCString,
    CFStringGetLength, CFStringGetMaximumSizeForEncoding, CFStringGetTypeID, CFStringRef,
    CFTypeRef, kCFAllocatorDefault, kCFNumberFloat64Type, kCFNumberSInt64Type,
    kCFStringEncodingUTF8,
};

/// An owned reference to any CoreFoundation object. Released on drop.
pub struct Cf(CFTypeRef);

// SAFETY: CoreFoundation immutable values and AXUIElementRefs are
// thread-safe to retain, release and pass between threads; the helper never
// mutates a shared one.
unsafe impl Send for Cf {}
unsafe impl Sync for Cf {}

impl Cf {
    /// Take ownership of a reference the "create"/"copy" rule gave us.
    /// Returns `None` for null.
    #[must_use]
    pub fn own(p: CFTypeRef) -> Option<Self> {
        (!p.is_null()).then_some(Self(p))
    }

    /// Retain a reference the "get" rule lent us.
    #[must_use]
    pub fn retain(p: CFTypeRef) -> Option<Self> {
        if p.is_null() {
            None
        } else {
            // SAFETY: `p` is a live CF object (the caller got it from a CF API).
            Some(Self(unsafe { CFRetain(p) }))
        }
    }

    /// The raw reference (still owned by `self`).
    #[must_use]
    pub fn raw(&self) -> CFTypeRef {
        self.0
    }

    fn is_type(&self, id: usize) -> bool {
        // SAFETY: `self.0` is a live CF object.
        unsafe { CFGetTypeID(self.0) == id }
    }

    /// Whether two references denote the same object (`CFEqual`).
    #[must_use]
    pub fn same_as(&self, other: &Cf) -> bool {
        // SAFETY: both live.
        unsafe { super::ffi::CFEqual(self.0, other.0) != 0 }
    }

    /// `CFHash`, consistent with [`Cf::same_as`].
    #[must_use]
    pub fn hash_code(&self) -> usize {
        // SAFETY: live.
        unsafe { super::ffi::CFHash(self.0) }
    }

    /// As a Rust string, if it is a CFString.
    #[must_use]
    pub fn as_string(&self) -> Option<String> {
        // SAFETY: type id queried on a live object.
        if !self.is_type(unsafe { CFStringGetTypeID() }) {
            return None;
        }
        Some(string_of(self.0))
    }

    /// As a bool, if it is a CFBoolean.
    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        // SAFETY: type checked first.
        if !self.is_type(unsafe { CFBooleanGetTypeID() }) {
            return None;
        }
        Some(unsafe { CFBooleanGetValue(self.0) } != 0)
    }

    /// As an f64, if it is a CFNumber.
    #[must_use]
    pub fn as_f64(&self) -> Option<f64> {
        // SAFETY: type checked first; the out pointer is a local f64.
        if !self.is_type(unsafe { CFNumberGetTypeID() }) {
            return None;
        }
        let mut v = 0f64;
        let ok = unsafe { CFNumberGetValue(self.0, kCFNumberFloat64Type, (&raw mut v).cast()) };
        (ok != 0).then_some(v)
    }

    /// As an i64, if it is an integral CFNumber.
    #[must_use]
    pub fn as_i64(&self) -> Option<i64> {
        // SAFETY: as above.
        if !self.is_type(unsafe { CFNumberGetTypeID() }) {
            return None;
        }
        let mut v = 0i64;
        let ok = unsafe { CFNumberGetValue(self.0, kCFNumberSInt64Type, (&raw mut v).cast()) };
        (ok != 0).then_some(v)
    }

    /// The elements, retained, if it is a CFArray.
    #[must_use]
    pub fn as_array(&self) -> Option<Vec<Cf>> {
        // SAFETY: type checked first; indices are within the count.
        if !self.is_type(unsafe { CFArrayGetTypeID() }) {
            return None;
        }
        let n = unsafe { CFArrayGetCount(self.0) };
        let mut out = Vec::with_capacity(usize::try_from(n).unwrap_or(0));
        for i in 0..n {
            let p = unsafe { CFArrayGetValueAtIndex(self.0, i) };
            if let Some(c) = Cf::retain(p) {
                out.push(c);
            }
        }
        Some(out)
    }

    /// The value for a string key, retained, if it is a CFDictionary.
    #[must_use]
    pub fn dict_get(&self, key: &CfStr) -> Option<Cf> {
        // SAFETY: type checked first; the key is a live CFString.
        if !self.is_type(unsafe { CFDictionaryGetTypeID() }) {
            return None;
        }
        let p = unsafe { CFDictionaryGetValue(self.0, key.0.raw()) };
        Cf::retain(p)
    }
}

impl Drop for Cf {
    fn drop(&mut self) {
        // SAFETY: we own one retain count.
        unsafe { CFRelease(self.0) }
    }
}

/// An owned CFString made from Rust text.
pub struct CfStr(pub Cf);

impl CfStr {
    /// A CFString of `s` (interior NULs are dropped).
    #[must_use]
    pub fn new(s: &str) -> Self {
        let c = CString::new(s.replace('\0', "")).unwrap_or_default();
        // SAFETY: `c` is a NUL-terminated UTF-8 string that outlives the call.
        let r = unsafe {
            CFStringCreateWithCString(kCFAllocatorDefault, c.as_ptr(), kCFStringEncodingUTF8)
        };
        Self(Cf::own(r).unwrap_or_else(|| {
            // An empty string is always creatable; reaching here means memory exhaustion.
            let e = CString::default();
            let r = unsafe {
                CFStringCreateWithCString(kCFAllocatorDefault, e.as_ptr(), kCFStringEncodingUTF8)
            };
            Cf::own(r).expect("CFString")
        }))
    }

    /// The raw string.
    #[must_use]
    pub fn raw(&self) -> CFStringRef {
        self.0.raw()
    }
}

/// UTF-8 text of a CFString (lossless for valid strings).
#[must_use]
pub fn string_of(s: CFStringRef) -> String {
    // SAFETY: `s` is a live CFString; the buffer is sized by the API's own bound.
    unsafe {
        let len = CFStringGetLength(s);
        let max = CFStringGetMaximumSizeForEncoding(len, kCFStringEncodingUTF8) + 1;
        let mut buf = vec![0u8; usize::try_from(max).unwrap_or(1)];
        if CFStringGetCString(
            s,
            buf.as_mut_ptr().cast::<c_char>(),
            max,
            kCFStringEncodingUTF8,
        ) == 0
        {
            return String::new();
        }
        let end = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
        String::from_utf8_lossy(&buf[..end]).into_owned()
    }
}

/// Convenience: a pointer from a Rust `&T` for out-parameters.
pub fn out<T>(t: &mut T) -> *mut c_void {
    std::ptr::from_mut(t).cast()
}
