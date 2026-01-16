//! WebSocket client for real-time task status updates - V2 Architecture
//!
//! 连接到 /ws/session/{session_id} 接收实时推送:
//! - TaskStateChanged: 任务状态变更
//! - SessionClosed: Session 被服务端关闭

use bevy::prelude::*;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use crate::api::{WsEvent, WS_BASE_URL};
use crate::state::{WsConnectionState, WsRequest, WsResponse};

/// WebSocket 内部命令
enum WsCommand {
    Connect { session_id: String },
    Disconnect,
    Shutdown,
}

/// WebSocket 客户端资源
#[derive(Resource)]
pub struct WsClient {
    command_tx: Sender<WsCommand>,
    response_rx: Mutex<Receiver<WsResponse>>,
}

impl WsClient {
    pub fn new() -> Self {
        let (command_tx, command_rx) = mpsc::channel::<WsCommand>();
        let (response_tx, response_rx) = mpsc::channel::<WsResponse>();

        // 启动 WebSocket 线程
        thread::spawn(move || {
            ws_thread(command_rx, response_tx);
        });

        Self {
            command_tx,
            response_rx: Mutex::new(response_rx),
        }
    }

    pub fn connect(&self, session_id: &str) {
        let _ = self.command_tx.send(WsCommand::Connect {
            session_id: session_id.to_string(),
        });
    }

    pub fn disconnect(&self) {
        let _ = self.command_tx.send(WsCommand::Disconnect);
    }

    pub fn poll_responses(&self) -> Vec<WsResponse> {
        let mut responses = Vec::new();
        if let Ok(rx) = self.response_rx.lock() {
            while let Ok(resp) = rx.try_recv() {
                responses.push(resp);
            }
        }
        responses
    }
}

impl Drop for WsClient {
    fn drop(&mut self) {
        let _ = self.command_tx.send(WsCommand::Shutdown);
    }
}

/// WebSocket 线程主循环
fn ws_thread(command_rx: Receiver<WsCommand>, response_tx: Sender<WsResponse>) {
    use tungstenite::{connect, Message};
    use url::Url;

    let mut current_socket: Option<tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>> = None;

    loop {
        // 检查命令
        match command_rx.try_recv() {
            Ok(WsCommand::Connect { session_id }) => {
                // 关闭现有连接
                if let Some(mut socket) = current_socket.take() {
                    let _ = socket.close(None);
                }

                // 建立新连接
                let url = format!("{}/session/{}", WS_BASE_URL, session_id);
                match Url::parse(&url) {
                    Ok(url) => {
                        match connect(url.as_str()) {
                            Ok((socket, _)) => {
                                // 设置非阻塞模式
                                if let tungstenite::stream::MaybeTlsStream::Plain(ref stream) = socket.get_ref() {
                                    let _ = stream.set_nonblocking(true);
                                }
                                current_socket = Some(socket);
                                let _ = response_tx.send(WsResponse::Connected);
                            }
                            Err(e) => {
                                let _ = response_tx.send(WsResponse::Error(format!("Failed to connect: {}", e)));
                            }
                        }
                    }
                    Err(e) => {
                        let _ = response_tx.send(WsResponse::Error(format!("Invalid URL: {}", e)));
                    }
                }
            }
            Ok(WsCommand::Disconnect) => {
                if let Some(mut socket) = current_socket.take() {
                    let _ = socket.close(None);
                }
                let _ = response_tx.send(WsResponse::Disconnected);
            }
            Ok(WsCommand::Shutdown) => {
                if let Some(mut socket) = current_socket.take() {
                    let _ = socket.close(None);
                }
                break;
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                break;
            }
        }

        // 读取 WebSocket 消息
        if let Some(ref mut socket) = current_socket {
            match socket.read() {
                Ok(Message::Text(text)) => {
                    // 解析 WebSocket 事件
                    match serde_json::from_str::<WsEvent>(&text) {
                        Ok(event) => {
                            match &event {
                                WsEvent::TaskStateChanged { .. } => {
                                    let _ = response_tx.send(WsResponse::TaskStateChanged(event));
                                }
                                WsEvent::SessionClosed { session_id, reason } => {
                                    let _ = response_tx.send(WsResponse::SessionClosedByServer {
                                        session_id: session_id.clone(),
                                        reason: reason.clone(),
                                    });
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Failed to parse WebSocket event: {} - {}", e, text);
                        }
                    }
                }
                Ok(Message::Ping(data)) => {
                    // 响应 Ping
                    let _ = socket.send(Message::Pong(data));
                }
                Ok(Message::Close(_)) => {
                    current_socket = None;
                    let _ = response_tx.send(WsResponse::Disconnected);
                }
                Err(tungstenite::Error::Io(ref e)) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // 非阻塞模式下没有数据，正常情况
                }
                Err(e) => {
                    tracing::warn!("WebSocket error: {}", e);
                    current_socket = None;
                    let _ = response_tx.send(WsResponse::Disconnected);
                }
                _ => {}
            }
        }

        // 短暂休眠
        thread::sleep(Duration::from_millis(50));
    }
}

/// 处理 WebSocket 请求
pub fn handle_ws_requests(
    mut events: EventReader<WsRequest>,
    ws_client: Option<Res<WsClient>>,
    mut app_state: ResMut<crate::state::AppState>,
) {
    let Some(client) = ws_client else { return };

    for event in events.read() {
        match event {
            WsRequest::Connect(session_id) => {
                app_state.ws_state = WsConnectionState::Connecting;
                client.connect(session_id);
            }
            WsRequest::Disconnect => {
                client.disconnect();
            }
        }
    }
}

/// 轮询 WebSocket 响应
pub fn poll_ws_responses(
    ws_client: Option<Res<WsClient>>,
    mut response_events: EventWriter<WsResponse>,
) {
    let Some(client) = ws_client else { return };

    for response in client.poll_responses() {
        response_events.send(response);
    }
}

/// 设置 WebSocket 客户端
pub fn setup_ws_client(mut commands: Commands) {
    let client = WsClient::new();
    commands.insert_resource(client);
}
