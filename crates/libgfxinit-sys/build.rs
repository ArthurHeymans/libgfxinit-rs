use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    rerun_env();

    let build_ada = env::var_os("CARGO_FEATURE_BUILD_ADA").is_some();
    let link_prebuilt = env::var_os("CARGO_FEATURE_LINK_PREBUILT").is_some();
    let check_llvm = env::var_os("CARGO_FEATURE_LLVM_ADA_TOOLCHAIN").is_some();

    let should_build_ada = build_ada && should_build_ada_for_target();

    if check_llvm && (should_build_ada || link_prebuilt) {
        let ada_cc = env::var("ADA_CC").unwrap_or_else(|_| "llvm-gcc".to_owned());
        verify_llvm_ada_compiler(&ada_cc);
    }

    if link_prebuilt {
        link_prebuilt_archives();
        return;
    }

    if should_build_ada {
        build_ada_archive();
    } else if build_ada {
        println!("cargo:warning=libgfxinit-sys: skipping Ada build for hosted target; set LIBGFXINIT_FORCE_BUILD_ADA=1 to force it");
    }
}

fn rerun_env() {
    for name in [
        "ADA_CC",
        "ADA_BIND",
        "AR",
        "LIBGFXINIT_FORCE_BUILD_ADA",
        "LIBHWBASE_SRC",
        "LIBGFXINIT_SRC",
        "LIBGFXINIT_GENERATION",
        "LIBGFXINIT_PCH",
        "LIBGFXINIT_PANEL_1_PORT",
        "LIBGFXINIT_PANEL_2_PORT",
        "LIBGFXINIT_ANALOG_I2C_PORT",
        "LIBGFXINIT_DEFAULT_MMIO",
        "LIBGFXINIT_MAINBOARD_PORTS",
        "LIBGFXINIT_HWBASE_DEFAULT_MMCONF",
        "LIBGFXINIT_HWBASE_DYNAMIC_MMIO",
        "LIBGFXINIT_LIB_DIR",
        "LIBHWBASE_LIB_DIR",
        "GNAT_RTS_LIB_DIR",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
    }

    println!("cargo:rerun-if-changed=build_support/bridge");
}

fn should_build_ada_for_target() -> bool {
    if env_truthy("LIBGFXINIT_FORCE_BUILD_ADA") {
        return true;
    }

    let target = env::var("TARGET").unwrap_or_default();
    !(target.contains("linux")
        || target.contains("darwin")
        || target.contains("windows")
        || target.contains("freebsd"))
}

fn verify_llvm_ada_compiler(ada_cc: &str) {
    if !ada_cc.contains("llvm") {
        panic!("ADA_CC must name the GNAT LLVM compiler (expected llvm-gcc, got {ada_cc:?})");
    }

    let output = Command::new(ada_cc)
        .arg("--version")
        .output()
        .unwrap_or_else(|err| panic!("failed to execute Ada compiler {ada_cc:?}: {err}"));

    if !output.status.success() {
        panic!("{ada_cc:?} --version exited with {}", output.status);
    }

    let mut version = String::from_utf8_lossy(&output.stdout).into_owned();
    version.push_str(&String::from_utf8_lossy(&output.stderr));

    if !version.to_ascii_lowercase().contains("llvm") {
        panic!("{ada_cc:?} does not appear to be GNAT LLVM; --version output was:\n{version}");
    }
}

