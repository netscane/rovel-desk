//! Audio playback system using rodio
//!
//! 由于 rodio 的 OutputStream 不是 Send+Sync，我们使用一个专用线程来处理音频播放

use bevy::prelude::*;
use std::io::Cursor;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;
use std::thread;

use crate::state::{AudioFinishedEvent, PauseAudioEvent, PlayAudioEvent, ResumeAudioEvent, StopAudioEvent};

/// 音频命令
enum AudioCommand {
    Play(Vec<u8>),
    Stop,
    Pause,
    Resume,
}

/// 音频状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioStatus {
    Idle,
    Playing,
    Paused,
    Finished,
}

/// 音频播放器资源 - 通过 channel 与音频线程通信
#[derive(Resource)]
pub struct AudioPlayer {
    command_tx: Sender<AudioCommand>,
    status_rx: Mutex<Receiver<AudioStatus>>,
    current_status: Mutex<AudioStatus>,
    /// 标记是否已通知播放完成，防止重复触发 AudioFinishedEvent
    has_notified_finished: Mutex<bool>,
}

impl AudioPlayer {
    pub fn new() -> Self {
        let (command_tx, command_rx) = mpsc::channel::<AudioCommand>();
        let (status_tx, status_rx) = mpsc::channel::<AudioStatus>();

        // 启动音频线程
        thread::spawn(move || {
            audio_thread(command_rx, status_tx);
        });

        Self {
            command_tx,
            status_rx: Mutex::new(status_rx),
            current_status: Mutex::new(AudioStatus::Idle),
            has_notified_finished: Mutex::new(false),
        }
    }

    pub fn play(&self, data: Vec<u8>) {
        // 重置状态，防止在新音频 Playing 状态到达前误判为 Finished
        if let Ok(mut current) = self.current_status.lock() {
            *current = AudioStatus::Idle;
        }
        // 重置完成通知标记
        if let Ok(mut notified) = self.has_notified_finished.lock() {
            *notified = false;
        }
        let _ = self.command_tx.send(AudioCommand::Play(data));
    }

    pub fn stop(&self) {
        let _ = self.command_tx.send(AudioCommand::Stop);
    }

    pub fn pause(&self) {
        let _ = self.command_tx.send(AudioCommand::Pause);
    }

    pub fn resume(&self) {
        let _ = self.command_tx.send(AudioCommand::Resume);
    }

    pub fn poll_status(&self) -> AudioStatus {
        // 获取最新状态
        if let Ok(rx) = self.status_rx.lock() {
            while let Ok(status) = rx.try_recv() {
                if let Ok(mut current) = self.current_status.lock() {
                    *current = status;
                }
            }
        }
        
        self.current_status.lock().map(|s| *s).unwrap_or(AudioStatus::Idle)
    }
}

/// 音频线程主循环
fn audio_thread(command_rx: Receiver<AudioCommand>, status_tx: Sender<AudioStatus>) {
    use rodio::{Decoder, OutputStream, Sink};

    let (_stream, stream_handle) = match OutputStream::try_default() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to initialize audio output: {}", e);
            return;
        }
    };

    let mut current_sink: Option<Sink> = None;

    loop {
        // 检查命令
        match command_rx.try_recv() {
            Ok(AudioCommand::Play(data)) => {
                // 停止当前播放
                if let Some(sink) = current_sink.take() {
                    sink.stop();
                }

                // 创建新的 sink 并播放
                let cursor = Cursor::new(data);
                match Decoder::new(cursor) {
                    Ok(source) => {
                        match Sink::try_new(&stream_handle) {
                            Ok(sink) => {
                                sink.append(source);
                                current_sink = Some(sink);
                                let _ = status_tx.send(AudioStatus::Playing);
                            }
                            Err(e) => {
                                eprintln!("Failed to create sink: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to decode audio: {}", e);
                    }
                }
            }
            Ok(AudioCommand::Stop) => {
                if let Some(sink) = current_sink.take() {
                    sink.stop();
                }
                let _ = status_tx.send(AudioStatus::Idle);
            }
            Ok(AudioCommand::Pause) => {
                if let Some(ref sink) = current_sink {
                    sink.pause();
                    let _ = status_tx.send(AudioStatus::Paused);
                }
            }
            Ok(AudioCommand::Resume) => {
                if let Some(ref sink) = current_sink {
                    sink.play();
                    let _ = status_tx.send(AudioStatus::Playing);
                }
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                // 主线程已断开，退出
                break;
            }
        }

        // 检查播放是否完成
        if let Some(ref sink) = current_sink {
            if sink.empty() {
                let _ = status_tx.send(AudioStatus::Finished);
                current_sink = None;
            }
        }

        // 短暂休眠以避免忙等待
        thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// 处理播放音频事件
pub fn handle_play_audio(
    mut events: EventReader<PlayAudioEvent>,
    audio_player: Option<Res<AudioPlayer>>,
) {
    let Some(player) = audio_player else { return };

    for event in events.read() {
        player.play(event.data.clone());
    }
}

/// 处理停止音频事件
pub fn handle_stop_audio(
    mut events: EventReader<StopAudioEvent>,
    audio_player: Option<Res<AudioPlayer>>,
) {
    let Some(player) = audio_player else { return };

    for _ in events.read() {
        player.stop();
    }
}

/// 处理暂停音频事件
pub fn handle_pause_audio(
    mut events: EventReader<PauseAudioEvent>,
    audio_player: Option<Res<AudioPlayer>>,
) {
    let Some(player) = audio_player else { return };

    for _ in events.read() {
        player.pause();
    }
}

/// 处理恢复音频事件
pub fn handle_resume_audio(
    mut events: EventReader<ResumeAudioEvent>,
    audio_player: Option<Res<AudioPlayer>>,
) {
    let Some(player) = audio_player else { return };

    for _ in events.read() {
        player.resume();
    }
}

/// 检查音频是否播放完成
pub fn check_audio_finished(
    audio_player: Option<Res<AudioPlayer>>,
    mut finished_events: EventWriter<AudioFinishedEvent>,
) {
    let Some(player) = audio_player else {
        return;
    };

    let status = player.poll_status();
    if status == AudioStatus::Finished {
        // 检查是否已经通知过，防止重复触发
        if let Ok(mut notified) = player.has_notified_finished.lock() {
            if !*notified {
                *notified = true;
                finished_events.send(AudioFinishedEvent);
            }
        }
    }
}
