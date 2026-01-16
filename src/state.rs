//! Application State - V2 Architecture
//!
//! V2 主要变化:
//! - 移除 SessionResponse（V1），使用简单的 session_id 字符串
//! - 新增任务管理状态（滑动窗口预取）
//! - 新增 WebSocket 连接状态

use bevy::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Instant;
use uuid::Uuid;

use crate::api::{NovelResponse, VoiceResponse, SegmentResponse, TaskInfo, WsEvent};

/// 应用视图状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, States, Hash)]
pub enum AppView {
    #[default]
    NovelList,
    Player,
}

/// 文件选择类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilePickerType {
    Novel,
    Voice,
}

/// 上传对话框状态
#[derive(Default)]
pub struct UploadDialogState {
    /// 是否显示上传小说对话框
    pub show_novel_dialog: bool,
    /// 是否显示上传音色对话框
    pub show_voice_dialog: bool,
    /// 小说标题
    pub novel_title: String,
    /// 小说文件路径
    pub novel_file_path: Option<PathBuf>,
    /// 音色名称
    pub voice_name: String,
    /// 音色描述
    pub voice_description: String,
    /// 音色文件路径
    pub voice_file_path: Option<PathBuf>,
    /// 是否正在选择文件
    pub picking_file: bool,
}

impl UploadDialogState {
    pub fn reset_novel(&mut self) {
        self.show_novel_dialog = false;
        self.novel_title.clear();
        self.novel_file_path = None;
    }

    pub fn reset_voice(&mut self) {
        self.show_voice_dialog = false;
        self.voice_name.clear();
        self.voice_description.clear();
        self.voice_file_path = None;
    }
}

/// 文件选择请求事件
#[derive(Event)]
pub struct FilePickerRequest {
    pub picker_type: FilePickerType,
}

/// 文件选择结果事件
#[derive(Event)]
pub struct FilePickerResult {
    pub picker_type: FilePickerType,
    pub path: Option<PathBuf>,
}

// ============================================================================
// V2 Session State
// ============================================================================

/// V2 当前会话信息
#[derive(Debug, Clone, Default)]
pub struct CurrentSession {
    /// Session ID (字符串格式)
    pub session_id: String,
    /// 关联的小说 ID
    pub novel_id: Uuid,
    /// 关联的音色 ID
    pub voice_id: Uuid,
    /// 当前播放段落索引
    pub current_index: u32,
}

// ============================================================================
// V2 Task Management (滑动窗口预取)
// ============================================================================

/// 任务状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    Pending,
    Inferring,
    Ready,
    Failed,
    Cancelled,
}

impl From<&str> for TaskState {
    fn from(s: &str) -> Self {
        match s {
            "pending" => TaskState::Pending,
            "inferring" => TaskState::Inferring,
            "ready" => TaskState::Ready,
            "failed" => TaskState::Failed,
            "cancelled" => TaskState::Cancelled,
            _ => TaskState::Pending,
        }
    }
}

/// 单个段落的任务信息
#[derive(Debug, Clone)]
pub struct SegmentTask {
    pub session_id: String,
    pub task_id: String,
    pub segment_index: u32,
    pub state: TaskState,
    pub duration_ms: Option<u32>,
    pub error: Option<String>,
    pub created_at: Instant,
}

/// 任务管理器状态
#[derive(Debug, Clone, Default)]
pub struct TaskManager {
    /// segment_index -> SegmentTask
    pub tasks: HashMap<u32, SegmentTask>,
    /// 预取窗口大小（向前预取多少段）
    pub prefetch_ahead: u32,
}

impl TaskManager {
    pub fn new(prefetch_ahead: u32) -> Self {
        Self {
            tasks: HashMap::new(),
            prefetch_ahead,
        }
    }

    /// 重置所有任务
    pub fn clear(&mut self) {
        self.tasks.clear();
    }

    /// 计算需要预取的段落索引（只返回不存在的任务）
    pub fn calculate_prefetch_range(&self, current_index: u32, total_segments: u32) -> Vec<u32> {
        let mut needed = Vec::new();
        let end = (current_index + self.prefetch_ahead + 1).min(total_segments);
        for i in current_index..end {
            // 预添加方案：只要任务存在就不需要重新提交
            if !self.tasks.contains_key(&i) {
                needed.push(i);
            }
        }
        needed
    }

    /// 检查段落是否已就绪
    pub fn is_segment_ready(&self, segment_index: u32) -> bool {
        self.tasks
            .get(&segment_index)
            .map(|t| t.state == TaskState::Ready)
            .unwrap_or(false)
    }

