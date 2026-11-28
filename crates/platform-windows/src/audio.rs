//! AsterShell Core Audio Volume Query & Event Notification Subsystem
//!
//! Queries the default Windows audio render endpoint via Core Audio (`IMMDeviceEnumerator`
//! and `IAudioEndpointVolume`), subscribes to `IAudioEndpointVolumeCallback` for volume/mute
//! updates, and subscribes to `IMMNotificationClient` for runtime default endpoint switching,
//! removal, and state changes — all without polling.

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

/// Control messages processed by the audio monitor thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioControlEvent {
    RebindEndpoint,
    Stop,
}

/// Tracks active callback registration invariants across endpoint switches (unit-testable).
#[derive(Debug, Default)]
pub struct EndpointRebindTracker {
    pub active_endpoint_id: Option<String>,
    pub active_registrations: usize,
    pub total_registrations: usize,
    pub total_unregistrations: usize,
}

impl EndpointRebindTracker {
    pub fn rebind(&mut self, new_endpoint_id: Option<&str>) {
        // Always unregister existing callback before registering a replacement
        if self.active_registrations > 0 {
            self.active_registrations -= 1;
            self.total_unregistrations += 1;
        }
        self.active_endpoint_id = new_endpoint_id.map(ToString::to_string);
        if self.active_endpoint_id.is_some() {
            self.active_registrations += 1;
            self.total_registrations += 1;
        }
    }

    pub fn shutdown(&mut self) {
        if self.active_registrations > 0 {
            self.active_registrations -= 1;
            self.total_unregistrations += 1;
        }
        self.active_endpoint_id = None;
    }
}

/// Handle to an active Core Audio volume & endpoint-notification subscription.
pub struct AudioMonitorHandle {
    #[cfg(windows)]
    ctrl_tx: Option<Sender<AudioControlEvent>>,
    join_handle: Option<JoinHandle<()>>,
}

