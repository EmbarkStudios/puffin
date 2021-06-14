/// Connects to the given http address receives puffin profile data
/// that is then fed to [`puffin::GlobalProfiler`].
///
/// You can then view the data with
/// [`puffin_egui`](https://crates.io/crates/puffin_egui) or [`puffin-imgui`](https://crates.io/crates/puffin-imgui).
///
/// ``` no_run
/// puffin_http::start_client("localhost:8585");
/// ```
pub fn start_client(addr: &str) -> anyhow::Result<()> {
    let mut stream = std::net::TcpStream::connect(addr)?;
    log::info!("Receiving profile data from {}", addr);

    std::thread::spawn(move || loop {
        match consume_message(&mut stream) {
            Ok(frame_data) => {
                puffin::GlobalProfiler::lock().add_frame(std::sync::Arc::new(frame_data));
            }
            Err(err) => {
                log::warn!("Connection to puffin server closed: {}", err);
                break;
            }
        }
    });

    Ok(())
}

/// Read a `puffin_http` message from a stream.
pub fn consume_message(stream: &mut dyn std::io::Read) -> anyhow::Result<puffin::FrameData> {
    let mut server_version = [0_u8; 2];
    stream.read_exact(&mut server_version)?;
    let server_version = u16::from_le_bytes(server_version);
    if server_version != crate::PROTOCOL_VERSION {
        anyhow::bail!(
            "puffin server is using protocol version {} and client is on {}",
            server_version,
            crate::PROTOCOL_VERSION
        );
    }

    let mut message_len = [0_u8; 4];
    stream.read_exact(&mut message_len)?;
    let message_len = u32::from_le_bytes(message_len);

    let mut bytes = vec![0_u8; message_len as usize];
    stream.read_exact(&mut bytes)?;

    use anyhow::Context as _;
    use bincode::Options as _;

    bincode::options()
        .deserialize(&bytes)
        .context("Failed to decode bincode")
}
