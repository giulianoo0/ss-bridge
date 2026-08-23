mod engine;
mod server;
mod ui;

fn main() {
    std::thread::spawn(|| {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        rt.block_on(async {
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
