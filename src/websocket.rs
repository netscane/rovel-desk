//! WebSocket client for real-time task status updates - V2 Architecture
//!
//! 支持两种连接类型:
//! - /ws/session/{session_id}: 接收任务状态推送 (TaskStateChanged, SessionClosed)
//! - /ws/events: 全局事件通道 (NovelReady, NovelFailed)

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

/// 全局事件通道命令
enum GlobalWsCommand {
    Connect,
    Disconnect,
    Shutdown,
}

/// WebSocket 客户端资源 (Session channel)
#[derive(Resource)]
pub struct WsClient {
    command_tx: Sender<WsCommand>,
    response_rx: Mutex<Receiver<WsResponse>>,
}

/// 全局事件 WebSocket 客户端资源
#[derive(Resource)]
pub struct GlobalWsClient {
    command_tx: Sender<GlobalWsCommand>,
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

impl GlobalWsClient {
    pub fn new() -> Self {
        let (command_tx, command_rx) = mpsc::channel::<GlobalWsCommand>();
        let (response_tx, response_rx) = mpsc::channel::<WsResponse>();

        // 启动全局事件 WebSocket 线程
        thread::spawn(move || {
            global_ws_thread(command_rx, response_tx);
        });

        Self {
            command_tx,
            response_rx: Mutex::new(response_rx),
        }
    }

    pub fn connect(&self) {
        let _ = self.command_tx.send(GlobalWsCommand::Connect);
    }

