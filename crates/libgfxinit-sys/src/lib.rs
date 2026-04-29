//! Raw `no_std` FFI declarations for the coreboot-style libgfxinit ABI.

#![no_std]

use core::ffi::{c_int, c_uchar};

unsafe extern "C" {
    pub fn gma_gfxinit(lightup_ok: *mut c_int);
    pub fn gma_gfxstop();
    pub fn gma_read_edid(edid: *mut c_uchar, port: c_int) -> c_int;

    /// GNAT binder-generated elaboration entry for the vendored Ada archive.
    /// Must be called before `gma_*` functions when `libgfxinit-sys` builds the
    /// Ada sources itself.
    pub fn gfxinit_adainit();

    /// GNAT binder-generated finalization entry for the vendored Ada archive.
    pub fn gfxinit_adafinal();
}
