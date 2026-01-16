//! File Picker System - runs in separate thread to avoid blocking

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use futures_lite::future;

use crate::state::{AppState, FilePickerRequest, FilePickerResult, FilePickerType};

/// File picker task component
#[derive(Component)]
pub struct FilePickerTask {
    task: Task<FilePickerResult>,
}

/// Handle file picker requests - spawn async task
pub fn handle_file_picker_requests(
    mut commands: Commands,
    mut events: EventReader<FilePickerRequest>,
) {
    let pool = AsyncComputeTaskPool::get();
    
    for event in events.read() {
        let picker_type = event.picker_type;
        
        let task = pool.spawn(async move {
            let path = match picker_type {
                FilePickerType::Novel => {
                    rfd::FileDialog::new()
                        .add_filter("文本文件", &["txt"])
                        .pick_file()
                }
                FilePickerType::Voice => {
                    rfd::FileDialog::new()
                        .add_filter("音频文件", &["wav", "mp3", "flac", "ogg"])
                        .pick_file()
                }
            };
            
            FilePickerResult { picker_type, path }
        });
        
        commands.spawn(FilePickerTask { task });
    }
}

/// Poll file picker tasks for completion
pub fn poll_file_picker_tasks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut FilePickerTask)>,
    mut result_events: EventWriter<FilePickerResult>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        if let Some(result) = future::block_on(future::poll_once(&mut task.task)) {
            result_events.send(result);
            commands.entity(entity).despawn();
        }
    }
}

/// Handle file picker results
pub fn handle_file_picker_results(
    mut events: EventReader<FilePickerResult>,
    mut app_state: ResMut<AppState>,
) {
    for event in events.read() {
        app_state.upload_dialog.picking_file = false;
        
        if let Some(path) = &event.path {
            match event.picker_type {
                FilePickerType::Novel => {
                    // Auto-fill title from filename
                    if app_state.upload_dialog.novel_title.is_empty() {
                        if let Some(stem) = path.file_stem() {
                            app_state.upload_dialog.novel_title = stem.to_string_lossy().to_string();
                        }
                    }
                    app_state.upload_dialog.novel_file_path = Some(path.clone());
                }
                FilePickerType::Voice => {
                    // Auto-fill name from filename
                    if app_state.upload_dialog.voice_name.is_empty() {
                        if let Some(stem) = path.file_stem() {
                            app_state.upload_dialog.voice_name = stem.to_string_lossy().to_string();
                        }
                    }
                    app_state.upload_dialog.voice_file_path = Some(path.clone());
                }
            }
        }
    }
}
