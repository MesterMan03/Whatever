use super::{LoadedGame, SimulationHandle};
use std::sync::atomic::Ordering;
pub fn run_server(game: LoadedGame) -> anyhow::Result<()> {
    let mut simulation = SimulationHandle::spawn(game)?;
    simulation.ready();
    let commands = simulation.commands.clone();
    std::thread::Builder::new()
        .name("whatever-server-stdin".into())
        .spawn(move || {
            use std::io::BufRead;
            for line in std::io::stdin().lock().lines() {
                let Ok(line) = line else { break };
                if !line.trim().is_empty() {
                    let _ = commands.send(super::bridge::RuntimeCommand::ConsoleInput(line));
                }
            }
        })?;
    tracing::info!("headless server running; type help for commands, Ctrl+C or quit to stop");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let signal_result=runtime.block_on(async{
        tokio::select! {
            result=shutdown_signal()=>result,
            ()=async{while !simulation.is_finished() && !simulation.shared.stop.load(Ordering::Relaxed){tokio::time::sleep(std::time::Duration::from_millis(50)).await;}}=>Ok(()),
        }
    });
    let shutdown_result = simulation.shutdown();
    signal_result?;
    shutdown_result
}
async fn shutdown_signal() -> std::io::Result<()> {
    #[cfg(windows)]
    {
        let mut breaks = tokio::signal::windows::ctrl_break()?;
        tokio::select! {result=tokio::signal::ctrl_c()=>result,_=breaks.recv()=>Ok(())}
    }
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! {result=tokio::signal::ctrl_c()=>result,_=terminate.recv()=>Ok(())}
    }
    #[cfg(not(any(windows, unix)))]
    {
        tokio::signal::ctrl_c().await
    }
}
