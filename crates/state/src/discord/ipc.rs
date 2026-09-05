use std::time::Duration;

use anyhow::{Result, bail, ensure};
use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::time::timeout;

#[cfg(windows)]
type Stream = tokio::net::windows::named_pipe::NamedPipeClient;
#[cfg(unix)]
type Stream = tokio::net::UnixStream;

const TIMEOUT: Duration = Duration::from_secs(3);
const MAX_FRAME: usize = 64 * 1024;

pub(super) struct Client {
    stream: Stream,
    nonce: u64,
}

impl Client {
    pub async fn connect(id: &str) -> Result<Self> {
        ensure!(
            id.parse::<u64>().is_ok_and(|id| id != 0),
            "invalid Discord application ID"
        );
        timeout(TIMEOUT, async {
            let mut stream = connect().await?;
            handshake(&mut stream, id).await?;
            Ok(Self { stream, nonce: 0 })
        })
        .await?
    }

    pub async fn set_activity(&mut self, activity: Value) -> Result<()> {
        self.nonce = self.nonce.wrapping_add(1);
        timeout(
            TIMEOUT,
            set_activity(&mut self.stream, activity, self.nonce),
        )
        .await?
    }
}

#[cfg(windows)]
async fn connect() -> Result<Stream> {
    use tokio::net::windows::named_pipe::ClientOptions;
    for index in 0..10 {
        if let Ok(stream) = ClientOptions::new().open(format!(r"\\?\pipe\discord-ipc-{index}")) {
            return Ok(stream);
        }
    }
    bail!("Discord IPC is unavailable")
}

#[cfg(unix)]
async fn connect() -> Result<Stream> {
    use std::path::PathBuf;
    let roots = ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .chain(std::iter::once(PathBuf::from("/tmp")));
    for root in roots {
        for subdir in ["", "app/com.discordapp.Discord", "snap.discord"] {
            for index in 0..10 {
                if let Ok(stream) =
                    Stream::connect(root.join(subdir).join(format!("discord-ipc-{index}"))).await
                {
                    return Ok(stream);
                }
            }
        }
    }
    bail!("Discord IPC is unavailable")
}

async fn handshake(stream: &mut (impl AsyncRead + AsyncWrite + Unpin), id: &str) -> Result<()> {
    write_frame(
        stream,
        0,
        &serde_json::to_vec(&json!({ "v": 1, "client_id": id }))?,
    )
    .await?;
    let response = response(stream).await?;
    ensure!(response["evt"] == "READY", "Discord rejected the handshake");
    Ok(())
}

async fn set_activity(
    stream: &mut (impl AsyncRead + AsyncWrite + Unpin),
    activity: Value,
    nonce: u64,
) -> Result<()> {
    let nonce = nonce.to_string();
    let payload = json!({
        "cmd": "SET_ACTIVITY",
        "args": { "pid": std::process::id(), "activity": activity },
        "nonce": nonce,
    });
    write_frame(stream, 1, &serde_json::to_vec(&payload)?).await?;
    loop {
        let reply = response(stream).await?;
        if reply["nonce"] == nonce {
            ensure!(
                reply["evt"] != "ERROR",
                "Discord rejected the activity (code {})",
                reply["data"]["code"]
            );
            ensure!(reply["cmd"] == "SET_ACTIVITY", "unexpected Discord reply");
            return Ok(());
        }
    }
}

async fn write_frame(
    stream: &mut (impl AsyncWrite + Unpin),
    opcode: u32,
    data: &[u8],
) -> Result<()> {
    ensure!(data.len() <= MAX_FRAME, "Discord frame is too large");
    stream.write_u32_le(opcode).await?;
    stream.write_u32_le(data.len() as u32).await?;
    stream.write_all(data).await?;
    stream.flush().await?;
    Ok(())
}

