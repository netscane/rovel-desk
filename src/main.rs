//! Rovel Desktop - Novel Player built with Bevy - V2 Architecture
//!
//! A desktop application for playing novels with TTS (Text-to-Speech).
//!
//! V2 主要变化:
//! - 客户端驱动推理任务提交
//! - WebSocket 实时推送任务状态
//! - 滑动窗口预取策略

mod api;
mod audio;
mod file_picker;
mod state;
mod systems;
mod ui;
mod websocket;

use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPlugin, EguiPreUpdateSet};

use api::ApiClient;
use audio::{check_audio_finished, handle_pause_audio, handle_play_audio, handle_resume_audio, handle_stop_audio, AudioPlayer};
use file_picker::{handle_file_picker_requests, handle_file_picker_results, poll_file_picker_tasks, setup_file_picker_channel};
use state::{
    ApiRequest, ApiResponse, AppState, AppView, AudioFinishedEvent, FilePickerRequest,
    FilePickerResult, PauseAudioEvent, PlayAudioEvent, ResumeAudioEvent, StopAudioEvent, WsRequest, WsResponse,
};
use systems::{
    cleanup_stale_tasks_system, clear_error_timer, handle_api_requests, handle_api_responses,
    handle_audio_finished, handle_ws_responses, poll_api_tasks, poll_processing_novels,
    prefetch_tasks_system, setup_api_channel, startup_load,
};
use ui::ui_system;
use websocket::{handle_ws_requests, poll_ws_responses, setup_ws_client};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Rovel - 小说播放器".to_string(),
                resolution: (1200.0, 800.0).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(EguiPlugin)
        // 状态
        .init_state::<AppView>()
        // 资源
        .init_resource::<AppState>()
        .insert_resource(ApiClient::default())
        // 音频播放器
        .add_systems(Startup, setup_audio)
        // API 响应通道
        .add_systems(Startup, setup_api_channel)
        // 文件选择器通道
        .add_systems(Startup, setup_file_picker_channel)
        // WebSocket 客户端
        .add_systems(Startup, setup_ws_client)
        // 事件
        .add_event::<ApiRequest>()
        .add_event::<ApiResponse>()
        .add_event::<PlayAudioEvent>()
        .add_event::<StopAudioEvent>()
        .add_event::<PauseAudioEvent>()
        .add_event::<ResumeAudioEvent>()
        .add_event::<AudioFinishedEvent>()
        .add_event::<FilePickerRequest>()
        .add_event::<FilePickerResult>()
        // V2: WebSocket 事件
        .add_event::<WsRequest>()
        .add_event::<WsResponse>()
        // 启动系统
        .add_systems(Startup, startup_load)
        // 配置中文字体
        .add_systems(Startup, configure_fonts)
        // 更新系统 - 非 UI 相关
        .add_systems(
            Update,
            (
                handle_api_requests,
                poll_api_tasks,
                handle_api_responses,
                // V2: WebSocket 系统
                handle_ws_requests,
                poll_ws_responses,
                handle_ws_responses,
                // 音频系统
                handle_play_audio,
                handle_stop_audio,
                handle_pause_audio,
                handle_resume_audio,
                check_audio_finished,
                handle_audio_finished,
                // 定时任务
                clear_error_timer,
                poll_processing_novels,
                prefetch_tasks_system,
                cleanup_stale_tasks_system,
                // 其他
                handle_voice_click,
                handle_file_picker_requests,
                poll_file_picker_tasks,
                handle_file_picker_results,
            )
                .chain(),
        )
        // UI 系统 - 确保在 egui context 初始化后运行
        .add_systems(
            Update,
            ui_system.after(EguiPreUpdateSet::InitContexts),
        )
        .run();
}

fn setup_audio(mut commands: Commands) {
    let player = AudioPlayer::new();
    commands.insert_resource(player);
}

/// 配置中文字体
fn configure_fonts(mut contexts: EguiContexts) {
    let ctx = contexts.ctx_mut();
    
    // 加载系统中文字体
    let mut fonts = egui::FontDefinitions::default();
    
    // 根据平台尝试不同的字体路径
    let font_paths: Vec<&str> = if cfg!(target_os = "macos") {
        vec![
            "/System/Library/Fonts/STHeiti Medium.ttc",
            "/System/Library/Fonts/PingFang.ttc",
            "/System/Library/Fonts/Hiragino Sans GB.ttc",
        ]
    } else if cfg!(target_os = "windows") {
        vec![
            "C:\\Windows\\Fonts\\msyh.ttc",      // 微软雅黑
            "C:\\Windows\\Fonts\\simsun.ttc",    // 宋体
            "C:\\Windows\\Fonts\\simhei.ttf",    // 黑体
        ]
    } else {
        // Linux
        vec![
            "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
            "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
        ]
    };
    
    // 尝试加载第一个可用的字体
    let mut font_loaded = false;
    for path in font_paths {
        if let Ok(font_data) = std::fs::read(path) {
            fonts.font_data.insert(
                "chinese".to_owned(),
                std::sync::Arc::new(egui::FontData::from_owned(font_data)),
            );
            
            // 将中文字体添加到所有字体族（优先级最高）
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "chinese".to_owned());
            
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push("chinese".to_owned());
            
            tracing::info!("Loaded Chinese font from: {}", path);
            font_loaded = true;
            break;
        }
    }
    
    if !font_loaded {
        tracing::warn!("No Chinese font found! Chinese characters may not display correctly.");
    }
    
    ctx.set_fonts(fonts);
}

/// 处理音色点击选择 - 确保有默认音色
fn handle_voice_click(mut app_state: ResMut<AppState>) {
    // 简单的选择逻辑：如果当前没有选中，选中第一个
    if app_state.selected_voice.is_none() && !app_state.voices.is_empty() {
        app_state.selected_voice = Some(app_state.voices[0].clone());
    }
}
