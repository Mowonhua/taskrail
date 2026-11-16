//! 文件职责：独立无头入口，使用与桌面完全相同的服务内核。
#[cfg(not(target_os = "android"))]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use taskrail_lib::server::{
        config::{parse_config, shutdown_signal, USAGE},
        start,
    };
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        println!("{USAGE}");
        return Ok(());
    }
    let config = parse_config(args, |name| std::env::var_os(name))?;
    let server = start(config).await?;
    println!("Taskrail server listening on {}", server.info.base_url);
    server.run_until(shutdown_signal()).await?;
    Ok(())
}

#[cfg(target_os = "android")]
fn main() {
    eprintln!("taskrail-server 仅支持桌面平台");
}
