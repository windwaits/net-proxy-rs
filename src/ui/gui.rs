#[cfg(feature = "gui")]
pub async fn launch() {
    println!("GUI mode not implemented in this headless build. Enable the 'gui' feature and supply a backend.");
}