    /// 预添加 pending 任务（提交前调用，存在即跳过）
    pub fn add_pending_tasks(&mut self, session_id: &str, indices: &[u32]) {
        let now = Instant::now();
        for &idx in indices {
            // 已存在就跳过，永不重复添加
            if self.tasks.contains_key(&idx) {
                continue;
            }
            self.tasks.insert(idx, SegmentTask {
                session_id: session_id.to_string(),
                task_id: String::new(),
                segment_index: idx,
                state: TaskState::Pending,
                duration_ms: None,
                error: None,
                created_at: now,
            });
        }
    }

    /// 更新任务状态（从 WebSocket 事件）
    /// 验证 session_id 防止旧 session 的事件影响新 session
    pub fn update_task_state(&mut self, current_session_id: &str, event: &WsEvent) {
        if let WsEvent::TaskStateChanged { session_id, task_id, segment_index, state, duration_ms, error } = event {
            // 验证 session_id 是否匹配当前 session
            if session_id != current_session_id {
                tracing::debug!("Ignoring task state from different session: {} != {}", session_id, current_session_id);
                return;
            }
            
            let now = Instant::now();
            
            if let Some(task) = self.tasks.get_mut(segment_index) {
                // 验证任务的 session_id 也匹配
                if task.session_id != *session_id {
                    return;
                }
                task.task_id = task_id.clone();
                task.state = TaskState::from(state.as_str());
                task.duration_ms = *duration_ms;
                task.error = error.clone();
                task.created_at = now;
            } else {
                // 任务可能在 WebSocket 事件到达前还未通过 add_pending_tasks 添加
                // 这是正常情况（WebSocket 比预添加快，理论上不应该发生，但保险起见）
                self.tasks.insert(*segment_index, SegmentTask {
                    session_id: session_id.clone(),
                    task_id: task_id.clone(),
                    segment_index: *segment_index,
                    state: TaskState::from(state.as_str()),
                    duration_ms: *duration_ms,
                    error: error.clone(),
                    created_at: now,
                });
            }
        }
    }

    /// 清理超时的 pending 任务（HTTP 失败或网络问题导致）
    pub fn cleanup_stale_pending(&mut self, timeout_secs: u64) {
        let timeout = std::time::Duration::from_secs(timeout_secs);
        self.tasks.retain(|_, task| {
            // 只清理 pending 状态且超时的任务
            if task.state == TaskState::Pending && task.created_at.elapsed() > timeout {
                tracing::debug!("Cleaning stale pending task: segment {}", task.segment_index);
                false
            } else {
                true
            }
        });
    }
}

// ============================================================================
// V2 WebSocket State
// ============================================================================

/// WebSocket 连接状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WsConnectionState {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
}

// ============================================================================
// Application State
// ============================================================================

/// 段落分页状态
#[derive(Debug, Clone, Default)]
pub struct SegmentPagination {
    /// 总段落数
    pub total_segments: usize,
    /// 每页显示数量
    pub page_size: usize,
    /// 当前加载的段落范围
    pub loaded_range: std::ops::Range<usize>,
    /// 是否还有更多段落
    pub has_more: bool,
    /// 是否正在加载更多
    pub loading_more: bool,
}

