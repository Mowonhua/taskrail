//! 文件职责：Taskrail Agent 的终端与 STDIO 入口。
#[cfg(not(target_os = "android"))]
#[tokio::main]
async fn main() -> std::process::ExitCode {
    match taskrail_lib::agent_cli::run().await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(target_os = "android")]
fn main() {
    eprintln!("taskrail-agent 仅支持桌面平台");
}
