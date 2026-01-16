//! Bevy Systems for API communication and state management - V2 Architecture
//!
//! V2 主要变化:
//! - 移除 Session CRUD (list/create/delete)
//! - 移除 Pause/Next/Sync (客户端驱动)
//! - 新增 SubmitInfer 推理任务提交
//! - 新增 WebSocket 事件处理
//! - 播放完成后自动提交下一批任务

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use futures_lite::future;

use crate::api::ApiClient;
use crate::state::{
    ApiRequest, ApiResponse, AppState, AudioFinishedEvent, CurrentSession,
    PlayAudioEvent, PlaybackState, WsResponse,
};

/// API 任务组件
#[derive(Component)]
pub struct ApiTask(Task<ApiResponse>);

/// 启动时加载数据
pub fn startup_load(mut api_events: EventWriter<ApiRequest>) {
    api_events.send(ApiRequest::LoadNovels);
    api_events.send(ApiRequest::LoadVoices);
}

/// 处理 API 请求 - 使用 blocking client 在线程池中执行
pub fn handle_api_requests(
    mut commands: Commands,
    mut events: EventReader<ApiRequest>,
    api_client: Res<ApiClient>,
    mut app_state: ResMut<AppState>,
) {
    let pool = AsyncComputeTaskPool::get();

    for event in events.read() {
        // 设置加载状态（上传操作除外）
        match event {
            ApiRequest::UploadNovel { .. } | ApiRequest::UploadVoice { .. } => {}
            _ => {
                app_state.loading = true;
            }
        }
        
        let client = (*api_client).clone();

        let task = match event {
            // ====== Novel APIs ======
            ApiRequest::LoadNovels => pool.spawn(async move {
                match client.list_novels() {
                    Ok(novels) => ApiResponse::NovelsLoaded(novels),
                    Err(e) => ApiResponse::Error(e.to_string()),
                }
            }),
            ApiRequest::LoadVoices => pool.spawn(async move {
                match client.list_voices() {
                    Ok(voices) => ApiResponse::VoicesLoaded(voices),
                    Err(e) => ApiResponse::Error(e.to_string()),
                }
            }),
            ApiRequest::UploadNovel { title, file_path } => {
                let title = title.clone();
                let path = file_path.clone();
                pool.spawn(async move {
                    match client.upload_novel(&title, &path) {
                        Ok(novel) => ApiResponse::NovelUploaded(novel),
                        Err(e) => ApiResponse::Error(e.to_string()),
                    }
                })
            }
            ApiRequest::UploadVoice { name, description, file_path } => {
                let name = name.clone();
                let desc = description.clone();
                let path = file_path.clone();
                pool.spawn(async move {
                    match client.upload_voice(&name, desc.as_deref(), &path) {
                        Ok(voice) => ApiResponse::VoiceUploaded(voice),
                        Err(e) => ApiResponse::Error(e.to_string()),
                    }
                })
            }
            ApiRequest::DeleteNovel(id) => {
                let id = *id;
                pool.spawn(async move {
                    match client.delete_novel(id) {
                        Ok(_) => ApiResponse::NovelDeleted(id),
                        Err(e) => ApiResponse::Error(e.to_string()),
                    }
                })
            }
            ApiRequest::DeleteVoice(id) => {
                let id = *id;
                pool.spawn(async move {
                    match client.delete_voice(id) {
                        Ok(_) => ApiResponse::VoiceDeleted(id),
                        Err(e) => ApiResponse::Error(e.to_string()),
                    }
                })
            }
            ApiRequest::PollNovelStatus(id) => {
                let id = *id;
                pool.spawn(async move {
                    match client.get_novel(id) {
                        Ok(novel) => ApiResponse::NovelStatusUpdated(novel),
                        Err(e) => ApiResponse::Error(e.to_string()),
                    }
                })
            }

            // ====== Session APIs (V2) ======
            ApiRequest::Play { novel_id, voice_id, start_index } => {
                let novel_id = *novel_id;
                let voice_id = *voice_id;
                let start_index = *start_index;
                pool.spawn(async move {
                    match client.play(novel_id, voice_id, start_index) {
                        Ok(resp) => ApiResponse::PlayStarted {
                            session_id: resp.session_id,
                            novel_id: resp.novel_id,
                            voice_id: resp.voice_id,
                            current_index: resp.current_index,
                        },
                        Err(e) => ApiResponse::Error(e.to_string()),
                    }
                })
            }
            ApiRequest::Seek { session_id, segment_index } => {
                let session_id = session_id.clone();
                let segment_index = *segment_index;
                pool.spawn(async move {
                    match client.seek(&session_id, segment_index) {
                        Ok(resp) => ApiResponse::SeekCompleted {
                            session_id: resp.session_id,
                            current_index: resp.current_index,
                            cancelled_tasks: resp.cancelled_tasks,
                        },
                        Err(e) => ApiResponse::Error(e.to_string()),
                    }
                })
            }
            ApiRequest::ChangeVoice { session_id, voice_id } => {
                let session_id = session_id.clone();
                let voice_id = *voice_id;
                pool.spawn(async move {
                    match client.change_voice(&session_id, voice_id) {
                        Ok(resp) => ApiResponse::VoiceChanged {
                            session_id: resp.session_id,
                            voice_id: resp.voice_id,
                            cancelled_tasks: resp.cancelled_tasks,
                        },
                        Err(e) => ApiResponse::Error(e.to_string()),
                    }
                })
            }
            ApiRequest::CloseSession(session_id) => {
                let session_id = session_id.clone();
                pool.spawn(async move {
                    match client.close_session(&session_id) {
                        Ok(resp) => ApiResponse::SessionClosed(resp.session_id),
                        Err(e) => ApiResponse::Error(e.to_string()),
                    }
                })
            }

            // ====== Inference APIs (V2) ======
            ApiRequest::SubmitInfer { session_id, segment_indices } => {
                let session_id = session_id.clone();
                let segment_indices = segment_indices.clone();
                pool.spawn(async move {
                    match client.submit_infer(&session_id, segment_indices) {
                        Ok(resp) => ApiResponse::InferSubmitted { tasks: resp.tasks },
                        Err(e) => ApiResponse::Error(e.to_string()),
                    }
                })
            }
            ApiRequest::QueryTaskStatus { task_ids } => {
                let task_ids = task_ids.clone();
                pool.spawn(async move {
                    match client.query_task_status(task_ids) {
                        Ok(resp) => ApiResponse::TaskStatusQueried { tasks: resp.tasks },
                        Err(e) => ApiResponse::Error(e.to_string()),
                    }
                })
            }

            // ====== Audio API (V2) ======
            ApiRequest::LoadAudio { novel_id, segment_index, voice_id } => {
                let novel_id = *novel_id;
                let segment_index = *segment_index;
                let voice_id = *voice_id;
                pool.spawn(async move {
                    match client.get_audio(novel_id, segment_index, voice_id) {
                        Ok(Some(data)) => ApiResponse::AudioLoaded {
                            novel_id,
                            segment_index,
                            data,
                        },
                        Ok(None) => ApiResponse::AudioNotReady {
                            novel_id,
                            segment_index,
                        },
                        Err(e) => ApiResponse::Error(e.to_string()),
                    }
                })
            }

            // ====== Segments ======
            ApiRequest::LoadSegments { novel_id, start, limit } => {
                let novel_id = *novel_id;
                let start = *start;
                let limit = *limit;
                pool.spawn(async move {
                    match client.get_novel_segments(novel_id, start, limit) {
                        Ok(resp) => ApiResponse::SegmentsLoaded {
                            novel_id: resp.novel_id,
                            total: resp.total,
                            segments: resp.segments,
                        },
                        Err(e) => ApiResponse::Error(e.to_string()),
                    }
                })
            }
        };

        commands.spawn(ApiTask(task));
    }
}