fn build_ada_archive() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let work = out_dir.join("ada-build");
    let (hw_src, gfx_src) = configured_source_paths();
    let bridge_src = manifest_dir.join("build_support/bridge");
    let hw_work = work.join("libhwbase");
    let gfx_work = work.join("libgfxinit");

    recreate_dir(&work);
    copy_dir(hw_src.as_path(), hw_work.as_path())
        .unwrap_or_else(|err| panic!("copying libhwbase source failed: {err}"));
    copy_dir(gfx_src.as_path(), gfx_work.as_path())
        .unwrap_or_else(|err| panic!("copying libgfxinit source failed: {err}"));

    let hw_config = hw_work.join(".config");
    let gfx_config = gfx_work.join(".config");
    write_hwbase_config(hw_config.as_path());
    write_gfxinit_config(gfx_config.as_path());
    add_rust_bridge(gfx_work.as_path(), bridge_src.as_path());

    let ada_cc = env::var("ADA_CC").unwrap_or_else(|_| "llvm-gcc".to_owned());
    let ada_bind = env::var("ADA_BIND").unwrap_or_else(|_| "llvm-gnatbind".to_owned());
    let ar = env::var("AR").unwrap_or_else(|_| "ar".to_owned());

    run_make(
        &hw_work,
        [
            ("CC", ada_cc.as_str()),
            ("GNATBIND", ada_bind.as_str()),
            ("AR", ar.as_str()),
            ("DESTDIR", "dest"),
        ],
        "install",
    );

    let libhw_dir = hw_work.join("dest");
    run_make(
        &gfx_work,
        [
            ("CC", ada_cc.as_str()),
            ("GNATBIND", ada_bind.as_str()),
            ("AR", ar.as_str()),
            ("DESTDIR", "dest"),
            ("libhw-dir", libhw_dir.to_str().unwrap()),
        ],
        "install",
    );

    let gfx_lib = gfx_work.join("dest/lib");
    let hw_lib = hw_work.join("dest/lib");
    println!("cargo:rustc-link-search=native={}", gfx_lib.display());
    println!("cargo:rustc-link-search=native={}", hw_lib.display());
    if let Some(rts_dir) = env::var_os("GNAT_RTS_LIB_DIR") {
        println!(
            "cargo:rustc-link-search=native={}",
            PathBuf::from(rts_dir).display()
        );
    }

    println!("cargo:rustc-link-lib=static=gfxinit");
    println!("cargo:rustc-link-lib=static=hw");
    if env::var_os("CARGO_FEATURE_HOSTED_GNAT_RUNTIME").is_some() {
        println!("cargo:rustc-link-lib=gnat");
    }
}

fn add_rust_bridge(gfx_work: &Path, bridge_src: &Path) {
    let bridge = gfx_work.join("rust_bridge");
    fs::create_dir_all(&bridge).unwrap();
    for file in ["gma.ads", "gma.adb", "gma-gfx_init.ads", "gma-gfx_init.adb"] {
        fs::copy(bridge_src.join(file), bridge.join(file))
            .unwrap_or_else(|err| panic!("copying bridge file {file} failed: {err}"));
    }

    let ports = env::var("LIBGFXINIT_MAINBOARD_PORTS")
        .map(|ports| format_ports(&ports))
        .unwrap_or_else(|_| "All_Ports".to_owned());
    fs::write(
        bridge.join("gma-mainboard.ads"),
        format!(
            "with HW.GFX.GMA;\nwith HW.GFX.GMA.Display_Probing;\n\nuse HW.GFX.GMA;\nuse HW.GFX.GMA.Display_Probing;\n\npackage GMA.Mainboard is\n   ports : constant Port_List := {ports};\nend GMA.Mainboard;\n"
        ),
    )
    .unwrap();

    fs::write(
        bridge.join("Makefile.inc"),
        "gfxinit-y += gma.ads\ngfxinit-y += gma.adb\ngfxinit-y += gma-mainboard.ads\ngfxinit-y += gma-gfx_init.ads\ngfxinit-y += gma-gfx_init.adb\n",
    )
    .unwrap();

    let makefile_inc = gfx_work.join("Makefile.inc");
    let mut contents = fs::read_to_string(&makefile_inc).unwrap();
    contents.push_str("\nsubdirs-y += rust_bridge\n");
    fs::write(makefile_inc, contents).unwrap();
}

fn format_ports(ports: &str) -> String {
    let ports = ports
        .split(',')
        .map(str::trim)
        .filter(|port| !port.is_empty())
        .collect::<Vec<_>>();
    if ports.is_empty() {
        "All_Ports".to_owned()
    } else {
        format!("({}, others => Disabled)", ports.join(", "))
    }
}

