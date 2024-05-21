use bindgen::{builder, EnumVariation, MacroTypeVariation};
use bindgen::callbacks::{EnumVariantCustomBehavior, EnumVariantValue};
use std::env;


#[derive(Debug)]
struct QEMUClockTypeParseCallback {}

impl bindgen::callbacks::ParseCallbacks for QEMUClockTypeParseCallback {
    fn enum_variant_behavior(&self, enum_name: Option<&str>, original_variant_name: &str, _variant_value: EnumVariantValue) -> Option<EnumVariantCustomBehavior> {
        if let Some(enum_name) = enum_name {
            if enum_name == "QEMUClockType" && original_variant_name == "QEMU_CLOCK_MAX" {
                return Some(EnumVariantCustomBehavior::Hide);
            }
        }
        None
    }
}

fn main() -> std::io::Result<()> {
    // Generate bindings for qemu timers
    let qemu_src = env!("QEMU_SRC", "Environment misses QEMU_SRC pointing at Qemu source tree");
    let qemu_build = option_env!("QEMU_BUILD").unwrap_or(qemu_src);
    let pkg_config = option_env!("PKG_CONFIG").unwrap_or("pkg-config");

    let qemu_timer = String::from(qemu_src) + "/include/qemu/timer.h";
    let glib_args = std::process::Command::new(pkg_config)
        .args(&["--cflags", "glib-2.0"])
        .output()
        .expect("Failed to configure C flags for glib-2.0");
    let glib_args = String::from_utf8(glib_args.stdout)
        .expect("Unable to read pkg-config output");
    let glib_args = glib_args.split_ascii_whitespace();

    let bindings = builder().header(&qemu_timer)
        .whitelist_function("qemu_clock_get_ns")
        .whitelist_function("timer_init_full")
        .whitelist_function("timer_deinit")
        .whitelist_function("timer_del")
        .whitelist_function("timer_mod")
        .default_enum_style(EnumVariation::Rust { non_exhaustive: false })
        .no_debug("QEMUTimerList.*")
        .no_copy("QEMUTimer.*")
        .default_macro_constant_type(MacroTypeVariation::Signed)
        .whitelist_var("SCALE_NS")
        .clang_args(&["-I", qemu_build,
                    "-I", &(String::from(qemu_src) + "/include"),
                    "--include", &(String::from(qemu_src) + "/include/qemu/osdep.h")])
        .clang_args(glib_args)
        .parse_callbacks(Box::new(QEMUClockTypeParseCallback {}))
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .generate()
        .expect(&format!("Failed to generate bindings to {}", qemu_timer));

    let out_dir = env::var("OUT_DIR")
        .expect("OUT_DIR environment variable is not defined");
    let qemu_timer_sys = out_dir.clone() + "/qemu-timer-sys.rs";
    bindings.write_to_file(&qemu_timer_sys)
        .expect(&format!("Failed to write bindings to {} in {}", qemu_timer, qemu_timer_sys));

    println!("cargo:rerun-if-env-changed=QEMU_SRC");
    println!("cargo:rerun-if-changed={}", qemu_timer);

    Ok(())
}
