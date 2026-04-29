# libhwbase/libgfxinit integration notes

## Upstream Ada projects

`vendor/libhwbase`:

- Provides low-level Ada hardware primitives (`HW.PCI`, `HW.MMIO_Range`,
  `HW.Port_IO`, timers, files/debug sinks).
- Generates `HW.Config` from `common/hw-config.ads.template` using `.config`.
- Builds static `libhw.a`.

`vendor/libgfxinit`:

- Depends on `libhwbase` via `libhw-dir`.
- Generates `HW.GFX.GMA.Config` from
  `common/hw-gfx-gma-config.ads.template` using `.config` values such as
  generation, PCH, panel ports, analog I2C port, and default MMIO base.
- Contains Intel GMA generation-specific code for G45, Ironlake, Haswell,
  Broxton, Skylake, Tigerlake, and Alderlake-family config.

The repo vendors these sources so a git dependency can build on another machine
without relying on `~/src/libhwbase` or `~/src/libgfxinit` paths.

## Coreboot pattern copied

Coreboot integrates the Ada libraries under these paths:

- `src/drivers/intel/gma/Makefile.mk`
- `src/drivers/intel/gma/libgfxinit.h`
- `src/drivers/intel/gma/gma.ads` / `gma.adb`
- `src/drivers/intel/gma/gma-gfx_init.ads`
- `src/drivers/intel/gma/hires_fb/gma-gfx_init.adb`
- `src/include/adainit.h`
- `src/lib/gnat/Makefile.mk`

Important behavior copied into `libgfxinit-sys`:

1. Generate Ada config packages before compiling.
2. Build libhwbase first, then libgfxinit.
3. Add a small Ada bridge that exports only:
   - `gma_gfxinit`
   - `gma_gfxstop`
   - `gma_read_edid`
4. Run GNAT binder and expose `gfxinit_adainit()` / `gfxinit_adafinal()`.
5. Receive framebuffer metadata through the Rust-provided
   `fb_add_framebuffer_info_simple` callback.

## Generation handling

Generation selection is done with mutually-exclusive Cargo features because a
dependency build script cannot reliably read the consuming package's
`package.metadata`.

Use exactly one for firmware builds:

- `gen-g45`
- `gen-ironlake`
- `gen-haswell`
- `gen-broxton`
- `gen-skylake`
- `gen-tigerlake`

CPU selection is left dynamic within the selected generation
(`CONFIG_GFX_GMA_DYN_CPU = y`). Board-specific values that are not good Cargo
features are environment variables: PCH, mainboard ports, panel ports, MMIO, and
MMCONF base.

## fstart integration sketch

A future fstart driver can be thin:

1. Depend on `libgfxinit-rs` with the correct `gen-*` feature.
2. Set board env values in `.cargo/config.toml` or the xtask build environment.
3. Call `unsafe { libgfxinit::ada_init_vendored() }` once during stage startup.
4. Call `libgfxinit::gfxinit()` from the display device init path.
5. Convert `libgfxinit::framebuffer_info()` into
   `fstart_services::framebuffer::FramebufferInfo`.

## Toolchain requirement

Ada artifacts used by this project should be produced by GNAT LLVM, not GCC
GNAT. The flake includes GCC GNAT only as the bootstrap compiler needed to build
AdaCore `gnat-llvm`; `libgfxinit-sys/build.rs` checks `ADA_CC` when Ada is built
or prebuilt Ada archives are linked.