/// 轮询 API 任务完成
pub fn poll_api_tasks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut ApiTask)>,
    mut response_events: EventWriter<ApiResponse>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        if let Some(response) = future::block_on(future::poll_once(&mut task.0)) {
            response_events.send(response);
            commands.entity(entity).despawn();
        }
    }
}

/// 处理 API 响应 - V2
pub fn handle_api_responses(
    mut events: EventReader<ApiResponse>,
    mut app_state: ResMut<AppState>,
    mut play_audio: EventWriter<PlayAudioEvent>,
    mut api_events: EventWriter<ApiRequest>,
    mut ws_events: EventWriter<crate::state::WsRequest>,
) {
    for event in events.read() {
        // 设置 loading = false（上传响应除外）
        match event {
            ApiResponse::NovelUploaded(_) | ApiResponse::VoiceUploaded(_) => {}
            _ => {
                app_state.loading = false;
            }
        }

        match event {
            // ====== Novel Responses ======
            ApiResponse::NovelsLoaded(novels) => {
                app_state.novels = novels.clone();
                app_state.clear_error();
            }
            ApiResponse::VoicesLoaded(voices) => {
                app_state.voices = voices.clone();
                if app_state.selected_voice.is_none() && !voices.is_empty() {
                    app_state.selected_voice = Some(voices[0].clone());
                }
                app_state.clear_error();
            }
            ApiResponse::NovelUploaded(novel) => {
                // 替换临时小说对象
                let mut temp_found = false;
                for existing in app_state.novels.iter_mut() {
                    if existing.is_temporary && existing.status == "uploading" {
                        *existing = novel.clone();
                        temp_found = true;
                        break;
                    }
                }
                if !temp_found {
                    app_state.novels.push(novel.clone());
                }
                if novel.status == "processing" {
                    app_state.processing_novels.insert(novel.id);
                }
                app_state.clear_error();
            }
            ApiResponse::VoiceUploaded(voice) => {
                app_state.voices.push(voice.clone());
                if app_state.selected_voice.is_none() {
                    app_state.selected_voice = Some(voice.clone());
                }
                app_state.clear_error();
            }
            ApiResponse::NovelDeleted(_id) => {
                api_events.send(ApiRequest::LoadNovels);
                app_state.selected_novel = None;
                app_state.clear_error();
            }
            ApiResponse::VoiceDeleted(_id) => {
                api_events.send(ApiRequest::LoadVoices);
                app_state.clear_error();
            }
            ApiResponse::NovelStatusUpdated(novel) => {
                if let Some(existing) = app_state.novels.iter_mut().find(|n| n.id == novel.id) {
                    existing.status = novel.status.clone();
                    existing.total_segments = novel.total_segments;
                    existing.is_temporary = false;
                }
                if novel.status != "processing" {
                    app_state.processing_novels.remove(&novel.id);
                }
                app_state.clear_error();
            }

            // ====== Session Responses (V2) ======
            ApiResponse::PlayStarted { session_id, novel_id, voice_id, current_index } => {
                // 创建 session 状态
                app_state.current_session = Some(CurrentSession {
                    session_id: session_id.clone(),
                    novel_id: *novel_id,
                    voice_id: *voice_id,
                    current_index: *current_index,
                });
                app_state.current_segment_index = *current_index as usize;
                app_state.playback_state = PlaybackState::Loading;
                app_state.waiting_for_audio = true;  // 等待音频
                
                // 初始化任务管理器
                app_state.init_task_manager();
                
                // 连接 WebSocket
                ws_events.send(crate::state::WsRequest::Connect(session_id.clone()));
                
                // 加载段落列表（回调中会提交推理任务）- 初始只加载 30 段
                api_events.send(ApiRequest::LoadSegments {
                    novel_id: *novel_id,
                    start: None,
                    limit: Some(30),
                });
                
                // 注意: 此时 total_segments 为 0，推理任务在 SegmentsLoaded 中提交
                
                app_state.clear_error();
            }
            ApiResponse::SeekCompleted { session_id, current_index, cancelled_tasks: _ } => {
                if let Some(session) = &mut app_state.current_session {
                    if session.session_id == *session_id {
                        session.current_index = *current_index;
                        app_state.current_segment_index = *current_index as usize;
                        
                        // 清除旧任务，提交新任务
                        app_state.task_manager.clear();
                        app_state.playback_state = PlaybackState::Loading;
                        app_state.waiting_for_audio = true;
                        
                        let total = app_state.segment_pagination.total_segments as u32;
                        let indices = app_state.task_manager.calculate_prefetch_range(*current_index, total);
                        if !indices.is_empty() {
                            // 预添加 pending 任务
                            app_state.task_manager.add_pending_tasks(session_id, &indices);
                            api_events.send(ApiRequest::SubmitInfer {
                                session_id: session_id.clone(),
                                segment_indices: indices,
                            });
                        }
                    }
                }
                app_state.clear_error();
            }
            ApiResponse::VoiceChanged { session_id, voice_id, cancelled_tasks: _ } => {
                // 先提取需要的信息
                let should_update = app_state.current_session.as_ref()
                    .map(|s| s.session_id == *session_id)
                    .unwrap_or(false);
                    
                if should_update {
                    // 更新 session voice_id
                    if let Some(session) = &mut app_state.current_session {
                        session.voice_id = *voice_id;
                    }
                    
                    // 获取当前索引
                    let current = app_state.current_session.as_ref()
                        .map(|s| s.current_index)
                        .unwrap_or(0);
                    
                    // 音色变化，清除任务重新提交
                    app_state.task_manager.clear();
                    app_state.playback_state = PlaybackState::Loading;
                    app_state.waiting_for_audio = true;
                    
                    let total = app_state.segment_pagination.total_segments as u32;
                    let indices = app_state.task_manager.calculate_prefetch_range(current, total);
                    if !indices.is_empty() {
                        // 预添加 pending 任务
                        app_state.task_manager.add_pending_tasks(session_id, &indices);
                        api_events.send(ApiRequest::SubmitInfer {
                            session_id: session_id.clone(),
                            segment_indices: indices,
                        });
                    }
                }
                app_state.clear_error();
            }
            ApiResponse::SessionClosed(_session_id) => {
                app_state.current_session = None;
                app_state.playback_state = PlaybackState::Stopped;
                app_state.task_manager.clear();
                ws_events.send(crate::state::WsRequest::Disconnect);
                app_state.clear_error();
            }

            // ====== Inference Responses (V2) ======
            ApiResponse::InferSubmitted { tasks } => {
                // 预添加方案：HTTP 响应不需要处理
                // - task_id 不需要（用 segment_index 作 key）
                // - 时间戳已被 WebSocket 更新刷新
                tracing::info!("InferSubmitted: {} tasks submitted", tasks.len());
                
                // 检查是否有已 ready 的缓存任务（不会有 WebSocket 通知）
                let current_segment = app_state.current_segment_index as u32;
                let mut current_ready = false;
                
                for task in tasks {
                    tracing::info!("  task_id={}, segment_index={}, state={}", task.task_id, task.segment_index, task.state);
                    
                    // 更新任务状态（主要处理缓存命中的 ready 状态）
                    if let Some(segment_task) = app_state.task_manager.tasks.get_mut(&task.segment_index) {
                        segment_task.task_id = task.task_id.clone();
                        segment_task.state = crate::state::TaskState::from(task.state.as_str());
                    }
                    
                    // 检查当前段是否已 ready
                    if task.segment_index == current_segment && task.state == "ready" {
                        current_ready = true;
                    }
                }
                
                // 如果当前段已 ready（缓存命中），立即加载音频
                if current_ready && (app_state.waiting_for_audio || app_state.playback_state == PlaybackState::Loading) {
                    if let Some(session) = &app_state.current_session {
                        tracing::info!("InferSubmitted: current segment {} is cached ready, loading audio", current_segment);
                        api_events.send(ApiRequest::LoadAudio {
                            novel_id: session.novel_id,
                            segment_index: current_segment,
                            voice_id: session.voice_id,
                        });
                    }
                }
                
                app_state.clear_error();
            }
            ApiResponse::TaskStatusQueried { tasks: _ } => {
                // 任务状态已通过 WebSocket 实时更新，这里可以作为备用
                app_state.clear_error();
            }

            // ====== Audio Responses (V2) ======
            ApiResponse::AudioLoaded { novel_id: _, segment_index, data } => {
                // 播放音频
                play_audio.send(PlayAudioEvent { data: data.clone() });
                
                // 如果是当前段，更新播放状态
                if *segment_index as usize == app_state.current_segment_index {
                    app_state.playback_state = PlaybackState::Playing;
                    app_state.waiting_for_audio = false;
                }
                
                app_state.clear_error();
            }
            ApiResponse::AudioNotReady { novel_id: _, segment_index } => {
                // 音频未就绪，等待 WebSocket 通知
                if *segment_index as usize == app_state.current_segment_index {
                    app_state.waiting_for_audio = true;
                }
            }

            // ====== Segments Response ======
            ApiResponse::SegmentsLoaded { novel_id: _, total: _, segments } => {
                // 使用 selected_novel 的 total_segments 作为真实总数
                let real_total = app_state.selected_novel.as_ref()
                    .map(|n| n.total_segments)
                    .unwrap_or(segments.len());
                
                tracing::info!("SegmentsLoaded: real_total={}, segments.len={}, current loaded_range={:?}", 
                    real_total, segments.len(), app_state.segment_pagination.loaded_range);
                
                // 重置加载状态
                app_state.segment_pagination.loading_more = false;
                
                // 获取段落的起始索引
                let seg_start = segments.first().map(|s| s.index).unwrap_or(0);
                let seg_end = seg_start + segments.len();
                
                // 判断加载类型
                let is_initial_load = app_state.segments.is_empty();
                let is_jump_load = seg_start > 0 && app_state.segments.is_empty();
                let is_append = !app_state.segments.is_empty() && seg_start >= app_state.segment_pagination.loaded_range.end;
                
                if is_initial_load || is_jump_load {
                    // 初始加载或跳转加载：替换
                    app_state.segments = segments.clone();
                    app_state.segment_pagination.loaded_range = seg_start..seg_end;
                    tracing::info!("SegmentsLoaded: load/jump, range={}..{}", seg_start, seg_end);
                } else if is_append {
                    // 追加加载：合并
                    tracing::info!("SegmentsLoaded: append load, seg_start={}, current_end={}", 
                        seg_start, app_state.segment_pagination.loaded_range.end);
                    app_state.segments.extend(segments.clone());
                    app_state.segment_pagination.loaded_range.end = 
                        app_state.segment_pagination.loaded_range.start + app_state.segments.len();
                    tracing::info!("SegmentsLoaded: appended, new range={:?}", app_state.segment_pagination.loaded_range);
                } else {
                    tracing::info!("SegmentsLoaded: ignoring duplicate load");
                }
                
                app_state.segment_pagination.total_segments = real_total;
                app_state.segment_pagination.has_more = app_state.segment_pagination.loaded_range.end < real_total;
                tracing::info!("SegmentsLoaded: has_more={}, loaded_end={}, real_total={}", 
                    app_state.segment_pagination.has_more, 
                    app_state.segment_pagination.loaded_range.end, 
                    real_total);
                
                // 提交推理任务（只在初始/跳转加载时提交）
                if is_initial_load || is_jump_load {
                    if let Some(session) = &app_state.current_session {
                        let current = session.current_index;
                        let session_id = session.session_id.clone();
                        tracing::info!("SegmentsLoaded: current_index={}, session_id={}", current, session_id);
                        let indices = app_state.task_manager.calculate_prefetch_range(current, real_total as u32);
                        tracing::info!("SegmentsLoaded: prefetch indices={:?}", indices);
                        if !indices.is_empty() {
                            app_state.task_manager.add_pending_tasks(&session_id, &indices);
                            api_events.send(ApiRequest::SubmitInfer {
                                session_id,
                                segment_indices: indices.clone(),
                            });
                            tracing::info!("SegmentsLoaded: submitted infer for indices={:?}", indices);
                        }
                    } else {
                        tracing::warn!("SegmentsLoaded: no current_session!");
                    }
                }
                
                app_state.clear_error();
            }

            // ====== Error ======
            ApiResponse::Error(msg) => {
                // 处理上传错误
                if msg.contains("upload") || msg.contains("上传") {
                    if let Some(temp_novel) = app_state.novels.iter_mut()
                        .find(|n| n.is_temporary && n.status == "uploading") {
                        temp_novel.status = "error".to_string();
                        temp_novel.is_temporary = false;
                    }
                }
                app_state.set_error(msg);
            }
        }
    }
}

