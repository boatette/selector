mod app;
mod config;
mod geometry;
mod monitor;
mod render;
mod selection;

use anyhow::Result;
use config::Config;

fn main() -> Result<()> {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(concat!(env!("CARGO_PKG_NAME"), "=info")),
    )
    .init();

    app::run(Config::load()?)
}
