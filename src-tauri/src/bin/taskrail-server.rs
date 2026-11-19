//! 文件职责：独立无头入口，使用与桌面完全相同的服务内核。
#[cfg(not(target_os = "android"))]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use taskrail_lib::server::{
        config::{parse_config, shutdown_signal, USAGE},
        start_with_public_url,
    };
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        println!("{USAGE}");
        return Ok(());
    }
    let config = parse_config(args, |name| std::env::var_os(name))?;
    let public_url = std::env::var("TASKRAIL_MCP_PUBLIC_URL")
        .ok()
        .map(|raw| {
            let url = url::Url::parse(&raw)?;
            if url.scheme() != "https"
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
                || url.path() != "/"
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err("TASKRAIL_MCP_PUBLIC_URL 必须是 HTTPS 服务根地址".into());
            }
            Ok::<_, Box<dyn std::error::Error>>(url)
        })
        .transpose()?;
    let server = start_with_public_url(config, public_url).await?;
    println!("Taskrail server listening on {}", server.info.base_url);
    server.run_until(shutdown_signal()).await?;
    Ok(())
}

#[cfg(target_os = "android")]
fn main() {
    eprintln!("taskrail-server 仅支持桌面平台");
}
