//! Hand-written declarations of the few macOS C functions the helper calls
//! (CoreFoundation, ApplicationServices / Accessibility, CoreGraphics,
//! Security, libSystem). No binding crate: the surface is small, every
//! declaration is checked against the SDK headers, and a smaller dependency
//! tree is the point of a privileged helper.
#![allow(
    non_snake_case,
    non_upper_case_globals,
    non_camel_case_types,
    dead_code
)]

use std::ffi::{c_char, c_int, c_void};

pub type CFTypeRef = *const c_void;
pub type CFStringRef = *const c_void;
pub type CFArrayRef = *const c_void;
pub type CFDictionaryRef = *const c_void;
pub type CFURLRef = *const c_void;
pub type CFBundleRef = *const c_void;
pub type CFAllocatorRef = *const c_void;
pub type CFIndex = isize;
pub type CFTypeID = usize;
pub type Boolean = u8;
pub type AXUIElementRef = *const c_void;
pub type AXValueRef = *const c_void;
pub type AXError = i32;
pub type CGImageRef = *const c_void;
pub type CGContextRef = *mut c_void;
pub type CGColorSpaceRef = *const c_void;
pub type CGEventRef = *mut c_void;
pub type CGEventSourceRef = *mut c_void;

pub const kCFStringEncodingUTF8: u32 = 0x0800_0100;
pub const kCFNumberSInt64Type: CFIndex = 4;
pub const kCFNumberFloat64Type: CFIndex = 6;

// AXError values the helper tells apart.
pub const kAXErrorSuccess: AXError = 0;
pub const kAXErrorFailure: AXError = -25200;
pub const kAXErrorIllegalArgument: AXError = -25201;
pub const kAXErrorInvalidUIElement: AXError = -25202;
pub const kAXErrorCannotComplete: AXError = -25204;
pub const kAXErrorAttributeUnsupported: AXError = -25205;
pub const kAXErrorActionUnsupported: AXError = -25206;
pub const kAXErrorNotImplemented: AXError = -25208;
pub const kAXErrorNoValue: AXError = -25212;
pub const kAXErrorAPIDisabled: AXError = -25211;

