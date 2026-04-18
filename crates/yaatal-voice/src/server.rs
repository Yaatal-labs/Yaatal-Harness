use std::sync::Arc;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::Response,
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;

use crate::{
    backend::{VoiceBackend, VoiceBackendError},
    contracts::{VoiceClientMessage, VoiceServerMessage},
};

pub fn router<B>(backend: Arc<B>) -> Router
where
    B: VoiceBackend + 'static,
{
    Router::new()
        .route("/session", get(open_session::<B>))
        .with_state(backend)
}

pub async fn run<B>(listener: TcpListener, backend: Arc<B>) -> Result<(), std::io::Error>
where
    B: VoiceBackend + 'static,
{
    axum::serve(listener, router(backend)).await
}

async fn open_session<B>(ws: WebSocketUpgrade, State(backend): State<Arc<B>>) -> Response
where
    B: VoiceBackend + 'static,
{
    ws.on_upgrade(move |socket| async move {
        if let Err(error) = handle_socket(socket, backend).await {
            tracing::warn!(error = %error, "voice mock session closed with error");
        }
    })
}

async fn handle_socket<B>(socket: WebSocket, backend: Arc<B>) -> Result<(), VoiceBackendError>
where
    B: VoiceBackend + 'static,
{
    let (mut sink, mut stream) = socket.split();
    let config = read_initial_config(&mut sink, &mut stream).await?;
    let mut session = backend.open_session(config).await?;

    loop {
        tokio::select! {
            maybe_ws_message = stream.next() => {
                match maybe_ws_message {
                    Some(Ok(message)) => {
                        if let Some(client_message) = parse_follow_up_message(message)? {
                            session.send(client_message).await?;
                        } else {
                            break;
                        }
                    }
                    Some(Err(err)) => return Err(VoiceBackendError::Transport(err.to_string())),
                    None => {
                        let _ = session.send(VoiceClientMessage::Close).await;
                        break;
                    }
                }
            }
            maybe_backend_message = session.recv() => {
                match maybe_backend_message {
                    Some(server_message) => send_server_message(&mut sink, &server_message).await?,
                    None => break,
                }
            }
        }
    }

    Ok(())
}

async fn read_initial_config(
    sink: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    stream: &mut futures_util::stream::SplitStream<WebSocket>,
) -> Result<crate::contracts::SessionConfig, VoiceBackendError> {
    let Some(message_result) = stream.next().await else {
        return Err(VoiceBackendError::Protocol(
            "client disconnected before session_config".to_string(),
        ));
    };

    let message = message_result.map_err(|err| VoiceBackendError::Transport(err.to_string()))?;
    let client_message = parse_client_message(message)?;
    let Some(config) = client_message.into_session_config() else {
        send_server_message(
            sink,
            &VoiceServerMessage::Error {
                message: "first message must be session_config".to_string(),
            },
        )
        .await?;
        return Err(VoiceBackendError::Protocol(
            "first message must be session_config".to_string(),
        ));
    };

    Ok(config)
}

fn parse_follow_up_message(
    message: Message,
) -> Result<Option<VoiceClientMessage>, VoiceBackendError> {
    match parse_client_message(message)? {
        VoiceClientMessage::SessionConfig { .. } => Err(VoiceBackendError::Protocol(
            "session_config can only be sent as the first message".to_string(),
        )),
        VoiceClientMessage::Close => Ok(None),
        message => Ok(Some(message)),
    }
}

fn parse_client_message(message: Message) -> Result<VoiceClientMessage, VoiceBackendError> {
    match message {
        Message::Text(text) => serde_json::from_str(text.as_str())
            .map_err(|err| VoiceBackendError::Protocol(err.to_string())),
        Message::Close(_) => Ok(VoiceClientMessage::Close),
        Message::Binary(_) => Err(VoiceBackendError::Protocol(
            "binary websocket frames are not supported by the mock service".to_string(),
        )),
        Message::Ping(_) | Message::Pong(_) => Ok(VoiceClientMessage::ClientPing),
    }
}

async fn send_server_message(
    sink: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    message: &VoiceServerMessage,
) -> Result<(), VoiceBackendError> {
    let payload = serde_json::to_string(message)
        .map_err(|err| VoiceBackendError::Protocol(err.to_string()))?;
    sink.send(Message::Text(payload.into()))
        .await
        .map_err(|err| VoiceBackendError::Transport(err.to_string()))
}
