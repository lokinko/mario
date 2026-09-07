use rand::{rngs::OsRng, RngCore};
use serde::Serialize;
#[cfg(target_os = "android")]
use tauri::Manager;
#[cfg(desktop)]
use tauri_plugin_shell::ShellExt;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalServiceConfig {
    base_url: String,
    auth_token: String,
}

#[tauri::command]
fn local_service_config(config: tauri::State<'_, LocalServiceConfig>) -> LocalServiceConfig {
    config.inner().clone()
}

fn generate_auth_token() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn available_port() -> std::io::Result<u16> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
    listener.local_addr().map(|address| address.port())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let port = available_port().expect("find an available local service port");
    let service_config = LocalServiceConfig {
        base_url: format!("http://127.0.0.1:{port}/api"),
        auth_token: generate_auth_token(),
    };
    let sidecar_config = service_config.clone();
    tauri::Builder::default()
        .manage(service_config)
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![local_service_config])
        .setup(move |app| {
            #[cfg(desktop)]
            {
                let parent_pid = std::process::id().to_string();
                let port = port.to_string();
                let command = app
                    .shell()
                    .sidecar("mario-server")?
                    .args(["--parent-pid", parent_pid.as_str(), "--port", port.as_str()])
                    .env("MARIO_AUTH_TOKEN", &sidecar_config.auth_token);
                let (_events, _child) = command.spawn()?;
            }
            #[cfg(target_os = "android")]
            {
                android_keyring::set_android_keyring_credential_builder()
                    .map_err(std::io::Error::other)?;
                let data_dir = app.path().app_local_data_dir()?;
                std::fs::create_dir_all(&data_dir)?;
                let auth_token = sidecar_config.auth_token.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(error) = mario_server::serve_embedded(
                        data_dir,
                        port,
                        auth_token,
                        std::future::pending(),
                    )
                    .await
                    {
                        eprintln!("embedded mario-server failed: {error}");
                    }
                });
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running mario desktop client");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_tokens_are_random_and_256_bits() {
        let first = generate_auth_token();
        let second = generate_auth_token();
        assert_eq!(first.len(), 64);
        assert!(first.chars().all(|character| character.is_ascii_hexdigit()));
        assert_ne!(first, second);
    }

    #[test]
    fn selected_port_is_non_zero() {
        assert_ne!(available_port().unwrap(), 0);
    }
}
