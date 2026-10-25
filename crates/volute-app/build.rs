const ICON: &str = "../../assets/brand/icons/volute.ico";
/// The tray's speaker (decision 0013), in the order of `volute_shell::TrayGlyph::index`: no wave,
/// one, two, three, muted. White for a dark taskbar, ink for a light one.
const TRAY: [&str; 5] = ["0", "1", "2", "3", "muted"];

fn main() {
    // French is bundled; source strings are English, without contexts.
    let config = slint_build::CompilerConfiguration::new()
        .with_style("fluent".into())
        .with_bundled_translations("translations")
        .with_default_translation_context(slint_build::DefaultTranslationContext::None);
    slint_build::compile_with_config("ui/app.slint", config).expect("Slint UI compiles");

    // Explorer, the taskbar, Alt+Tab and the tray read the icons from the executable's resources:
    // ID 1 the application icon (winresource gives the first icon that ID), IDs 10 to 14 the
    // white tray speaker, 20 to 24 the ink one. The target OS, not the host's: `cfg!(target_os)`
    // would describe the build script.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed={ICON}");
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon(ICON);
        for (base, colour) in [(10, "white"), (20, "ink")] {
            for (offset, state) in TRAY.iter().enumerate() {
                let path = format!("../../assets/brand/icons/tray/speaker-{colour}-{state}.ico");
                println!("cargo:rerun-if-changed={path}");
                resource.set_icon_with_id(&path, &(base + offset).to_string());
            }
        }
        resource.set("FileDescription", "Volute");
        resource.set("ProductName", "Volute");
        resource.compile().expect("Windows resources compile");
    }
}
