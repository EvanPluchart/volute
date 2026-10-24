const ICON: &str = "../../assets/brand/icons/volute.ico";
const TRAY_MUTED: &str = "../../assets/brand/icons/tray-muted.ico";

fn main() {
    // French is bundled; source strings are English, without contexts.
    let config = slint_build::CompilerConfiguration::new()
        .with_style("fluent".into())
        .with_bundled_translations("translations")
        .with_default_translation_context(slint_build::DefaultTranslationContext::None);
    slint_build::compile_with_config("ui/app.slint", config).expect("Slint UI compiles");

    // Explorer, the taskbar, Alt+Tab and the tray read the icons from the executable's resources:
    // ID 1 the application icon (winresource gives the first icon that ID), ID 2 the muted tray
    // icon. The target OS, not the host's: `cfg!(target_os)` would describe the build script.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed={ICON}");
        println!("cargo:rerun-if-changed={TRAY_MUTED}");
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon(ICON);
        resource.set_icon_with_id(TRAY_MUTED, "2");
        resource.set("FileDescription", "Volute");
        resource.set("ProductName", "Volute");
        resource.compile().expect("Windows resources compile");
    }
}