async fn read_frame(stream: &mut (impl AsyncRead + Unpin)) -> Result<(u32, Vec<u8>)> {
    let opcode = stream.read_u32_le().await?;
    let size = stream.read_u32_le().await? as usize;
    ensure!(size <= MAX_FRAME, "Discord frame is too large");
    let mut data = vec![0; size];
    stream.read_exact(&mut data).await?;
    Ok((opcode, data))
}

async fn response(stream: &mut (impl AsyncRead + AsyncWrite + Unpin)) -> Result<Value> {
    loop {
        let (opcode, data) = read_frame(stream).await?;
        match opcode {
            1 => return Ok(serde_json::from_slice(&data)?),
            2 => bail!("Discord closed the connection"),
            3 => write_frame(stream, 4, &data).await?,
            4 => {}
            _ => bail!("unexpected Discord opcode {opcode}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn handshake_activity_ping_and_clear() {
        let (mut client, mut server) = tokio::io::duplex(8192);
        let server = tokio::spawn(async move {
            let (opcode, bytes) = read_frame(&mut server).await.unwrap();
            assert_eq!(opcode, 0);
            assert_eq!(
                serde_json::from_slice::<Value>(&bytes).unwrap(),
                json!({"v": 1, "client_id": "123"})
            );
            write_frame(&mut server, 1, br#"{"evt":"READY"}"#)
                .await
                .unwrap();
            for nonce in [1, 2] {
                let (opcode, bytes) = read_frame(&mut server).await.unwrap();
                assert_eq!(opcode, 1);
                let request: Value = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(request["cmd"], "SET_ACTIVITY");
                assert_eq!(request["args"]["pid"], std::process::id());
                assert_eq!(request["args"]["activity"].is_null(), nonce == 2);
                write_frame(&mut server, 3, b"ping").await.unwrap();
                assert_eq!(
                    read_frame(&mut server).await.unwrap(),
                    (4, b"ping".to_vec())
                );
                let response =
                    json!({"cmd": "SET_ACTIVITY", "nonce": nonce.to_string(), "evt": null});
                write_frame(&mut server, 1, &serde_json::to_vec(&response).unwrap())
                    .await
                    .unwrap();
            }
        });
        handshake(&mut client, "123").await.unwrap();
        set_activity(&mut client, json!({"type": 2, "details": "Test track"}), 1)
            .await
            .unwrap();
        set_activity(&mut client, Value::Null, 2).await.unwrap();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn rejects_invalid_handshake_oversized_frames_and_disconnects() {
        let (mut client, mut server) = tokio::io::duplex(8192);
        write_frame(&mut server, 1, br#"{"evt":"ERROR"}"#)
            .await
            .unwrap();
        assert!(handshake(&mut client, "123").await.is_err());
        server.write_u32_le(1).await.unwrap();
        server.write_u32_le(MAX_FRAME as u32 + 1).await.unwrap();
        assert!(read_frame(&mut client).await.is_err());
        drop(server);
        assert!(response(&mut client).await.is_err());
    }

    #[tokio::test]
    async fn rejects_activity_errors_and_times_out_unresponsive_peers() {
        let (mut client, mut server) = tokio::io::duplex(8192);
        write_frame(
            &mut server,
            1,
            br#"{"cmd":"SET_ACTIVITY","nonce":"1","evt":"ERROR","data":{"code":4000}}"#,
        )
        .await
        .unwrap();
        assert!(set_activity(&mut client, Value::Null, 1).await.is_err());
        assert!(
            timeout(Duration::from_millis(20), response(&mut client))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    #[ignore = "requires the Discord desktop app; briefly publishes a test activity"]
    async fn live_discord_accepts_and_clears_activity() {
        let mut client = Client::connect(super::super::CLIENT_ID).await.unwrap();
        client
            .set_activity(
                json!({"type": 2, "details": "Sonora RPC test", "state": "Local verification"}),
            )
            .await
            .unwrap();
        client.set_activity(Value::Null).await.unwrap();
    }
}
