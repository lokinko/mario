#[tokio::main]
async fn main() {
    if let Err(error) = mario_server::run_from_env().await {
        eprintln!("mario-server failed: {error}");
        std::process::exit(1);
    }
}
