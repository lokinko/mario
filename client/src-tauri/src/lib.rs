use std::{
    net::{SocketAddr, TcpStream},
    time::Duration,
};
use tauri_plugin_shell::ShellExt;

fn server_is_running() -> bool {
    let address: SocketAddr = "127.0.0.1:4217"
        .parse()
        .expect("valid local server address");
    TcpStream::connect_timeout(&address, Duration::from_millis(150)).is_ok()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            if !server_is_running() {
                let command = app.shell().sidecar("compass-server")?;
                let (_events, _child) = command.spawn()?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Compass Invest desktop client");
}
