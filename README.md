# libgfxinit-rs

`no_std` Rust bindings for the Ada `libhwbase` + `libgfxinit` Intel GMA
implementation used by coreboot.

The intended consumer is eventually `~/src/fstart`: use libgfxinit as an Intel
GMA framebuffer driver without rewriting the Ada implementation.

## Dependency shape

For a firmware/no_std consumer, use one hardware generation feature:

```toml
[dependencies]
libgfxinit-rs = { git = "https://example.invalid/libgfxinit-rs", features = ["gen-haswell"] }
```

Available generation features:

- `gen-g45`
- `gen-ironlake`
- `gen-haswell`
- `gen-broxton`
- `gen-skylake`
- `gen-tigerlake`

Cargo features are additive, so `libgfxinit-sys` enforces that exactly one
`gen-*` feature is selected when Ada is built. The generation is a feature
because a dependency crate cannot reliably read the consuming package's
`package.metadata`; board-specific details are environment/config values.

Default features:

- `build-ada`: build vendored `libhwbase`/`libgfxinit` Ada sources for non-hosted
  targets.
- `fb-callback`: provide `fb_add_framebuffer_info_simple(...)` and store the last
  framebuffer description for Rust.

Hosted `linux`/`darwin`/`windows` targets skip the Ada build by default so unit
tests and `cargo check` work without a firmware Ada runtime. Set
`LIBGFXINIT_FORCE_BUILD_ADA=1` to force the Ada build on a hosted target.

## Board configuration

Use environment variables, typically in the consumer's `.cargo/config.toml`:

```toml
[env]
ADA_CC = "llvm-gcc"
ADA_BIND = "llvm-gnatbind"
LIBGFXINIT_MAINBOARD_PORTS = "eDP,HDMI1,DP1"
LIBGFXINIT_PCH = "Lynx_Point"
LIBGFXINIT_PANEL_1_PORT = "eDP"
LIBGFXINIT_PANEL_2_PORT = "Disabled"
LIBGFXINIT_ANALOG_I2C_PORT = "PCH_DAC"
LIBGFXINIT_DEFAULT_MMIO = "16\\#e000_0000\\#"
LIBGFXINIT_HWBASE_DEFAULT_MMCONF = "16\\#f000_0000\\#"
```

If not overridden, the build uses dynamic CPU detection inside the selected
generation and a generation-specific default PCH.

## Runtime usage

```rust
unsafe { libgfxinit::ada_init_vendored() };
libgfxinit::gfxinit()?;
let fb = libgfxinit::framebuffer_info();
```

The fstart driver should call `ada_init_vendored()` once during stage startup,
then call `gfxinit()`, read `framebuffer_info()`, and map it into
`fstart_services::framebuffer::FramebufferInfo`.

## LLVM Ada toolchain

Ada artifacts should be built with GNAT LLVM. Nixpkgs currently packages GCC
GNAT, not GNAT LLVM, so the flake provides bootstrap helpers:

```sh
nix develop
nix run .#build-gnat-llvm -- .gnat-llvm
export PATH="$PWD/.gnat-llvm/gnat-llvm/llvm-interface/bin:$PATH"
export ADA_CC=llvm-gcc
export ADA_BIND=llvm-gnatbind
nix run .#check-llvm-ada
```

## Useful commands

```sh
cargo test
cargo check --target x86_64-unknown-none --no-default-features --features fb-callback
nix build path:$PWD
nix flake check --no-build path:$PWD
```

## Notes

- The crate vendors `libhwbase` and `libgfxinit` sources so a git dependency can
  build without `~/src/libhwbase` or `~/src/libgfxinit` existing on the consuming
  machine.
- `link-prebuilt` is still available for integrations that want to build Ada
  archives outside Cargo and only use the Rust ABI wrapper.