    #[allow(dead_code)]
    pub fn disconnect(&self) {
        let _ = self.command_tx.send(GlobalWsCommand::Disconnect);
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

impl Drop for GlobalWsClient {
    fn drop(&mut self) {
        let _ = self.command_tx.send(GlobalWsCommand::Shutdown);
    }
}

/// WebSocket 线程主循环 (Session channel)
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
                                // Session channel 不应该收到这些事件
                                WsEvent::NovelReady { .. } | WsEvent::NovelFailed { .. } 
                                | WsEvent::NovelDeleting { .. } | WsEvent::NovelDeleted { .. }
                                | WsEvent::NovelDeleteFailed { .. } | WsEvent::VoiceDeleted { .. } => {
                                    tracing::warn!("Received global event on session channel: {:?}", event);
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

/// 全局事件 WebSocket 线程主循环
fn global_ws_thread(command_rx: Receiver<GlobalWsCommand>, response_tx: Sender<WsResponse>) {
    use tungstenite::{connect, Message};
    use url::Url;

    let mut current_socket: Option<tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>> = None;
    let mut should_reconnect = false;
    let mut reconnect_delay = Duration::from_secs(1);
    let mut consecutive_failures = 0u32;

    loop {
        // 检查命令
        match command_rx.try_recv() {
            Ok(GlobalWsCommand::Connect) => {
                should_reconnect = true;
                reconnect_delay = Duration::from_secs(1);
                consecutive_failures = 0;
            }
            Ok(GlobalWsCommand::Disconnect) => {
                should_reconnect = false;
                if let Some(mut socket) = current_socket.take() {
                    let _ = socket.close(None);
                }
                let _ = response_tx.send(WsResponse::GlobalDisconnected);
            }
            Ok(GlobalWsCommand::Shutdown) => {
                if let Some(mut socket) = current_socket.take() {
                    let _ = socket.close(None);
                }
                break;
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                // 主线程已断开，退出
                if let Some(mut socket) = current_socket.take() {
                    let _ = socket.close(None);
                }
                break;
            }
        }

        // 自动连接/重连
        if should_reconnect && current_socket.is_none() {
            // 如果连续失败太多次，等待更长时间
            if consecutive_failures > 10 {
                thread::sleep(Duration::from_secs(5));
                consecutive_failures = 5; // 重置一部分，允许继续尝试
            }
            
            let url = format!("{}/events", WS_BASE_URL);
            match Url::parse(&url) {
                Ok(url) => {
                    match connect(url.as_str()) {
                        Ok((socket, _)) => {
                            // 设置非阻塞模式
                            if let tungstenite::stream::MaybeTlsStream::Plain(ref stream) = socket.get_ref() {
                                let _ = stream.set_nonblocking(true);
                            }
                            current_socket = Some(socket);
                            reconnect_delay = Duration::from_secs(1);
                            consecutive_failures = 0;
                            let _ = response_tx.send(WsResponse::GlobalConnected);
                            tracing::info!("Global WebSocket connected to {}", WS_BASE_URL);
                        }
                        Err(e) => {
                            consecutive_failures += 1;
                            let error_str = e.to_string();
                            
                            // 对于 Windows Winsock 错误，短暂等待后重试
                            if error_str.contains("10093") || error_str.contains("WSAStartup") {
                                tracing::warn!("Global WebSocket Winsock error, will retry in 2s");
                                thread::sleep(Duration::from_secs(2));
                            } else {
                                tracing::warn!("Failed to connect global WebSocket: {}, retrying in {:?}", e, reconnect_delay);
                                thread::sleep(reconnect_delay);
                                // 指数退避，最大 30 秒
                                reconnect_delay = (reconnect_delay * 2).min(Duration::from_secs(30));
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Invalid global WebSocket URL: {}", e);
                    should_reconnect = false;
                }
            }
        }

        // 读取 WebSocket 消息
        if let Some(ref mut socket) = current_socket {
            match socket.read() {
                Ok(Message::Text(text)) => {
                    tracing::info!("Global WebSocket received: {}", text);
                    // 解析 WebSocket 事件
                    match serde_json::from_str::<WsEvent>(&text) {
                        Ok(event) => {
                            tracing::info!("Global WebSocket parsed event: {:?}", event);
                            match &event {
                                WsEvent::NovelReady { novel_id, title, total_segments } => {
                                    let _ = response_tx.send(WsResponse::NovelReady {
                                        novel_id: *novel_id,
                                        title: title.clone(),
                                        total_segments: *total_segments,
                                    });
                                }
                                WsEvent::NovelFailed { novel_id, error } => {
                                    let _ = response_tx.send(WsResponse::NovelFailed {
                                        novel_id: *novel_id,
                                        error: error.clone(),
                                    });
                                }
                                WsEvent::NovelDeleting { novel_id } => {
                                    let _ = response_tx.send(WsResponse::NovelDeleting {
                                        novel_id: *novel_id,
                                    });
                                }
                                WsEvent::NovelDeleted { novel_id } => {
                                    let _ = response_tx.send(WsResponse::NovelDeleted {
                                        novel_id: *novel_id,
                                    });
                                }
                                WsEvent::NovelDeleteFailed { novel_id, error } => {
                                    let _ = response_tx.send(WsResponse::NovelDeleteFailed {
                                        novel_id: *novel_id,
                                        error: error.clone(),
                                    });
                                }
                                WsEvent::VoiceDeleted { voice_id } => {
                                    let _ = response_tx.send(WsResponse::VoiceDeleted {
                                        voice_id: *voice_id,
                                    });
                                }
                                // Global channel 不应该收到这些事件
                                WsEvent::TaskStateChanged { .. } | WsEvent::SessionClosed { .. } => {
                                    tracing::warn!("Received session event on global channel: {:?}", event);
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Failed to parse global WebSocket event: {} - {}", e, text);
                        }
                    }
                }
                Ok(Message::Ping(data)) => {
                    let _ = socket.send(Message::Pong(data));
                }
                Ok(Message::Close(_)) => {
                    current_socket = None;
                    let _ = response_tx.send(WsResponse::GlobalDisconnected);
                    tracing::info!("Global WebSocket disconnected, will reconnect");
                }
                Err(tungstenite::Error::Io(ref e)) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // 非阻塞模式下没有数据，正常情况
                }
                Err(e) => {
                    // 检查是否是致命错误
                    let error_str = e.to_string();
                    if error_str.contains("10093") || error_str.contains("WSAStartup") {
                        tracing::warn!("Global WebSocket fatal error: {}, stopping", e);
                        should_reconnect = false;
                        current_socket = None;
                    } else {
                        tracing::warn!("Global WebSocket error: {}", e);
                        current_socket = None;
                        // 断开后会自动重连
                    }
                }
                _ => {}
            }
        }

        // 短暂休眠
        thread::sleep(Duration::from_millis(50));
    }
}

/// 处理 WebSocket 请求 (Session channel)
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

/// 轮询 WebSocket 响应 (Session channel)
pub fn poll_ws_responses(
    ws_client: Option<Res<WsClient>>,
    mut response_events: EventWriter<WsResponse>,
) {
    let Some(client) = ws_client else { return };

    for response in client.poll_responses() {
        response_events.send(response);
    }
}

/// 轮询全局 WebSocket 响应
pub fn poll_global_ws_responses(
    global_ws_client: Option<Res<GlobalWsClient>>,
    mut response_events: EventWriter<WsResponse>,
) {
    let Some(client) = global_ws_client else { return };

    for response in client.poll_responses() {
        response_events.send(response);
    }
}

/// 设置 WebSocket 客户端 (Session + Global)
pub fn setup_ws_client(mut commands: Commands) {
    // Session channel client
    let client = WsClient::new();
    commands.insert_resource(client);
    
    // Global events channel client - 启动时自动连接
    let global_client = GlobalWsClient::new();
    global_client.connect(); // 自动连接全局事件通道
    commands.insert_resource(global_client);
}
