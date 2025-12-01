use std::process;

#[tokio::main]
async fn main() {
    if let Err(err) = net_proxy_rs::ui::cli::run().await {
        eprintln!("error: {err}");
        process::exit(1);
    }
}