/// 处理 WebSocket 响应 - V2
pub fn handle_ws_responses(
    mut events: EventReader<WsResponse>,
    mut app_state: ResMut<AppState>,
    mut api_events: EventWriter<ApiRequest>,
) {
    for event in events.read() {
        match event {
            WsResponse::Connected => {
                app_state.ws_state = crate::state::WsConnectionState::Connected;
            }
            WsResponse::Disconnected => {
                app_state.ws_state = crate::state::WsConnectionState::Disconnected;
            }
            WsResponse::TaskStateChanged(ws_event) => {
                tracing::info!("WsResponse::TaskStateChanged: {:?}", ws_event);
                // 提取 session_id 避免借用冲突
                let current_session_id = app_state.current_session.as_ref().map(|s| s.session_id.clone());
                
                // 验证 session_id 并更新任务状态
                if let Some(ref sid) = current_session_id {
                    app_state.task_manager.update_task_state(sid, ws_event);
                }
                
                // 检查当前段是否就绪
                if let crate::api::WsEvent::TaskStateChanged { session_id, segment_index, state, .. } = ws_event {
                    // 再次验证 session_id
                    let is_current_session = current_session_id.as_ref()
                        .map(|sid| sid == session_id)
                        .unwrap_or(false);
                    
                    if !is_current_session {
                        tracing::warn!("TaskStateChanged: session_id mismatch, ignoring");
                        continue;
                    }
                    
                    let current = app_state.current_segment_index as u32;
                    tracing::info!("TaskStateChanged: segment_index={}, current={}, state={}", segment_index, current, state);
                    
                    if *segment_index == current && state == "ready" {
                        // 当前段就绪，获取音频播放
                        if let Some(session) = &app_state.current_session {
                            tracing::info!("TaskStateChanged: current segment ready, waiting_for_audio={}, playback_state={:?}", 
                                app_state.waiting_for_audio, app_state.playback_state);
                            if app_state.waiting_for_audio || app_state.playback_state == PlaybackState::Loading {
                                tracing::info!("TaskStateChanged: loading audio for segment {}", current);
                                api_events.send(ApiRequest::LoadAudio {
                                    novel_id: session.novel_id,
                                    segment_index: current,
                                    voice_id: session.voice_id,
                                });
                            }
                        }
                    }
                }
            }
            WsResponse::SessionClosedByServer { session_id, reason } => {
                if let Some(session) = &app_state.current_session {
                    if session.session_id == *session_id {
                        app_state.current_session = None;
                        app_state.playback_state = PlaybackState::Stopped;
                        app_state.task_manager.clear();
                        app_state.set_error(format!("Session closed by server: {}", reason));
                    }
                }
            }
            WsResponse::Error(msg) => {
                app_state.ws_state = crate::state::WsConnectionState::Disconnected;
                // 不设置为全局错误，只记录
                tracing::warn!("WebSocket error: {}", msg);
            }
        }
    }
}

