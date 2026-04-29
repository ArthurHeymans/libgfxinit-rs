//! Rust bindings for the small C ABI that coreboot layers over Ada libgfxinit.
//!
//! Upstream `libgfxinit` and `libhwbase` are Ada libraries.  They do not expose
//! a stable Rust or C API by themselves; coreboot adds a deliberately small C
//! ABI around the Ada packages and calls the GNAT binder's `*_adainit()` entry
//! before using it.  This crate models that ABI so firmware written in Rust can
//! call the same surface:
//!
//! - `gma_gfxinit(int *lightup_ok)`
//! - `gma_gfxstop(void)`
//! - `gma_read_edid(unsigned char edid[128], int port)`
//! - optionally, the Ada-imported `fb_add_framebuffer_info_simple(...)` callback
//!
//! By default the companion `libgfxinit-sys` crate builds Ada sources for
//! non-hosted firmware targets.  Sources come from `LIBHWBASE_SRC` /
//! `LIBGFXINIT_SRC`, or from the optional `libgfxinit-src` crate when the
//! `vendored` feature is enabled.  Hosted targets skip the Ada build unless
//! `LIBGFXINIT_FORCE_BUILD_ADA=1` is set, which keeps ordinary Rust unit tests
//! usable without a firmware Ada runtime.

#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]

use core::ffi::c_int;

/// Raw EDID block size used by libgfxinit.
pub const EDID_BLOCK_LEN: usize = 128;

/// Intel GMA connector port values from coreboot's `libgfxinit.h`.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[repr(i32)]
pub enum Port {
    Disabled = 0,
    Lvds = 1,
    Edp = 2,
    Dp1 = 3,
    Dp2 = 4,
    Dp3 = 5,
    Hdmi1 = 6,
    Hdmi2 = 7,
    Hdmi3 = 8,
    Analog = 9,
}

impl Port {
    /// Return the integer ABI value expected by `gma_read_edid`.
    #[inline]
    pub const fn as_raw(self) -> c_int {
        self as c_int
    }
}

impl TryFrom<c_int> for Port {
    type Error = InvalidPort;

    fn try_from(value: c_int) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Disabled),
            1 => Ok(Self::Lvds),
            2 => Ok(Self::Edp),
            3 => Ok(Self::Dp1),
            4 => Ok(Self::Dp2),
            5 => Ok(Self::Dp3),
            6 => Ok(Self::Hdmi1),
            7 => Ok(Self::Hdmi2),
            8 => Ok(Self::Hdmi3),
            9 => Ok(Self::Analog),
            raw => Err(InvalidPort(raw)),
        }
    }
}

/// Invalid raw port value.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct InvalidPort(pub c_int);

/// One raw EDID block.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Edid(pub [u8; EDID_BLOCK_LEN]);

/// Errors returned by the safe Rust wrappers.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Error {
    /// `gma_gfxinit` returned `lightup_ok == 0`.
    DisplayInitFailed,
    /// `gma_read_edid` returned `-1`.
    EdidProbeFailed,
    /// `gma_read_edid` returned `-2` for a port outside libgfxinit's active
    /// port range.
    InvalidPort,
    /// `gma_read_edid` returned an undocumented status code.
    UnexpectedStatus(c_int),
}

/// Call the GNAT binder-generated elaboration function.
///
/// Ada code must be elaborated before any exported Ada subprogram is called.
/// Coreboot does this with stage-specific functions such as
/// `romstage_adainit()` and `ramstage_adainit()`.  fstart should do the same
/// from its stage startup path before creating a display driver that calls
/// [`gfxinit`], [`gfxstop`], or [`read_edid`].
///
/// # Safety
///
/// `init` must be the correct GNAT binder initialization function for the exact
/// Ada objects linked into this firmware image, and it must be called at most as
/// often as that runtime supports.
#[inline]
pub unsafe fn ada_init(init: unsafe extern "C" fn()) {
    // SAFETY: upheld by the caller, see function safety contract.
    unsafe { init() };
}

/// Call the GNAT binder-generated elaboration function from the vendored Ada
/// archive built by `libgfxinit-sys`.
///
/// # Safety
///
/// Same contract as [`ada_init`]: call this before any other Ada-exported
/// libgfxinit symbol, and only when linking the vendored Ada archive.
#[inline]
pub unsafe fn ada_init_vendored() {
    // SAFETY: upheld by the caller, see function safety contract.
    unsafe { ffi::gfxinit_adainit() };
}

/// Call the GNAT binder-generated finalization function from the vendored Ada
/// archive built by `libgfxinit-sys`.
///
/// # Safety
///
/// Must only be called after successful Ada elaboration and when the runtime is
/// in a state where GNAT finalization is valid.
#[inline]
pub unsafe fn ada_final_vendored() {
    // SAFETY: upheld by the caller, see function safety contract.
    unsafe { ffi::gfxinit_adafinal() };
}

/// Initialize the display using libgfxinit.
pub fn gfxinit() -> Result<(), Error> {
    let mut lightup_ok = 0;
    // SAFETY: `gma_gfxinit` is an Ada-exported C ABI function.  The pointer is
    // valid for the duration of the call.  The caller/integration is responsible
    // for calling `ada_init` before this safe wrapper is used.
    unsafe { ffi::gma_gfxinit(&mut lightup_ok) };

    if lightup_ok != 0 {
        Ok(())
    } else {
        Err(Error::DisplayInitFailed)
    }
}

/// Disable libgfxinit-programmed outputs.
pub fn gfxstop() {
    // SAFETY: see `gfxinit`; no Rust-side invariants are required.
    unsafe { ffi::gma_gfxstop() };
}