impl SegmentPagination {
    pub fn new(page_size: usize) -> Self {
        Self {
            total_segments: 0,
            page_size,
            loaded_range: 0..0,
            has_more: false,
            loading_more: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlaybackState {
    #[default]
    Stopped,
    Playing,
    Paused,
    Loading,
}

/// 应用状态资源 - V2
#[derive(Resource, Default)]
pub struct AppState {
    /// 小说列表
    pub novels: Vec<NovelResponse>,
    /// 音色列表
    pub voices: Vec<VoiceResponse>,
    /// 当前选中的小说
    pub selected_novel: Option<NovelResponse>,
    /// 当前选中的音色
    pub selected_voice: Option<VoiceResponse>,
    /// V2: 当前播放会话
    pub current_session: Option<CurrentSession>,
    /// 当前段落列表（分页加载的段落）
    pub segments: Vec<SegmentResponse>,
    /// 段落分页状态
    pub segment_pagination: SegmentPagination,
    /// 当前播放段落索引
    pub current_segment_index: usize,
    /// 播放状态
    pub playback_state: PlaybackState,
    /// 加载状态
    pub loading: bool,
    /// 错误信息
    pub error: Option<String>,
    /// 上传对话框状态
    pub upload_dialog: UploadDialogState,
    /// 正在处理中的小说 IDs（用于轮询）
    pub processing_novels: HashSet<Uuid>,
    /// V2: 任务管理器
    pub task_manager: TaskManager,
    /// V2: WebSocket 连接状态
    pub ws_state: WsConnectionState,
    /// V2: 等待音频就绪后自动播放
    pub waiting_for_audio: bool,
    /// 需要滚动到的段落索引（用于滑块拖动后同步滚动视图）
    pub scroll_to_segment: Option<usize>,
}

impl AppState {
    pub fn clear_error(&mut self) {
        self.error = None;
    }

    pub fn set_error(&mut self, msg: impl Into<String>) {
        self.error = Some(msg.into());
    }

    pub fn init_segment_pagination(&mut self, total_segments: usize) {
        self.segment_pagination = SegmentPagination::new(100); // 默认每页100段
        self.segment_pagination.total_segments = total_segments;
    }

    /// 初始化任务管理器
    pub fn init_task_manager(&mut self) {
        self.task_manager = TaskManager::new(3); // 向前预取 3 段
    }
}

// ============================================================================
// API Events - V2
// ============================================================================

/// API 请求事件 - V2
#[derive(Event)]
pub enum ApiRequest {
    // Novel
    LoadNovels,
    LoadVoices,
    UploadNovel { title: String, file_path: PathBuf },
    UploadVoice { name: String, description: Option<String>, file_path: PathBuf },
    DeleteNovel(Uuid),
    DeleteVoice(Uuid),
    PollNovelStatus(Uuid),
    
    // Session (V2)
    /// 开始播放（按需创建 session）
    Play { novel_id: Uuid, voice_id: Uuid, start_index: u32 },
    /// Seek 到指定段落
    Seek { session_id: String, segment_index: u32 },
    /// 切换音色
    ChangeVoice { session_id: String, voice_id: Uuid },
    /// 关闭 session
    CloseSession(String),
    
    // Inference (V2)
    /// 提交推理任务
    SubmitInfer { session_id: String, segment_indices: Vec<u32> },
    /// 查询任务状态
    QueryTaskStatus { task_ids: Vec<String> },
    
    // Audio (V2)
    /// 获取音频
    LoadAudio { novel_id: Uuid, segment_index: u32, voice_id: Uuid },
    
    // Segments
    /// 加载段落列表
    LoadSegments { novel_id: Uuid, start: Option<usize>, limit: Option<usize> },
}

/// API 响应事件 - V2
#[derive(Event)]
pub enum ApiResponse {
    // Novel
    NovelsLoaded(Vec<NovelResponse>),
    VoicesLoaded(Vec<VoiceResponse>),
    NovelUploaded(NovelResponse),
    VoiceUploaded(VoiceResponse),
    NovelDeleted(Uuid),
    VoiceDeleted(Uuid),
    NovelStatusUpdated(NovelResponse),
    
    // Session (V2)
    /// 播放开始，session 已创建
    PlayStarted { session_id: String, novel_id: Uuid, voice_id: Uuid, current_index: u32 },
    /// Seek 完成
    SeekCompleted { session_id: String, current_index: u32, cancelled_tasks: usize },
    /// 音色切换完成
    VoiceChanged { session_id: String, voice_id: Uuid, cancelled_tasks: usize },
    /// Session 已关闭
    SessionClosed(String),
    
    // Inference (V2)
    /// 任务已提交
    InferSubmitted { tasks: Vec<TaskInfo> },
    /// 任务状态查询结果
    TaskStatusQueried { tasks: Vec<crate::api::TaskStatusInfo> },
    
    // Audio (V2)
    /// 音频已加载
    AudioLoaded { novel_id: Uuid, segment_index: u32, data: Vec<u8> },
    /// 音频未就绪
    AudioNotReady { novel_id: Uuid, segment_index: u32 },
    
    // Segments
    /// 段落已加载
    SegmentsLoaded { novel_id: Uuid, total: usize, segments: Vec<SegmentResponse> },
    
    // Error
    Error(String),
}

// ============================================================================
// WebSocket Events - V2
// ============================================================================

/// WebSocket 事件
#[derive(Event)]
pub enum WsRequest {
    /// 连接到 session
    Connect(String),
    /// 断开连接
    Disconnect,
}

/// WebSocket 响应事件
#[derive(Event)]
pub enum WsResponse {
    /// 连接成功
    Connected,
    /// 连接断开
    Disconnected,
    /// 收到任务状态变更
    TaskStateChanged(WsEvent),
    /// Session 被服务端关闭
    SessionClosedByServer { session_id: String, reason: String },
    /// 连接错误
    Error(String),
}

// ============================================================================
// Audio Events
// ============================================================================

/// 音频播放事件
#[derive(Event)]
pub struct PlayAudioEvent {
    pub data: Vec<u8>,
}

/// 停止音频事件
#[derive(Event)]
pub struct StopAudioEvent;

/// 暂停音频事件
#[derive(Event)]
pub struct PauseAudioEvent;

/// 恢复音频事件
#[derive(Event)]
pub struct ResumeAudioEvent;

/// 音频播放完成事件
#[derive(Event)]
pub struct AudioFinishedEvent;
