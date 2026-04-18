#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use yaatal_voice::{contracts::VoiceServerMessage, mock::MockVoiceBackend, server};

async fn next_server_message(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> Result<VoiceServerMessage, String> {
    let frame = socket.next().await.expect("frame").expect("message");
    let Message::Text(payload) = frame else {
        return Err("expected text frame".to_string());
    };

    serde_json::from_str(payload.as_str()).map_err(|err| err.to_string())
}

#[tokio::test]
async fn websocket_session_streams_mock_messages() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let addr = listener.local_addr().expect("local addr");
    let backend = Arc::new(MockVoiceBackend::default());

    let server_task = tokio::spawn(server::run(listener, backend));

    let url = format!("ws://{addr}/session");
    let (mut socket, _) = connect_async(url).await.expect("connect");

    socket
        .send(Message::Text(
            serde_json::json!({
                "type": "session_config",
                "session_id": "session-42",
                "persona": "market-guide",
                "lang": "wo",
                "market": "SN-DKR"
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("send config");

    let ready = next_server_message(&mut socket)
        .await
        .expect("session ready");
    assert_eq!(
        ready,
        VoiceServerMessage::SessionReady {
            backend: "mock-personaplex".to_string(),
            session_id: "session-42".to_string(),
        }
    );

    socket
        .send(Message::Text(
            serde_json::json!({
                "type": "audio_chunk",
                "audio_base64": "ZmFrZQ==",
                "transcript_hint": "find white fabric near Sandaga"
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("send audio");

    let subtitle = next_server_message(&mut socket).await.expect("subtitle");
    assert_eq!(
        subtitle,
        VoiceServerMessage::Subtitle {
            text: "find white fabric near Sandaga".to_string(),
            final_chunk: true,
        }
    );

    let audio = next_server_message(&mut socket).await.expect("audio");
    assert_eq!(
        audio,
        VoiceServerMessage::AudioChunk {
            audio_base64: "bW9jay1hdWRpbw==".to_string(),
        }
    );

    let turn_end = next_server_message(&mut socket).await.expect("turn end");
    assert_eq!(turn_end, VoiceServerMessage::TurnEnd { reason: None });

    server_task.abort();
}