pub const kAXValueCGPointType: u32 = 1;
pub const kAXValueCGSizeType: u32 = 2;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CGPoint {
    pub x: f64,
    pub y: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CGSize {
    pub width: f64,
    pub height: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CGRect {
    pub origin: CGPoint,
    pub size: CGSize,
}

/// `CGRectNull`: the window's own bounds when passed to `CGWindowListCreateImage`.
pub const CGRectNull: CGRect = CGRect {
    origin: CGPoint {
        x: f64::INFINITY,
        y: f64::INFINITY,
    },
    size: CGSize {
        width: 0.0,
        height: 0.0,
    },
};

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    pub static kCFAllocatorDefault: CFAllocatorRef;
    pub static kCFBooleanTrue: CFTypeRef;
    pub static kCFBooleanFalse: CFTypeRef;
    pub fn CFRelease(cf: CFTypeRef);
    pub fn CFRetain(cf: CFTypeRef) -> CFTypeRef;
    pub fn CFEqual(a: CFTypeRef, b: CFTypeRef) -> Boolean;
    pub fn CFHash(a: CFTypeRef) -> usize;
    pub fn CFGetTypeID(cf: CFTypeRef) -> CFTypeID;
    pub fn CFStringGetTypeID() -> CFTypeID;
    pub fn CFArrayGetTypeID() -> CFTypeID;
    pub fn CFBooleanGetTypeID() -> CFTypeID;
    pub fn CFNumberGetTypeID() -> CFTypeID;
    pub fn CFDictionaryGetTypeID() -> CFTypeID;
    pub fn CFArrayGetCount(a: CFArrayRef) -> CFIndex;
    pub fn CFArrayGetValueAtIndex(a: CFArrayRef, i: CFIndex) -> *const c_void;
    pub fn CFStringGetLength(s: CFStringRef) -> CFIndex;
    pub fn CFStringGetMaximumSizeForEncoding(len: CFIndex, enc: u32) -> CFIndex;
    pub fn CFStringGetCString(s: CFStringRef, buf: *mut c_char, size: CFIndex, enc: u32)
    -> Boolean;
    pub fn CFStringCreateWithCString(
        alloc: CFAllocatorRef,
        c: *const c_char,
        enc: u32,
    ) -> CFStringRef;
    pub fn CFBooleanGetValue(b: CFTypeRef) -> Boolean;
    pub fn CFNumberGetValue(n: CFTypeRef, ty: CFIndex, out: *mut c_void) -> Boolean;
    pub fn CFDictionaryGetValue(d: CFDictionaryRef, key: *const c_void) -> *const c_void;
    pub fn CFURLCreateFromFileSystemRepresentation(
        alloc: CFAllocatorRef,
        buf: *const u8,
        len: CFIndex,
        is_dir: Boolean,
    ) -> CFURLRef;
    pub fn CFBundleCreate(alloc: CFAllocatorRef, url: CFURLRef) -> CFBundleRef;
    pub fn CFBundleGetIdentifier(b: CFBundleRef) -> CFStringRef;
    pub fn CFDictionaryCreate(
        alloc: CFAllocatorRef,
        keys: *const *const c_void,
        values: *const *const c_void,
        count: CFIndex,
        key_callbacks: *const c_void,
        value_callbacks: *const c_void,
    ) -> CFDictionaryRef;
    pub static kCFTypeDictionaryKeyCallBacks: u8;
    pub static kCFTypeDictionaryValueCallBacks: u8;
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    pub fn AXIsProcessTrusted() -> Boolean;
    pub fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> Boolean;
    pub fn AXUIElementCreateApplication(pid: c_int) -> AXUIElementRef;
    pub fn AXUIElementCopyAttributeValue(
        e: AXUIElementRef,
        attr: CFStringRef,
        out: *mut CFTypeRef,
    ) -> AXError;
    pub fn AXUIElementSetAttributeValue(
        e: AXUIElementRef,
        attr: CFStringRef,
        value: CFTypeRef,
    ) -> AXError;
    pub fn AXUIElementIsAttributeSettable(
        e: AXUIElementRef,
        attr: CFStringRef,
        out: *mut Boolean,
    ) -> AXError;
    pub fn AXUIElementCopyActionNames(e: AXUIElementRef, out: *mut CFArrayRef) -> AXError;
    pub fn AXUIElementPerformAction(e: AXUIElementRef, action: CFStringRef) -> AXError;
    pub fn AXUIElementSetMessagingTimeout(e: AXUIElementRef, seconds: f32) -> AXError;
    pub fn AXUIElementCopyElementAtPosition(
        app: AXUIElementRef,
        x: f32,
        y: f32,
        out: *mut AXUIElementRef,
    ) -> AXError;
    pub fn AXValueGetValue(v: AXValueRef, ty: u32, out: *mut c_void) -> Boolean;
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    pub fn CGPreflightScreenCaptureAccess() -> Boolean;
    pub fn CGRequestScreenCaptureAccess() -> Boolean;
    pub fn CGWindowListCopyWindowInfo(option: u32, relative_to: u32) -> CFArrayRef;
    pub fn CGRectMakeWithDictionaryRepresentation(d: CFDictionaryRef, out: *mut CGRect) -> Boolean;
    pub fn CGMainDisplayID() -> u32;
    pub fn CGDisplayBounds(id: u32) -> CGRect;
    pub fn CGImageGetWidth(i: CGImageRef) -> usize;
    pub fn CGImageGetHeight(i: CGImageRef) -> usize;
    pub fn CGImageRelease(i: CGImageRef);
    pub fn CGColorSpaceCreateDeviceRGB() -> CGColorSpaceRef;
    pub fn CGColorSpaceRelease(c: CGColorSpaceRef);
    pub fn CGBitmapContextCreate(
        data: *mut c_void,
        width: usize,
        height: usize,
        bits_per_component: usize,
        bytes_per_row: usize,
        space: CGColorSpaceRef,
        bitmap_info: u32,
    ) -> CGContextRef;
    pub fn CGBitmapContextGetData(c: CGContextRef) -> *mut c_void;
    pub fn CGContextDrawImage(c: CGContextRef, rect: CGRect, image: CGImageRef);
    pub fn CGContextRelease(c: CGContextRef);
    pub fn CGEventSourceCreate(state: i32) -> CGEventSourceRef;
    pub fn CGEventSourceSecondsSinceLastEventType(state: i32, ty: u32) -> f64;
    pub fn CGEventCreateMouseEvent(
        source: CGEventSourceRef,
        ty: u32,
        pos: CGPoint,
        button: u32,
    ) -> CGEventRef;
    pub fn CGEventCreateKeyboardEvent(
        source: CGEventSourceRef,
        key: u16,
        down: Boolean,
    ) -> CGEventRef;
    pub fn CGEventCreateScrollWheelEvent2(
        source: CGEventSourceRef,
        units: u32,
        wheel_count: u32,
        w1: i32,
        w2: i32,
        w3: i32,
    ) -> CGEventRef;
    pub fn CGEventKeyboardSetUnicodeString(e: CGEventRef, len: usize, s: *const u16);
    pub fn CGEventSetFlags(e: CGEventRef, flags: u64);
    pub fn CGEventSetLocation(e: CGEventRef, p: CGPoint);
    pub fn CGEventSetIntegerValueField(e: CGEventRef, field: u32, v: i64);
    pub fn CGEventPost(tap: u32, e: CGEventRef);
    pub fn CGEventPostToPid(pid: c_int, e: CGEventRef);
    pub fn CGSessionCopyCurrentDictionary() -> CFDictionaryRef;
}

#[link(name = "Security", kind = "framework")]
unsafe extern "C" {
    pub fn SecStaticCodeCreateWithPath(path: CFURLRef, flags: u32, out: *mut *const c_void) -> i32;
    pub fn SecCodeCopySigningInformation(
        code: *const c_void,
        flags: u32,
        out: *mut CFDictionaryRef,
    ) -> i32;
}

unsafe extern "C" {
    pub fn proc_pidpath(pid: c_int, buf: *mut c_void, size: u32) -> c_int;
    pub fn proc_pidinfo(
        pid: c_int,
        flavor: c_int,
        arg: u64,
        buf: *mut c_void,
        size: c_int,
    ) -> c_int;
    pub fn proc_listallpids(buf: *mut c_void, size: c_int) -> c_int;
    pub fn geteuid() -> u32;
    pub fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
}

/// `RTLD_DEFAULT` on Darwin.
pub const RTLD_DEFAULT: *mut c_void = -2isize as *mut c_void;

/// `CGWindowListCreateImage(rect, option, window, image_option)`, resolved at
/// run time: the symbol is obsoleted in newer SDKs (macOS 15 moved to
/// ScreenCaptureKit) but present on the macOS versions measured; a missing
/// symbol is a typed capture failure, not a crash at launch.
pub type CGWindowListCreateImageFn = unsafe extern "C" fn(CGRect, u32, u32, u32) -> CGImageRef;
/// `_AXUIElementGetWindow(element, &window_number)`: private but stable for a
/// decade; resolved at run time, with a bounds-matching fallback.
pub type AxGetWindowFn = unsafe extern "C" fn(AXUIElementRef, *mut u32) -> AXError;