/// 处理音频播放完成，自动播放下一段 - V2
pub fn handle_audio_finished(
    mut events: EventReader<AudioFinishedEvent>,
    mut app_state: ResMut<AppState>,
    mut api_events: EventWriter<ApiRequest>,
) {
    for _ in events.read() {
        if app_state.playback_state != PlaybackState::Playing {
            continue;
        }

        // 先提取需要的值，避免借用冲突
        let session_info = app_state.current_session.as_ref().map(|s| {
            (s.session_id.clone(), s.novel_id, s.voice_id)
        });
        
        let Some((session_id, novel_id, voice_id)) = session_info else { continue };
        
        let total = app_state.segment_pagination.total_segments;
        let current = app_state.current_segment_index;
        
        if current + 1 >= total {
            // 已播放完最后一段
            app_state.playback_state = PlaybackState::Stopped;
            continue;
        }
        
        // 移动到下一段
        let next_index = (current + 1) as u32;
        app_state.current_segment_index = next_index as usize;
        
        if let Some(session) = &mut app_state.current_session {
            session.current_index = next_index;
        }
        
        // 检查下一段是否就绪
        if app_state.task_manager.is_segment_ready(next_index) {
            // 直接获取音频
            api_events.send(ApiRequest::LoadAudio {
                novel_id,
                segment_index: next_index,
                voice_id,
            });
        } else {
            // 等待 WebSocket 通知
            app_state.playback_state = PlaybackState::Loading;
            app_state.waiting_for_audio = true;
        }
        
        // 滑动窗口：提交新的预取任务
        let indices = app_state.task_manager.calculate_prefetch_range(next_index, total as u32);
        if !indices.is_empty() {
            // 预添加 pending 任务
            app_state.task_manager.add_pending_tasks(&session_id, &indices);
            api_events.send(ApiRequest::SubmitInfer {
                session_id,
                segment_indices: indices,
            });
        }
    }
}