impl AudioMonitorHandle {
    /// Starts an event-driven Core Audio monitor emitting `AudioVolumeState` on volume,
    /// mute, or default audio endpoint changes.
    pub fn start() -> Result<(Self, Receiver<AudioVolumeState>)> {
        let (tx, rx) = mpsc::channel();

        #[cfg(windows)]
        {
            let (ctrl_tx, ctrl_rx) = mpsc::channel();
            let handle = std::thread::Builder::new()
                .name("astershell-audio-monitor".to_string())
                .spawn(move || {
                    win32::run_audio_monitor(tx, ctrl_rx);
                })
                .map_err(|e| {
                    astercore::AsterError::Platform(format!(
                        "Failed to spawn audio monitor thread: {}",
                        e
                    ))
                })?;

            Ok((
                Self {
                    ctrl_tx: Some(ctrl_tx),
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
            if let Some(tx) = self.ctrl_tx.take() {
                let _ = tx.send(AudioControlEvent::Stop);
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
    use windows::core::{implement, PCWSTR};
    use windows::Win32::Media::Audio::Endpoints::{
        IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl,
    };
    use windows::Win32::Media::Audio::{
        eConsole, eRender, EDataFlow, ERole, IMMDeviceEnumerator, IMMNotificationClient,
        IMMNotificationClient_Impl, MMDeviceEnumerator, AUDIO_VOLUME_NOTIFICATION_DATA,
        DEVICE_STATE,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
    };
    use windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY;

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

    #[implement(IMMNotificationClient)]
    struct EndpointNotificationClient {
        ctrl_tx: Sender<AudioControlEvent>,
    }

    impl IMMNotificationClient_Impl for EndpointNotificationClient_Impl {
        fn OnDeviceStateChanged(
            &self,
            _pwstrdeviceid: &PCWSTR,
            _dwnewstate: DEVICE_STATE,
        ) -> windows::core::Result<()> {
            let _ = self.ctrl_tx.send(AudioControlEvent::RebindEndpoint);
            Ok(())
        }

        fn OnDeviceAdded(&self, _pwstrdeviceid: &PCWSTR) -> windows::core::Result<()> {
            Ok(())
        }

        fn OnDeviceRemoved(&self, _pwstrdeviceid: &PCWSTR) -> windows::core::Result<()> {
            let _ = self.ctrl_tx.send(AudioControlEvent::RebindEndpoint);
            Ok(())
        }

        fn OnDefaultDeviceChanged(
            &self,
            flow: EDataFlow,
            role: ERole,
            _pwstrdefaultdeviceid: &PCWSTR,
        ) -> windows::core::Result<()> {
            if flow == eRender && role == eConsole {
                let _ = self.ctrl_tx.send(AudioControlEvent::RebindEndpoint);
            }
            Ok(())
        }

        fn OnPropertyValueChanged(
            &self,
            _pwstrdeviceid: &PCWSTR,
            _key: &PROPERTYKEY,
        ) -> windows::core::Result<()> {
            Ok(())
        }
    }

    fn get_endpoint_volume(
        enumerator: &IMMDeviceEnumerator,
    ) -> windows::core::Result<IAudioEndpointVolume> {
        unsafe {
            let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
            device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)
        }
    }

    pub fn query_master_volume_win32() -> Result<AudioVolumeState> {
        unsafe {
            let com_init = CoInitializeEx(None, COINIT_MULTITHREADED);
            let res = match CoCreateInstance::<_, IMMDeviceEnumerator>(
                &MMDeviceEnumerator,
                None,
                CLSCTX_ALL,
            )
            .and_then(|enumerator| get_endpoint_volume(&enumerator))
            {
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

    fn bind_active_endpoint(
        enumerator: &IMMDeviceEnumerator,
        tx: &Sender<AudioVolumeState>,
        active_binding: &mut Option<(IAudioEndpointVolume, IAudioEndpointVolumeCallback)>,
    ) {
        // 1. Unregister old callback BEFORE releasing the old COM interface
        if let Some((old_ep, old_cb)) = active_binding.take() {
            unsafe {
                let _ = old_ep.UnregisterControlChangeNotify(&old_cb);
            }
        }

        // 2. Bind to current default render endpoint
        match get_endpoint_volume(enumerator) {
            Ok(endpoint) => unsafe {
                let scalar = endpoint.GetMasterVolumeLevelScalar().unwrap_or(1.0);
                let muted = endpoint.GetMute().map(|b| b.as_bool()).unwrap_or(false);
                let _ = tx.send(AudioVolumeState {
                    volume_percent: (scalar * 100.0).round().clamp(0.0, 100.0) as u8,
                    is_muted: muted,
                    available: true,
                });

                let cb: IAudioEndpointVolumeCallback = VolumeCallback { tx: tx.clone() }.into();
                if endpoint.RegisterControlChangeNotify(&cb).is_ok() {
                    *active_binding = Some((endpoint, cb));
                }
            },
            Err(e) => {
                tracing::debug!(error = %e, "Default audio render endpoint unavailable after switch/removal");
                let _ = tx.send(AudioVolumeState::default());
            }
        }
    }

    pub fn run_audio_monitor(tx: Sender<AudioVolumeState>, ctrl_rx: Receiver<AudioControlEvent>) {
        run_audio_monitor_loop(tx, ctrl_rx);
    }

    fn run_audio_monitor_loop(tx: Sender<AudioVolumeState>, stop_rx: Receiver<AudioControlEvent>) {
        unsafe {
            let com_init = CoInitializeEx(None, COINIT_MULTITHREADED);

            if let Ok(enumerator) =
                CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL)
            {
                let (event_tx, event_rx) = mpsc::channel();
                let forward_tx = event_tx.clone();
                let _forwarder = std::thread::Builder::new()
                    .name("astershell-audio-stop-fwd".to_string())
                    .spawn(move || {
                        while let Ok(ev) = stop_rx.recv() {
                            let is_stop = ev == AudioControlEvent::Stop;
                            let _ = forward_tx.send(ev);
                            if is_stop {
                                break;
                            }
                        }
                    })
                    .ok();

                let notif_client: IMMNotificationClient =
                    EndpointNotificationClient { ctrl_tx: event_tx }.into();
                let notif_registered = enumerator
                    .RegisterEndpointNotificationCallback(&notif_client)
                    .is_ok();

                let mut active_binding: Option<(
                    IAudioEndpointVolume,
                    IAudioEndpointVolumeCallback,
                )> = None;
                bind_active_endpoint(&enumerator, &tx, &mut active_binding);

                while let Ok(ev) = event_rx.recv() {
                    match ev {
                        AudioControlEvent::RebindEndpoint => {
                            tracing::info!(
                                "Default audio endpoint changed/removed; rebinding volume callback"
                            );
                            bind_active_endpoint(&enumerator, &tx, &mut active_binding);
                        }
                        AudioControlEvent::Stop => break,
                    }
                }

                // Unregister volume callback and endpoint notification client before COM teardown
                if let Some((old_ep, old_cb)) = active_binding.take() {
                    let _ = old_ep.UnregisterControlChangeNotify(&old_cb);
                }
                if notif_registered {
                    let _ = enumerator.UnregisterEndpointNotificationCallback(&notif_client);
                }
            } else {
                let _ = tx.send(AudioVolumeState::default());
                while let Ok(ev) = stop_rx.recv() {
                    if ev == AudioControlEvent::Stop {
                        break;
                    }
                }
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

    #[test]
    fn test_endpoint_rebind_tracker_prevents_duplicate_registrations() {
        let mut tracker = EndpointRebindTracker::default();
        tracker.rebind(Some("speaker-1"));
        assert_eq!(tracker.active_registrations, 1);
        assert_eq!(tracker.total_registrations, 1);
        assert_eq!(tracker.total_unregistrations, 0);

        // Switching to headset unregisters speaker-1 first, keeping active_registrations == 1
        tracker.rebind(Some("headset-2"));
        assert_eq!(tracker.active_registrations, 1);
        assert_eq!(tracker.total_registrations, 2);
        assert_eq!(tracker.total_unregistrations, 1);

        // Device removal leaves 0 active registrations
        tracker.rebind(None);
        assert_eq!(tracker.active_registrations, 0);
        assert_eq!(tracker.total_unregistrations, 2);

        // Shutdown when already unbound is a safe no-op
        tracker.shutdown();
        assert_eq!(tracker.active_registrations, 0);
        assert_eq!(tracker.total_unregistrations, 2);
    }
}
