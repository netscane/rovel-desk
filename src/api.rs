//! HTTP API Client for Rovel backend - V2 Architecture
//!
//! V2 架构主要变化:
//! - Session 按需创建（play 时自动创建）
//! - 客户端驱动推理任务提交
//! - WebSocket 实时推送任务状态
//! - 音频通过 novel_id + segment_index + voice_id 获取

use anyhow::Result;
use bevy::prelude::Resource;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use uuid::Uuid;

const BASE_URL: &str = "http://192.168.2.31:5060/api";
pub const WS_BASE_URL: &str = "ws://192.168.2.31:5060/ws";

// ============================================================================
// 统一响应格式
// ============================================================================

#[derive(Debug, Clone, Deserialize)]
pub struct ApiResponse<T> {
    pub errno: i32,
    pub error: String,
    pub data: Option<T>,
}

impl<T> ApiResponse<T> {
    pub fn into_result(self) -> Result<T> {
        if self.errno == 0 {
            self.data.ok_or_else(|| anyhow::anyhow!("No data in response"))
        } else {
            Err(anyhow::anyhow!("API error ({}): {}", self.errno, self.error))
        }
    }
}

// 用于无数据返回的情况
#[derive(Debug, Clone, Deserialize)]
pub struct EmptyData {}

// ============================================================================
// Novel DTOs
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NovelResponse {
    pub id: Uuid,
    pub title: String,
    pub total_segments: usize,
    /// 状态: "processing" | "ready" | "error"
    #[serde(default = "default_status")]
    pub status: String,
    pub created_at: String,
    /// 是否为临时小说（上传中但未从服务器返回）
    #[serde(default)]
    pub is_temporary: bool,
}

fn default_status() -> String {
    "ready".to_string()
}

impl NovelResponse {
    /// 创建临时小说对象（用于上传时的即时反馈）
    pub fn create_temporary(title: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            title,
            total_segments: 0,
            status: "uploading".to_string(),
            created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            is_temporary: true,
        }
    }
    
    /// 检查是否为临时对象
    pub fn is_temporary(&self) -> bool {
        self.is_temporary
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentResponse {
    pub index: usize,
    pub content: String,
    pub char_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentsResponse {
    pub novel_id: Uuid,
    pub total: usize,
    pub segments: Vec<SegmentResponse>,
}

// ============================================================================
// Voice DTOs
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceResponse {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub created_at: String,
}

// ============================================================================
// Session DTOs (V2)
// ============================================================================

/// V2 Play 响应 - Session 按需创建
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayResponse {
    pub session_id: String,
    pub novel_id: Uuid,
    pub voice_id: Uuid,
    pub current_index: u32,
}

/// V2 Seek 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeekResponse {
    pub session_id: String,
    pub current_index: u32,
    pub cancelled_tasks: usize,
}

/// V2 Change Voice 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeVoiceResponse {
    pub session_id: String,
    pub voice_id: Uuid,
    pub cancelled_tasks: usize,
}

/// V2 Close Session 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloseSessionResponse {
    pub session_id: String,
}

// ============================================================================
// Inference Task DTOs (V2)
// ============================================================================

/// 推理任务信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInfo {
    pub task_id: String,
    pub segment_index: u32,
    pub state: String, // "pending" | "inferring" | "ready" | "failed" | "cancelled"
}

/// 提交推理任务响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitInferResponse {
    pub tasks: Vec<TaskInfo>,
}

/// 查询任务状态响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatusInfo {
    pub task_id: String,
    pub segment_index: u32,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryTaskStatusResponse {
    pub tasks: Vec<TaskStatusInfo>,
}

// ============================================================================
// WebSocket Event DTOs (V2)
// ============================================================================

