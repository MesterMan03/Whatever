#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod app;
mod document;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "whatever_editor=info,whatever_engine=info,wgpu=error".into()),
        )
        .init();
    app::run()
}
