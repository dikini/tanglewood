//! Actual MCP stdio service; stdout is reserved for protocol frames.
#[cfg(target_os = "linux")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use rmcp::ServiceExt;
    let mut arguments = std::env::args_os().skip(1);
    let flag = arguments.next();
    let path = arguments.next();
    if flag.as_deref() != Some(std::ffi::OsStr::new("--socket"))
        || path.is_none()
        || arguments.next().is_some()
    {
        return Err("usage: tanglewood-mcp --socket /private/directory/owner.sock".into());
    }
    let bridge = tanglewood::mcp::McpBridge::new(path.unwrap().into());
    let (read, write) = rmcp::transport::stdio_pipes();
    let transport =
        rmcp::transport::async_rw::AsyncRwTransport::<rmcp::RoleServer, _, _>::new(read, write)
            .with_max_line_length(2 * 1024 * 1024);
    bridge.serve(transport).await?.waiting().await?;
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("Tanglewood MCP is available only on Linux");
    std::process::exit(1);
}