/// WebSocket 事件类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", content = "data")]
pub enum WsEvent {
    /// 任务状态变更
    TaskStateChanged {
        session_id: String,
        task_id: String,
        segment_index: u32,
        state: String, // "inferring" | "ready" | "failed"
        #[serde(skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// Session 被服务端关闭
    SessionClosed {
        session_id: String,
        reason: String,
    },
}

// ============================================================================
// Request DTOs
// ============================================================================

#[derive(Debug, Clone, Serialize)]
struct IdRequest {
    id: Uuid,
}

#[derive(Debug, Clone, Serialize)]
struct GetNovelSegmentsRequest {
    novel_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    start: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<usize>,
}

/// V2 Play Request
#[derive(Debug, Clone, Serialize)]
struct PlayRequest {
    novel_id: Uuid,
    voice_id: Uuid,
    #[serde(default)]
    start_index: u32,
}

/// V2 Seek Request
#[derive(Debug, Clone, Serialize)]
struct SeekRequest {
    session_id: String,
    segment_index: u32,
}

/// V2 Change Voice Request
#[derive(Debug, Clone, Serialize)]
struct ChangeVoiceRequest {
    session_id: String,
    voice_id: Uuid,
}

/// V2 Close Session Request
#[derive(Debug, Clone, Serialize)]
struct CloseSessionRequest {
    session_id: String,
}

/// V2 Submit Inference Request
#[derive(Debug, Clone, Serialize)]
struct SubmitInferRequest {
    session_id: String,
    segment_indices: Vec<u32>,
}

/// V2 Query Task Status Request
#[derive(Debug, Clone, Serialize)]
struct QueryTaskStatusRequest {
    task_ids: Vec<String>,
}

/// V2 Get Audio Request
#[derive(Debug, Clone, Serialize)]
struct GetAudioRequest {
    novel_id: Uuid,
    segment_index: u32,
    voice_id: Uuid,
}

// ============================================================================
// API Client (Blocking) - V2
// ============================================================================

#[derive(Clone, Resource)]
pub struct ApiClient {
    client: reqwest::blocking::Client,
    base_url: String,
}

impl Default for ApiClient {
    fn default() -> Self {
        Self::new(BASE_URL.to_string())
    }
}

impl ApiClient {
    pub fn new(base_url: String) -> Self {
        Self {
            client: reqwest::blocking::Client::new(),
            base_url,
        }
    }

    /// 通用 GET 请求
    fn get<T: DeserializeOwned>(&self, endpoint: &str) -> Result<T> {
        let url = format!("{}{}", self.base_url, endpoint);
        let resp: ApiResponse<T> = self.client.get(&url).send()?.json()?;
        resp.into_result()
    }

    /// 通用 POST 请求
    fn post<R: Serialize, T: DeserializeOwned>(&self, endpoint: &str, body: &R) -> Result<T> {
        let url = format!("{}{}", self.base_url, endpoint);
        let resp: ApiResponse<T> = self.client.post(&url).json(body).send()?.json()?;
        resp.into_result()
    }

    /// POST 请求（无返回数据）
    fn post_empty<R: Serialize>(&self, endpoint: &str, body: &R) -> Result<()> {
        let url = format!("{}{}", self.base_url, endpoint);
        let resp: ApiResponse<EmptyData> = self.client.post(&url).json(body).send()?.json()?;
        if resp.errno == 0 {
            Ok(())
        } else {
            Err(anyhow::anyhow!("API error ({}): {}", resp.errno, resp.error))
        }
    }

    // ========================================================================
    // Novel APIs
    // ========================================================================

    pub fn list_novels(&self) -> Result<Vec<NovelResponse>> {
        self.get("/novel/list")
    }

    pub fn get_novel(&self, id: Uuid) -> Result<NovelResponse> {
        self.post("/novel/get", &IdRequest { id })
    }

    /// 获取小说段落 (V2: 通过 novel_id 直接查询，不需要 session)
    pub fn get_novel_segments(&self, novel_id: Uuid, start: Option<usize>, limit: Option<usize>) -> Result<SegmentsResponse> {
        self.post("/novel/segments", &GetNovelSegmentsRequest { novel_id, start, limit })
    }

    pub fn upload_novel(&self, title: &str, file_path: &std::path::Path) -> Result<NovelResponse> {
        let url = format!("{}/novel/upload", self.base_url);
        let file_name = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("novel.txt");
        let file_content = std::fs::read(file_path)?;

        let form = reqwest::blocking::multipart::Form::new()
            .text("title", title.to_string())
            .part(
                "file",
                reqwest::blocking::multipart::Part::bytes(file_content)
                    .file_name(file_name.to_string())
                    .mime_str("text/plain; charset=utf-8")?,
            );

        let resp: ApiResponse<NovelResponse> =
            self.client.post(&url).multipart(form).send()?.json()?;
        resp.into_result()
    }

    pub fn delete_novel(&self, id: Uuid) -> Result<()> {
        self.post_empty("/novel/delete", &IdRequest { id })
    }

