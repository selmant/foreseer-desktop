use base64::Engine;
use directories::ProjectDirs;
use foreseer_desktop::config::{AppConfig, validate_foreseer_url};
use foreseer_desktop::extension::ForeseerExtension;
use jfn_rust::{HostExtensionDescriptor, HostOptions};
use std::sync::Arc;

mod setup;

fn main() {
    configure_product_profile();
    let cli_requested_setup = handle_cli_args();

    let config_exists = AppConfig::exists();
    let config = AppConfig::load();
    let needs_setup = cli_requested_setup || !config_exists || !config.is_configured();

    let frontend_script = include_str!("assets/foreseer-native.js")
        .replace("__HOST_VERSION__", env!("CARGO_PKG_VERSION"));
    let primary_web_script = include_str!("assets/jellyfin-session.js").to_string();

    let (descriptor, frontend_url, allow_insecure_http) = if needs_setup {
        let setup_html = setup::get_setup_html("");
        let base64_html = base64::engine::general_purpose::STANDARD.encode(setup_html);
        let url = format!("data:text/html;base64,{base64_html}");
        let descriptor = HostExtensionDescriptor::from_setup_document(
            &url,
            vec![frontend_script],
            vec![primary_web_script],
            false,
        )
        .expect("generated setup document");
        (descriptor, url, true)
    } else {
        let url = validate_foreseer_url(&config.server_url, config.allow_insecure_http)
            .expect("validated configured Foreseer URL");
        let descriptor = HostExtensionDescriptor::from_url(
            &url,
            vec![frontend_script],
            vec![primary_web_script],
            false,
        )
        .expect("validated configured Foreseer URL");
        (descriptor, url, config.allow_insecure_http)
    };

    let extension: Arc<dyn jfn_rust::HostExtension> =
        ForeseerExtension::new(descriptor, frontend_url, allow_insecure_http, needs_setup);
    let options = HostOptions::with_extension(extension);
    std::process::exit(jfn_rust::app::jfn_app_main_with(options));
}

fn handle_cli_args() -> bool {
    let args: Vec<String> = std::env::args().collect();
    if args.len() <= 1 {
        return false;
    }

    match args[1].as_str() {
        "--setup" => return true,
        "--help" | "-h" => {
            println!("Foreseer Desktop Client");
            println!();
            println!("USAGE:");
            println!("  foreseer-desktop [OPTIONS]");
            println!();
            println!("OPTIONS:");
            println!("  --setup            Open the graphical server setup page");
            println!("  --set-url <URL>    Set target Foreseer server URL in config file and exit");
            println!("  --allow-http       Allow insecure HTTP when saving server URL");
            println!("  --show-config      Display current config file path and settings");
            println!("  --help, -h         Show this help message");
            std::process::exit(0);
        }
        "--show-config" => {
            let path = AppConfig::config_file_path();
            let config = AppConfig::load();
            println!(
                "Config Path: {}",
                path.map(|p| p.display().to_string())
                    .unwrap_or_else(|| "Unknown".into())
            );
            println!("Server URL:  {}", config.server_url);
            println!("Allow HTTP:  {}", config.allow_insecure_http);
            std::process::exit(0);
        }
        "--set-url" => {
            if args.len() < 3 {
                eprintln!(
                    "Error: --set-url requires a URL argument (e.g. --set-url https://my-server.com)"
                );
                std::process::exit(1);
            }
            let url = args[2].clone();
            let allow_http = args.iter().any(|arg| arg == "--allow-http");
            let url = match validate_foreseer_url(&url, allow_http) {
                Ok(url) => url,
                Err(error) => {
                    eprintln!("Error: {}", error.message());
                    std::process::exit(1);
                }
            };

            let mut config = AppConfig::load();
            config.server_url = url;
            config.allow_insecure_http = allow_http;
            if let Err(e) = config.save() {
                eprintln!("Error saving config: {}", e);
                std::process::exit(1);
            }
            println!("Successfully saved server URL to config.");
            std::process::exit(0);
        }
        _ => {}
    }
    false
}

fn configure_product_profile() {
    set_default_env("JELLIUM_DESKTOP_TITLE", "Foreseer".as_ref());
    set_default_env(
        "JELLIUM_DESKTOP_APP_ID",
        "com.selmantrabzon.Foreseer".as_ref(),
    );
    mirror_env("FORESEER_LOG_LEVEL", "JELLIUM_DESKTOP_LOG_LEVEL");
    mirror_env("FORESEER_LOG_FILE", "JELLIUM_DESKTOP_LOG_FILE");
    mirror_env("FORESEER_CONFIG_DIR", "JELLIUM_DESKTOP_CONFIG_DIR");
    mirror_env("FORESEER_CACHE_DIR", "JELLIUM_DESKTOP_CACHE_DIR");
    mirror_env("FORESEER_PLATFORM_PAINT", "JELLIUM_DESKTOP_PLATFORM_PAINT");
    mirror_env("FORESEER_MPV_HOME", "MPV_HOME");
    set_default_env(
        "JELLIUM_DESKTOP_HOST_VERSION",
        env!("CARGO_PKG_VERSION").as_ref(),
    );
    set_default_env(
        "JELLIUM_DESKTOP_HOST_JELLIUM_REVISION",
        include_str!("../jellium.rev").trim().as_ref(),
    );
    if let Some(project_dirs) = ProjectDirs::from("com", "selmantrabzon", "Foreseer") {
        set_default_env(
            "JELLIUM_DESKTOP_CONFIG_DIR",
            project_dirs.config_dir().as_os_str(),
        );
        set_default_env(
            "JELLIUM_DESKTOP_CACHE_DIR",
            project_dirs.cache_dir().as_os_str(),
        );
    }
}

fn set_default_env(target: &str, value: &std::ffi::OsStr) {
    if std::env::var_os(target).is_none() {
        unsafe { std::env::set_var(target, value) };
    }
}

fn mirror_env(source: &str, target: &str) {
    if std::env::var_os(target).is_none()
        && let Some(value) = std::env::var_os(source)
    {
        unsafe { std::env::set_var(target, value) };
    }
}
