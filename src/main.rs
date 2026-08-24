#![windows_subsystem = "windows"]

mod engine;
mod portmap;
mod server;
mod ui;
mod update;

fn main() {
    // librqbit speaks tracing; without a subscriber every word of it — the
    // initial checksum, fastresume, peer errors — lands nowhere. Opt-in via
    // the usual env var so the packaged app stays silent.
    if std::env::var_os("RUST_LOG").is_some() {
        use tracing_subscriber::EnvFilter;
        tracing_subscriber::fmt().with_env_filter(EnvFilter::from_default_env()).init();
    }
    std::thread::spawn(|| {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        rt.block_on(async {
            tokio::spawn(update::check());
            tokio::spawn(portmap::watch());
            match engine::Engine::new().await {
                Ok(engine) => {
                    if let Err(err) = server::serve(engine).await {
                        eprintln!("server: {err:#}");
                    }
                }
                Err(err) => eprintln!("engine: {err:#}"),
            }
        });
    });
    ui::run();
}