fn write_hwbase_config(path: &Path) {
    let dynamic_mmio = env_truthy("LIBGFXINIT_HWBASE_DYNAMIC_MMIO");
    let default_mmconf = env::var("LIBGFXINIT_HWBASE_DEFAULT_MMCONF")
        .unwrap_or_else(|_| "16\\#f000_0000\\#".to_owned());
    let config = format!(
        "CONFIG_HWBASE_DEBUG_NULL = y\n\
         CONFIG_HWBASE_DEBUG_TEXT_IO =\n\
         CONFIG_HWBASE_STATIC_MMIO = {}\n\
         CONFIG_HWBASE_DYNAMIC_MMIO = {}\n\
         CONFIG_HWBASE_TIMER_CLOCK_GETTIME = y\n\
         CONFIG_HWBASE_TIMER_MUTIME =\n\
         CONFIG_HWBASE_POSIX_FILE =\n\
         CONFIG_HWBASE_DEFAULT_MMCONF = {default_mmconf}\n\
         CONFIG_HWBASE_DIRECT_PCIDEV = y\n\
         CONFIG_HWBASE_LINUX_PCIDEV =\n",
        if dynamic_mmio { "" } else { "y" },
        if dynamic_mmio { "y" } else { "" },
    );
    fs::write(path, config).unwrap();
}

fn write_gfxinit_config(path: &Path) {
    let generation = env::var("LIBGFXINIT_GENERATION").unwrap_or_else(|_| selected_generation());
    let pch = env::var("LIBGFXINIT_PCH").unwrap_or_else(|_| default_pch(&generation).to_owned());
    let panel_1 = env::var("LIBGFXINIT_PANEL_1_PORT").unwrap_or_else(|_| "eDP".to_owned());
    let panel_2 = env::var("LIBGFXINIT_PANEL_2_PORT").unwrap_or_else(|_| "Disabled".to_owned());
    let analog_i2c =
        env::var("LIBGFXINIT_ANALOG_I2C_PORT").unwrap_or_else(|_| "PCH_DAC".to_owned());
    let default_mmio =
        env::var("LIBGFXINIT_DEFAULT_MMIO").unwrap_or_else(|_| "16\\#e000_0000\\#".to_owned());

    let config = format!(
        "CONFIG_GFX_GMA_DYN_CPU = y\n\
         CONFIG_GFX_GMA_GENERATION = {generation}\n\
         CONFIG_GFX_GMA_PCH = {pch}\n\
         CONFIG_GFX_GMA_PANEL_1_PORT = {panel_1}\n\
         CONFIG_GFX_GMA_PANEL_2_PORT = {panel_2}\n\
         CONFIG_GFX_GMA_ANALOG_I2C_PORT = {analog_i2c}\n\
         CONFIG_GFX_GMA_DEFAULT_MMIO = {default_mmio}\n",
    );
    fs::write(path, config).unwrap();
}

fn selected_generation() -> String {
    let generations = [
        ("CARGO_FEATURE_GEN_G45", "G45"),
        ("CARGO_FEATURE_GEN_IRONLAKE", "Ironlake"),
        ("CARGO_FEATURE_GEN_HASWELL", "Haswell"),
        ("CARGO_FEATURE_GEN_BROXTON", "Broxton"),
        ("CARGO_FEATURE_GEN_SKYLAKE", "Skylake"),
        ("CARGO_FEATURE_GEN_TIGERLAKE", "Tigerlake"),
    ];
    let selected = generations
        .iter()
        .filter_map(|(feature, name)| env::var_os(feature).map(|_| *name))
        .collect::<Vec<_>>();

    match selected.as_slice() {
        [one] => (*one).to_owned(),
        [] => panic!(
            "select exactly one libgfxinit generation feature when building Ada: gen-g45, gen-ironlake, gen-haswell, gen-broxton, gen-skylake, or gen-tigerlake; alternatively set LIBGFXINIT_GENERATION"
        ),
        many => panic!("libgfxinit generation features are mutually exclusive; selected {many:?}"),
    }
}