    // ========================================================================
    // Voice APIs
    // ========================================================================

    pub fn list_voices(&self) -> Result<Vec<VoiceResponse>> {
        self.get("/voice/list")
    }

    pub fn upload_voice(
        &self,
        name: &str,
        description: Option<&str>,
        file_path: &std::path::Path,
    ) -> Result<VoiceResponse> {
        let url = format!("{}/voice/upload", self.base_url);
        let file_name = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("voice.wav");
        let file_content = std::fs::read(file_path)?;

        let mime_type = match file_path.extension().and_then(|e| e.to_str()) {
            Some("wav") => "audio/wav",
            Some("mp3") => "audio/mpeg",
            Some("flac") => "audio/flac",
            Some("ogg") => "audio/ogg",
            _ => "audio/wav",
        };

        let mut form = reqwest::blocking::multipart::Form::new()
            .text("name", name.to_string())
            .part(
                "file",
                reqwest::blocking::multipart::Part::bytes(file_content)
                    .file_name(file_name.to_string())
                    .mime_str(mime_type)?,
            );

        if let Some(desc) = description {
            form = form.text("description", desc.to_string());
        }

        let resp: ApiResponse<VoiceResponse> =
            self.client.post(&url).multipart(form).send()?.json()?;
        resp.into_result()
    }

    pub fn delete_voice(&self, id: Uuid) -> Result<()> {
        self.post_empty("/voice/delete", &IdRequest { id })
    }

    // ========================================================================
    // Session APIs (V2)
    // ========================================================================

    /// V2: 开始播放，按需创建 Session
    pub fn play(&self, novel_id: Uuid, voice_id: Uuid, start_index: u32) -> Result<PlayResponse> {
        self.post("/session/play", &PlayRequest { novel_id, voice_id, start_index })
    }

    /// V2: Seek 到指定位置，自动取消旧任务
    pub fn seek(&self, session_id: &str, segment_index: u32) -> Result<SeekResponse> {
        self.post("/session/seek", &SeekRequest { 
            session_id: session_id.to_string(), 
            segment_index 
        })
    }

    /// V2: 切换音色，自动取消旧任务
    pub fn change_voice(&self, session_id: &str, voice_id: Uuid) -> Result<ChangeVoiceResponse> {
        self.post("/session/change_voice", &ChangeVoiceRequest { 
            session_id: session_id.to_string(), 
            voice_id 
        })
    }

    /// V2: 关闭 Session
    pub fn close_session(&self, session_id: &str) -> Result<CloseSessionResponse> {
        self.post("/session/close", &CloseSessionRequest { 
            session_id: session_id.to_string() 
        })
    }

    // ========================================================================
    // Inference APIs (V2)
    // ========================================================================

    /// V2: 提交推理任务
    pub fn submit_infer(&self, session_id: &str, segment_indices: Vec<u32>) -> Result<SubmitInferResponse> {
        self.post("/infer/submit", &SubmitInferRequest { 
            session_id: session_id.to_string(), 
            segment_indices 
        })
    }

    /// V2: 查询任务状态
    pub fn query_task_status(&self, task_ids: Vec<String>) -> Result<QueryTaskStatusResponse> {
        self.post("/infer/status", &QueryTaskStatusRequest { task_ids })
    }

    // ========================================================================
    // Audio API (V2)
    // ========================================================================

    /// V2: 获取音频 (通过 novel_id + segment_index + voice_id)
    pub fn get_audio(&self, novel_id: Uuid, segment_index: u32, voice_id: Uuid) -> Result<Option<Vec<u8>>> {
        let url = format!("{}/audio", self.base_url);
        let resp = self
            .client
            .post(&url)
            .json(&GetAudioRequest {
                novel_id,
                segment_index,
                voice_id,
            })
            .send()?;

        if resp.status().is_success() {
            // 检查是否是 JSON 错误响应
            let content_type = resp
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");

            if content_type.contains("application/json") {
                let api_resp: ApiResponse<EmptyData> = resp.json()?;
                if api_resp.errno != 0 {
                    return Ok(None); // 音频未准备好
                }
                Ok(None)
            } else {
                // 二进制音频数据
                let bytes = resp.bytes()?;
                Ok(Some(bytes.to_vec()))
            }
        } else {
            Ok(None)
        }
    }
}
