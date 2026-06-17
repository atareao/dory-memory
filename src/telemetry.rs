use notify::{RecursiveMode, Result as NotifyResult, Watcher};
use std::path::Path;

pub fn start_workspace_daemon(workspace_root: &str) -> NotifyResult<()> {
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        match res {
            Ok(event) => {
                if event.kind.is_modify() {
                    tracing::info!(
                        "Dory Telemetry Daemon: Environment shift caught! Synchronizing state context loops..."
                    );
                }
            }
            Err(e) => eprintln!("Dory Telemetry Watcher Error: {e}"),
        }
    })?;

    watcher.watch(Path::new(workspace_root), RecursiveMode::Recursive)?;
    tracing::info!("Telemetry daemon started on: {workspace_root}");
    Ok(())
}
