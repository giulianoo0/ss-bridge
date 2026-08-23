#![windows_subsystem = "windows"]

mod engine;
mod server;
mod ui;
mod update;

fn main() {
    std::thread::spawn(|| {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        rt.block_on(async {
            tokio::spawn(update::check());
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