/// 清除错误（3秒后自动清除）
pub fn clear_error_timer(
    mut app_state: ResMut<AppState>,
    time: Res<Time>,
    mut timer: Local<f32>,
) {
    if app_state.error.is_some() {
        *timer += time.delta_secs();
        if *timer >= 3.0 {
            app_state.clear_error();
            *timer = 0.0;
        }
    } else {
        *timer = 0.0;
    }
}

/// 轮询处理中的小说状态（每 2 秒）
pub fn poll_processing_novels(
    app_state: Res<AppState>,
    mut api_events: EventWriter<ApiRequest>,
    time: Res<Time>,
    mut timer: Local<f32>,
) {
    if app_state.processing_novels.is_empty() {
        *timer = 0.0;
        return;
    }

    *timer += time.delta_secs();

    if *timer >= 2.0 {
        *timer = 0.0;
        for novel_id in app_state.processing_novels.iter() {
            api_events.send(ApiRequest::PollNovelStatus(*novel_id));
        }
    }
}

/// 定期检查任务状态并提交预取任务（每 1 秒）
pub fn prefetch_tasks_system(
    mut app_state: ResMut<AppState>,
    mut api_events: EventWriter<ApiRequest>,
    time: Res<Time>,
    mut timer: Local<f32>,
) {
    // 只在播放中或等待音频时执行
    if app_state.playback_state != PlaybackState::Playing 
        && app_state.playback_state != PlaybackState::Loading {
        *timer = 0.0;
        return;
    }

    let Some(session) = &app_state.current_session else {
        *timer = 0.0;
        return;
    };

    *timer += time.delta_secs();

    if *timer >= 1.0 {
        *timer = 0.0;
        
        let current = session.current_index;
        let session_id = session.session_id.clone();
        let total = app_state.segment_pagination.total_segments as u32;
        let indices = app_state.task_manager.calculate_prefetch_range(current, total);
        
        if !indices.is_empty() {
            // 预添加 pending 任务
            app_state.task_manager.add_pending_tasks(&session_id, &indices);
            api_events.send(ApiRequest::SubmitInfer {
                session_id,
                segment_indices: indices,
            });
        }
    }
}

/// 定期清理超时的 pending 任务（每 5 秒检查，清理 30 秒超时的）
pub fn cleanup_stale_tasks_system(
    mut app_state: ResMut<AppState>,
    time: Res<Time>,
    mut timer: Local<f32>,
) {
    *timer += time.delta_secs();
    if *timer >= 5.0 {
        *timer = 0.0;
        app_state.task_manager.cleanup_stale_pending(30);
    }
}
