//! AsterShell Core Audio Volume Query & Event Notification Subsystem
//!
//! Queries the default Windows audio render endpoint via Core Audio (`IMMDeviceEnumerator`
//! and `IAudioEndpointVolume`) and subscribes to `IAudioEndpointVolumeCallback` for
//! zero-polling volume and mute state updates.

use astercore::Result;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;

/// Represents the current system master audio state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioVolumeState {
    pub volume_percent: u8,
    pub is_muted: bool,
    pub available: bool,
}

impl Default for AudioVolumeState {
    fn default() -> Self {
        Self {
            volume_percent: 100,
            is_muted: false,
            available: false,
        }
    }
}

/// Queries the current default audio render endpoint volume and mute status.
pub fn query_master_volume() -> Result<AudioVolumeState> {
    #[cfg(windows)]
    {
        win32::query_master_volume_win32()
    }
    #[cfg(not(windows))]
    {
        Ok(AudioVolumeState::default())
    }
}

/// Handle to an active Core Audio event callback subscription.
pub struct AudioMonitorHandle {
    #[cfg(windows)]
    stop_tx: Option<Sender<()>>,
    join_handle: Option<JoinHandle<()>>,
}

impl AudioMonitorHandle {
    /// Starts an event-driven Core Audio monitor emitting `AudioVolumeState` on changes.
    pub fn start() -> Result<(Self, Receiver<AudioVolumeState>)> {
        let (tx, rx) = mpsc::channel();

        #[cfg(windows)]
        {
            let (stop_tx, stop_rx) = mpsc::channel();
            let handle = std::thread::Builder::new()
                .name("astershell-audio-monitor".to_string())
                .spawn(move || {
                    win32::run_audio_monitor(tx, stop_rx);
                })
                .map_err(|e| {
                    astercore::AsterError::Platform(format!(
                        "Failed to spawn audio monitor thread: {}",
                        e
                    ))
                })?;

            Ok((
                Self {
                    stop_tx: Some(stop_tx),
                    join_handle: Some(handle),
                },
                rx,
            ))
        }

        #[cfg(not(windows))]
        {
            let _ = tx.send(AudioVolumeState::default());
            Ok((Self { join_handle: None }, rx))
        }
    }
}

impl Drop for AudioMonitorHandle {
    fn drop(&mut self) {
        #[cfg(windows)]
        {
            if let Some(tx) = self.stop_tx.take() {
                let _ = tx.send(());
            }
        }
        if let Some(handle) = self.join_handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(windows)]
mod win32 {
    use super::*;
    use windows::core::implement;
    use windows::Win32::Media::Audio::Endpoints::{
        IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl,
    };
    use windows::Win32::Media::Audio::{
        eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator, AUDIO_VOLUME_NOTIFICATION_DATA,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
    };

    #[implement(IAudioEndpointVolumeCallback)]
    struct VolumeCallback {
        tx: Sender<AudioVolumeState>,
    }

    impl IAudioEndpointVolumeCallback_Impl for VolumeCallback_Impl {
        fn OnNotify(
            &self,
            pnotify: *mut AUDIO_VOLUME_NOTIFICATION_DATA,
        ) -> windows::core::Result<()> {
            if !pnotify.is_null() {
                let data = unsafe { &*pnotify };
                let pct = (data.fMasterVolume * 100.0).round().clamp(0.0, 100.0) as u8;
                let _ = self.tx.send(AudioVolumeState {
                    volume_percent: pct,
                    is_muted: data.bMuted.as_bool(),
                    available: true,
                });
            }
            Ok(())
        }
    }

    fn get_endpoint_volume() -> windows::core::Result<IAudioEndpointVolume> {
        unsafe {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
            device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)
        }
    }

    pub fn query_master_volume_win32() -> Result<AudioVolumeState> {
        unsafe {
            let com_init = CoInitializeEx(None, COINIT_MULTITHREADED);
            let res = match get_endpoint_volume() {
                Ok(vol) => {
                    let scalar = vol.GetMasterVolumeLevelScalar().unwrap_or(1.0);
                    let muted = vol.GetMute().map(|b| b.as_bool()).unwrap_or(false);
                    Ok(AudioVolumeState {
                        volume_percent: (scalar * 100.0).round().clamp(0.0, 100.0) as u8,
                        is_muted: muted,
                        available: true,
                    })
                }
                Err(e) => {
                    tracing::debug!(error = %e, "No default audio render endpoint available");
                    Ok(AudioVolumeState::default())
                }
            };
            if com_init.is_ok() {
                CoUninitialize();
            }
            res
        }
    }

    pub fn run_audio_monitor(tx: Sender<AudioVolumeState>, stop_rx: Receiver<()>) {
        unsafe {
            let com_init = CoInitializeEx(None, COINIT_MULTITHREADED);

            if let Ok(endpoint) = get_endpoint_volume() {
                let scalar = endpoint.GetMasterVolumeLevelScalar().unwrap_or(1.0);
                let muted = endpoint.GetMute().map(|b| b.as_bool()).unwrap_or(false);
                let _ = tx.send(AudioVolumeState {
                    volume_percent: (scalar * 100.0).round().clamp(0.0, 100.0) as u8,
                    is_muted: muted,
                    available: true,
                });

                let cb: IAudioEndpointVolumeCallback = VolumeCallback { tx: tx.clone() }.into();
                if endpoint.RegisterControlChangeNotify(&cb).is_ok() {
                    let _ = stop_rx.recv();
                    let _ = endpoint.UnregisterControlChangeNotify(&cb);
                } else {
                    let _ = stop_rx.recv();
                }
            } else {
                let _ = tx.send(AudioVolumeState::default());
                let _ = stop_rx.recv();
            }

            if com_init.is_ok() {
                CoUninitialize();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_master_volume_returns_valid_bounds() {
        let state = query_master_volume().expect("query_master_volume should not fail");
        assert!(state.volume_percent <= 100);
    }

    #[test]
    fn test_audio_monitor_start_and_clean_drop() {
        let (handle, rx) = AudioMonitorHandle::start().expect("AudioMonitorHandle::start");
        let initial = rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("Must receive initial audio state");
        assert!(initial.volume_percent <= 100);
        drop(handle);
    }
}