fn default_pch(generation: &str) -> &'static str {
    match generation {
        "G45" => "No_PCH",
        "Ironlake" => "Ibex_Peak",
        "Haswell" => "Lynx_Point",
        "Broxton" => "No_PCH",
        "Skylake" => "Sunrise_Point",
        "Tigerlake" => "Tiger_Point",
        _ => "No_PCH",
    }
}

#[cfg(feature = "vendored")]
fn configured_source_paths() -> (PathBuf, PathBuf) {
    let sources = libgfxinit_src::Sources::new();
    let hw = sources.libhwbase().to_path_buf();
    let gfx = sources.libgfxinit().to_path_buf();
    println!("cargo:rerun-if-changed={}", hw.display());
    println!("cargo:rerun-if-changed={}", gfx.display());
    (hw, gfx)
}

#[cfg(not(feature = "vendored"))]
fn configured_source_paths() -> (PathBuf, PathBuf) {
    let hw = env_path_with_message(
        "LIBHWBASE_SRC",
        "LIBHWBASE_SRC must point at a libhwbase checkout, or enable the `vendored` feature",
    );
    let gfx = env_path_with_message(
        "LIBGFXINIT_SRC",
        "LIBGFXINIT_SRC must point at a libgfxinit checkout, or enable the `vendored` feature",
    );
    println!("cargo:rerun-if-changed={}", hw.display());
    println!("cargo:rerun-if-changed={}", gfx.display());
    (hw, gfx)
}

fn link_prebuilt_archives() {
    let gfx_dir = env_path("LIBGFXINIT_LIB_DIR");
    let hw_dir = env::var_os("LIBHWBASE_LIB_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| gfx_dir.clone());

    println!("cargo:rustc-link-search=native={}", gfx_dir.display());
    if hw_dir != gfx_dir {
        println!("cargo:rustc-link-search=native={}", hw_dir.display());
    }
    if let Some(rts_dir) = env::var_os("GNAT_RTS_LIB_DIR") {
        println!(
            "cargo:rustc-link-search=native={}",
            PathBuf::from(rts_dir).display()
        );
    }

    println!("cargo:rustc-link-lib=static=gfxinit");
    println!("cargo:rustc-link-lib=static=hw");
    if env::var_os("CARGO_FEATURE_HOSTED_GNAT_RUNTIME").is_some() {
        println!("cargo:rustc-link-lib=gnat");
    }
}

fn env_path(name: &str) -> PathBuf {
    env_path_with_message(
        name,
        &format!("{name} must be set when feature link-prebuilt is enabled"),
    )
}

fn env_path_with_message(name: &str, message: &str) -> PathBuf {
    env::var_os(name)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("{message}"))
}

fn env_truthy(name: &str) -> bool {
    match env::var(name) {
        Ok(value) => matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "y" | "Y"),
        Err(_) => false,
    }
}

fn recreate_dir(path: &Path) {
    let _ = fs::remove_dir_all(path);
    fs::create_dir_all(path).unwrap();
}

fn copy_dir(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let name = entry.file_name();
        if ignored(&name) {
            continue;
        }
        let dst = to.join(&name);
        if ty.is_dir() {
            let src = entry.path();
            copy_dir(src.as_path(), dst.as_path())?;
        } else if ty.is_file() {
            fs::copy(entry.path(), dst)?;
        }
    }
    Ok(())
}

fn ignored(name: &OsStr) -> bool {
    match name.to_str() {
        Some(name) => matches!(name, ".git" | "build" | "dest" | ".config" | ".opencode"),
        None => false,
    }
}

fn run_make<const N: usize>(dir: &Path, envs: [(&str, &str); N], target: &str) {
    let mut cmd = Command::new("make");
    cmd.current_dir(dir).arg(target).arg("V=1");
    for (key, value) in envs {
        cmd.env(key, value);
    }
    let status = cmd
        .status()
        .unwrap_or_else(|err| panic!("failed to run make in {}: {err}", dir.display()));
    if !status.success() {
        panic!("make {target} failed in {} with {status}", dir.display());
    }
}