/// Read the EDID block from a libgfxinit GMA port.
pub fn read_edid(port: Port) -> Result<Edid, Error> {
    let mut edid = [0u8; EDID_BLOCK_LEN];
    // SAFETY: `edid` is a valid 128-byte output buffer and `port` is one of the
    // ABI values from coreboot's `libgfxinit.h`.
    let status = unsafe { ffi::gma_read_edid(edid.as_mut_ptr(), port.as_raw()) };

    match status {
        0 => Ok(Edid(edid)),
        -1 => Err(Error::EdidProbeFailed),
        -2 => Err(Error::InvalidPort),
        other => Err(Error::UnexpectedStatus(other)),
    }
}

/// Framebuffer description passed by the Ada `hires_fb` bridge.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct FramebufferInfo {
    /// Physical address of the linear framebuffer.
    pub base_addr: usize,
    /// Horizontal visible pixels.
    pub width: u32,
    /// Vertical visible pixels.
    pub height: u32,
    /// Bytes between two adjacent scanlines.
    pub bytes_per_line: u32,
    /// Bits per pixel.  libgfxinit's coreboot `hires_fb` path currently passes
    /// 32 for XRGB8888-like linear framebuffer output.
    pub bits_per_pixel: u8,
}

impl FramebufferInfo {
    /// Pixels per scanline, if `bits_per_pixel` divides the line stride.
    #[inline]
    pub const fn stride_pixels(self) -> Option<u32> {
        if self.bits_per_pixel == 0 {
            return None;
        }
        let bytes_per_pixel = (self.bits_per_pixel as u32).div_ceil(8);
        if bytes_per_pixel == 0 || !self.bytes_per_line.is_multiple_of(bytes_per_pixel) {
            None
        } else {
            Some(self.bytes_per_line / bytes_per_pixel)
        }
    }
}

#[cfg(feature = "fb-callback")]
mod fb_callback {
    use super::{c_int, FramebufferInfo};
    use core::cell::UnsafeCell;

    struct FramebufferState(UnsafeCell<Option<FramebufferInfo>>);

    // SAFETY: firmware initialization is single-threaded for the intended use.
    // Consumers that call into libgfxinit concurrently must provide external
    // synchronization around `gma_gfxinit` and these accessors.
    unsafe impl Sync for FramebufferState {}

    static FRAMEBUFFER: FramebufferState = FramebufferState(UnsafeCell::new(None));

    pub fn framebuffer_info() -> Option<FramebufferInfo> {
        // SAFETY: see `FramebufferState` Sync justification above.  The value is
        // `Copy`, so no reference to the static escapes.
        unsafe { *FRAMEBUFFER.0.get() }
    }

    pub fn take_framebuffer_info() -> Option<FramebufferInfo> {
        // SAFETY: see `FramebufferState` Sync justification above.
        unsafe { (*FRAMEBUFFER.0.get()).take() }
    }

    /// Callback imported by coreboot's `hires_fb/gma-gfx_init.adb` bridge.
    ///
    /// Returning non-zero tells the Ada side that the framebuffer was accepted.
    #[no_mangle]
    pub extern "C" fn fb_add_framebuffer_info_simple(
        fb_addr: usize,
        x_resolution: u32,
        y_resolution: u32,
        bytes_per_line: u32,
        bits_per_pixel: u8,
    ) -> c_int {
        let info = FramebufferInfo {
            base_addr: fb_addr,
            width: x_resolution,
            height: y_resolution,
            bytes_per_line,
            bits_per_pixel,
        };

        // SAFETY: see `FramebufferState` Sync justification above.
        unsafe {
            *FRAMEBUFFER.0.get() = Some(info);
        }
        1
    }
}

#[cfg(feature = "fb-callback")]
pub use fb_callback::{framebuffer_info, take_framebuffer_info};

/// Raw C ABI declarations from `libgfxinit-sys`.
pub mod ffi {
    pub use libgfxinit_sys::{
        gfxinit_adafinal, gfxinit_adainit, gma_gfxinit, gma_gfxstop, gma_read_edid,
    };
}

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_values_match_coreboot_header() {
        assert_eq!(Port::Disabled.as_raw(), 0);
        assert_eq!(Port::Lvds.as_raw(), 1);
        assert_eq!(Port::Edp.as_raw(), 2);
        assert_eq!(Port::Dp1.as_raw(), 3);
        assert_eq!(Port::Dp2.as_raw(), 4);
        assert_eq!(Port::Dp3.as_raw(), 5);
        assert_eq!(Port::Hdmi1.as_raw(), 6);
        assert_eq!(Port::Hdmi2.as_raw(), 7);
        assert_eq!(Port::Hdmi3.as_raw(), 8);
        assert_eq!(Port::Analog.as_raw(), 9);
        assert_eq!(Port::try_from(10), Err(InvalidPort(10)));
    }

    #[test]
    fn stride_pixels_uses_bytes_per_line() {
        let info = FramebufferInfo {
            base_addr: 0x1000,
            width: 1024,
            height: 768,
            bytes_per_line: 4096,
            bits_per_pixel: 32,
        };
        assert_eq!(info.stride_pixels(), Some(1024));
    }

    #[cfg(feature = "fb-callback")]
    #[test]
    fn framebuffer_callback_records_last_info() {
        assert_eq!(take_framebuffer_info(), None);
        let accepted =
            crate::fb_callback::fb_add_framebuffer_info_simple(0xfeed_0000, 800, 600, 3200, 32);
        assert_eq!(accepted, 1);
        assert_eq!(
            framebuffer_info(),
            Some(FramebufferInfo {
                base_addr: 0xfeed_0000,
                width: 800,
                height: 600,
                bytes_per_line: 3200,
                bits_per_pixel: 32,
            })
        );
    }
}
