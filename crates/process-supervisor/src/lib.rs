//! AsterShell Process Supervisor Subsystem

use astercore::{AsterError, ComponentId, ProcessStatus, RestartPolicy, Result};
use platform_windows::JobObject;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Role assigned to an authenticated IPC client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientRole {
    /// Supervised child process (e.g. Taskbar, Search UI). Cannot execute administrative actions like host shutdown.
    SupervisedChild,
    /// Administrative controller (e.g. host CLI, test runner). Authorized for lifecycle administration.
    AdminController,
}

/// Thread-safe registry of authorized supervised process IDs for mutual authentication.
#[derive(Debug, Clone)]
pub struct SupervisedPidRegistry {
    authorized_pids: Arc<RwLock<HashMap<u32, ClientRole>>>,
    strict: bool,
}

impl SupervisedPidRegistry {
    pub fn new(strict: bool) -> Self {
        Self {
            authorized_pids: Arc::new(RwLock::new(HashMap::new())),
            strict,
        }
    }

    pub fn authorize(&self, pid: u32) {
        self.authorize_with_role(pid, ClientRole::SupervisedChild);
    }

    pub fn authorize_admin(&self, pid: u32) {
        self.authorize_with_role(pid, ClientRole::AdminController);
    }

    pub fn authorize_with_role(&self, pid: u32, role: ClientRole) {
        if pid != 0 {
            if let Ok(mut lock) = self.authorized_pids.write() {
                lock.insert(pid, role);
            }
        }
    }

    pub fn deauthorize(&self, pid: u32) {
        if let Ok(mut lock) = self.authorized_pids.write() {
            lock.remove(&pid);
        }
    }

    pub fn is_authorized(&self, pid: u32) -> bool {
        if !self.strict {
            return true;
        }
        if let Ok(lock) = self.authorized_pids.read() {
            lock.contains_key(&pid)
        } else {
            false
        }
    }

    pub fn get_role(&self, pid: u32) -> Option<ClientRole> {
        if let Ok(lock) = self.authorized_pids.read() {
            lock.get(&pid).copied()
        } else {
            None
        }
    }

    pub fn is_admin(&self, pid: u32) -> bool {
        self.get_role(pid) == Some(ClientRole::AdminController)
    }

    pub fn len(&self) -> usize {
        self.authorized_pids.read().map(|l| l.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for SupervisedPidRegistry {
    fn default() -> Self {
        Self::new(true)
    }
}

/// Specification for launching a managed child process.
#[derive(Debug, Clone)]
pub struct ProcessSpec {
    pub id: ComponentId,
    pub executable: PathBuf,
    pub args: Vec<String>,
    pub restart_policy: RestartPolicy,
}

/// Action determined by the supervisor during a health evaluation cycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupervisorAction {
    Running,
    RestartingImmediately(ComponentId),
    BackoffDelay { id: ComponentId, delay_ms: u64 },
    CircuitBroken(ComponentId),
}

/// Internal state tracking for a single supervised child process.
pub struct ManagedChild {
    pub spec: ProcessSpec,
    pub status: ProcessStatus,
    pub child: Option<tokio::process::Child>,
    pub crash_timestamps: Vec<Instant>,
    pub consecutive_crashes: u32,
    pub next_restart_allowed: Option<Instant>,
    pub last_started_at: Option<Instant>,
}

impl ManagedChild {
    pub fn new(spec: ProcessSpec) -> Self {
        Self {
            spec,
            status: ProcessStatus::Uninitialized,
            child: None,
            crash_timestamps: Vec::new(),
            consecutive_crashes: 0,
            next_restart_allowed: None,
            last_started_at: None,
        }
    }

    /// Resets crash counters if the process has run continuously for at least `window_secs`.
    pub fn reset_if_stable(&mut self, now: Instant) -> bool {
        let window_secs = self.spec.restart_policy.window_secs;
        if window_secs == 0 {
            return false;
        }
        if let Some(started) = self.last_started_at {
            if now.duration_since(started) >= Duration::from_secs(window_secs)
                && (self.consecutive_crashes > 0 || !self.crash_timestamps.is_empty())
            {
                self.consecutive_crashes = 0;
                self.crash_timestamps.clear();
                return true;
            }
        }
        false
    }

    /// Evaluates crash history against the restart policy to determine next state.
    pub fn evaluate_crash(&mut self, exit_code: Option<u32>) -> SupervisorAction {
        let now = Instant::now();
        self.reset_if_stable(now);
        self.last_started_at = None;
        self.consecutive_crashes += 1;
        self.crash_timestamps.push(now);

        // Filter out crashes outside window
        let window_duration = Duration::from_secs(self.spec.restart_policy.window_secs);
        self.crash_timestamps
            .retain(|&t| now.duration_since(t) <= window_duration);

        if self.consecutive_crashes >= self.spec.restart_policy.circuit_breaker_threshold {
            self.next_restart_allowed = None;
            self.status = ProcessStatus::CircuitBroken {
                consecutive_crashes: self.consecutive_crashes,
            };
            tracing::error!(
                process = self.spec.id.as_str(),
                crashes = self.consecutive_crashes,
                "Circuit breaker tripped! Process suspended from automatic restarts."
            );
            return SupervisorAction::CircuitBroken(self.spec.id);
        }

        if self.crash_timestamps.len() as u32 > self.spec.restart_policy.max_retries_in_window {
            let backoff_multiplier = 2u64.pow((self.consecutive_crashes - 1).min(6));
            let delay_ms = (self.spec.restart_policy.initial_backoff_ms * backoff_multiplier)
                .min(self.spec.restart_policy.max_off_ms_capped());

            self.next_restart_allowed = Some(now + Duration::from_millis(delay_ms));
            self.status = ProcessStatus::BackoffPaused {
                retry_in_ms: delay_ms,
            };

            tracing::warn!(
                process = self.spec.id.as_str(),
                delay_ms,
                "Rate limit exceeded. Process placed in backoff."
            );
            SupervisorAction::BackoffDelay {
                id: self.spec.id,
                delay_ms,
            }
        } else {
            self.status = ProcessStatus::Crashed { exit_code };
            tracing::info!(
                process = self.spec.id.as_str(),
                exit_code = ?exit_code,
                "Process crashed; restarting immediately."
            );
            SupervisorAction::RestartingImmediately(self.spec.id)
        }
    }
}

trait RestartPolicyExt {
    fn max_off_ms_capped(&self) -> u64;
}

impl RestartPolicyExt for RestartPolicy {
    fn max_off_ms_capped(&self) -> u64 {
        self.max_backoff_ms
    }
}

/// Persistent supervisor managing child processes assigned to a Windows Job Object.
pub struct ProcessSupervisor {
    job_object: JobObject,
    processes: HashMap<ComponentId, ManagedChild>,
    pid_registry: SupervisedPidRegistry,
}

impl ProcessSupervisor {
    pub fn new(job_object: JobObject) -> Self {
        Self {
            job_object,
            processes: HashMap::new(),
            pid_registry: SupervisedPidRegistry::new(true),
        }
    }

    pub fn new_with_registry(job_object: JobObject, registry: SupervisedPidRegistry) -> Self {
        Self {
            job_object,
            processes: HashMap::new(),
            pid_registry: registry,
        }
    }

    pub fn pid_registry(&self) -> SupervisedPidRegistry {
        self.pid_registry.clone()
    }

    pub fn child_pid(&self, id: ComponentId) -> Option<u32> {
        self.processes
            .get(&id)
            .and_then(|m| m.child.as_ref().and_then(|c| c.id()))
    }

    pub fn is_supervised_pid(&self, pid: u32) -> bool {
        self.pid_registry.is_authorized(pid)
    }

    pub fn identify_child(&self, pid: u32) -> Option<ComponentId> {
        for (&id, managed) in &self.processes {
            if let Some(ref child) = managed.child {
                if child.id() == Some(pid) {
                    return Some(id);
                }
            }
        }
        None
    }

    pub fn register(&mut self, spec: ProcessSpec) {
        let id = spec.id;
        self.processes.insert(id, ManagedChild::new(spec));
    }

    /// Spawns a registered child process and binds its handle to the Job Object.
    pub fn spawn(&mut self, id: ComponentId) -> Result<u32> {
        let managed = self
            .processes
            .get_mut(&id)
            .ok_or_else(|| AsterError::Supervisor(format!("Unregistered component: {:?}", id)))?;

        if let Some(ref mut existing) = managed.child {
            let existing_pid = existing.id();
            match existing.try_wait() {
                Ok(None) => {
                    return Err(AsterError::Supervisor(format!(
                        "Component {:?} is already running (pid {:?})",
                        id, existing_pid
                    )));
                }
                Ok(Some(_)) | Err(_) => {
                    if let Some(old_pid) = existing_pid {
                        self.pid_registry.deauthorize(old_pid);
                    }
                    managed.child = None;
                }
            }
        }

        managed.status = ProcessStatus::Starting;

        let mut cmd = tokio::process::Command::new(&managed.spec.executable);
        cmd.args(&managed.spec.args);

        #[cfg(windows)]
        {
            // CREATE_SUSPENDED would allow atomic Job assignment; standard spawn + immediate assign is supported
        }

        let child = cmd
            .spawn()
            .map_err(|e| AsterError::Supervisor(format!("Failed to spawn {:?}: {}", id, e)))?;

        let pid = child.id().unwrap_or(0);

        #[cfg(windows)]
        if let Some(raw_h) = child.raw_handle() {
            if let Err(e) = self.job_object.assign_raw_handle(raw_h as _) {
                tracing::warn!(
                    process = id.as_str(),
                    pid,
                    error = %e,
                    "Failed to assign child handle to JobObject"
                );
            }
        }

        if pid != 0 {
            self.pid_registry.authorize(pid);
        }

        managed.child = Some(child);
        managed.status = ProcessStatus::Running;
        managed.last_started_at = Some(Instant::now());
        managed.next_restart_allowed = None;
        tracing::info!(process = id.as_str(), pid, "Spawned supervised process");
        Ok(pid)
    }

    /// Checks the health of all managed child processes, returning actions taken.
    pub async fn poll_health(&mut self) -> Vec<SupervisorAction> {
        let mut actions = Vec::new();
        let now = Instant::now();

        for (&id, managed) in self.processes.iter_mut() {
            if matches!(
                managed.status,
                ProcessStatus::Stopped | ProcessStatus::CircuitBroken { .. }
            ) && managed.child.is_none()
            {
                continue;
            }

            // Check if backoff delay has elapsed
            if let ProcessStatus::BackoffPaused { .. } = managed.status {
                if let Some(allowed_time) = managed.next_restart_allowed {
                    if now >= allowed_time {
                        managed.next_restart_allowed = None;
                        actions.push(SupervisorAction::RestartingImmediately(id));
                    }
                }
                continue;
            }

            // Check live child status
            if let Some(ref mut child) = managed.child {
                let child_pid = child.id();
                match child.try_wait() {
                    Ok(Some(status)) => {
                        if let Some(old_pid) = child_pid {
                            self.pid_registry.deauthorize(old_pid);
                        }
                        let code = status.code().map(|c| c as u32);
                        let action = managed.evaluate_crash(code);
                        managed.child = None;
                        actions.push(action);
                    }
                    Ok(None) => {
                        if managed.reset_if_stable(now) {
                            tracing::info!(
                                process = id.as_str(),
                                "Supervised process ran stably past restart window; crash counters reset"
                            );
                        }
                    }
                    Err(e) => {
                        tracing::error!(process = id.as_str(), error = %e, "try_wait failed");
                    }
                }
            }
        }

        actions
    }

    /// Polls all managed child processes and respawns any that require recovery.
    pub async fn poll_and_recover(&mut self) -> Vec<SupervisorAction> {
        let actions = self.poll_health().await;

        for action in &actions {
            if let SupervisorAction::RestartingImmediately(id) = *action {
                match self.spawn(id) {
                    Ok(new_pid) => {
                        tracing::info!(
                            process = id.as_str(),
                            pid = new_pid,
                            "Recovered crashed supervised process"
                        );
                    }
                    Err(e) => {
                        tracing::error!(
                            process = id.as_str(),
                            error = %e,
                            "Failed to respawn supervised process"
                        );
                        if let Some(managed) = self.processes.get_mut(&id) {
                            let _ = managed.evaluate_crash(None);
                        }
                    }
                }
            }
        }

        actions
    }

    /// Gracefully terminates all supervised child processes within a given timeout.
    /// Gives child processes a bounded grace period to exit cleanly (e.g. on IPC pipe disconnection);
    /// forcibly terminates with `start_kill` as a fallback if still running after timeout.
    pub async fn shutdown_all(&mut self, timeout: Duration) -> Result<()> {
        tracing::info!("Initiating supervisor graceful shutdown...");
        let start = Instant::now();

        // Phase 1: Wait bounded grace period for children to exit cleanly
        while start.elapsed() < timeout {
            let mut any_alive = false;
            for managed in self.processes.values_mut() {
                if let Some(ref mut child) = managed.child {
                    let child_pid = child.id();
                    match child.try_wait() {
                        Ok(Some(_status)) => {
                            if let Some(pid) = child_pid {
                                self.pid_registry.deauthorize(pid);
                            }
                            managed.child = None;
                            managed.status = ProcessStatus::Stopped;
                        }
                        Ok(None) => {
                            any_alive = true;
                        }
                        Err(_) => {}
                    }
                }
            }
            if !any_alive {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        // Phase 2: For any child still running after the grace period, forcibly terminate as fallback
        for (&id, managed) in self.processes.iter_mut() {
            if let Some(mut child) = managed.child.take() {
                if let Some(pid) = child.id() {
                    self.pid_registry.deauthorize(pid);
                }
                tracing::warn!(
                    process = id.as_str(),
                    "Child process did not exit within grace period; forcibly terminating"
                );
                let _ = child.start_kill();
            }
            managed.status = ProcessStatus::Stopped;
        }

        tracing::info!("Supervisor shutdown completed.");
        Ok(())
    }

    pub fn status(&self, id: ComponentId) -> Option<ProcessStatus> {
        self.processes.get(&id).map(|p| p.status)
    }

    pub fn spec(&self, id: ComponentId) -> Option<&ProcessSpec> {
        self.processes.get(&id).map(|p| &p.spec)
    }

    pub fn job_object(&self) -> &JobObject {
        &self.job_object
    }

    /// Consumes the supervisor and returns the underlying JobObject.
    pub fn into_job_object(self) -> JobObject {
        self.job_object
    }

    /// Queries the OS via the Win32 `IsProcessInJob` API to verify whether the specified
    /// managed child process is currently assigned to this supervisor's Windows Job Object.
    pub fn is_child_in_job(&self, id: ComponentId) -> Result<bool> {
        if let Some(managed) = self.processes.get(&id) {
            if let Some(ref child) = managed.child {
                #[cfg(windows)]
                if let Some(raw_h) = child.raw_handle() {
                    return self.job_object.contains_process_handle(raw_h as _);
                }
                #[cfg(not(windows))]
                return Ok(true);
            }
        }
        Ok(false)
    }
}

pub fn crate_name() -> &'static str {
    "process-supervisor"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crash_evaluation_immediate_restart() {
        let spec = ProcessSpec {
            id: ComponentId::Taskbar,
            executable: PathBuf::from("dummy.exe"),
            args: vec![],
            restart_policy: RestartPolicy::default(),
        };

        let mut child = ManagedChild::new(spec);
        let action = child.evaluate_crash(Some(1));
        assert_eq!(
            action,
            SupervisorAction::RestartingImmediately(ComponentId::Taskbar)
        );
        assert_eq!(child.consecutive_crashes, 1);
    }

    #[test]
    fn test_circuit_breaker_tripping() {
        let spec = ProcessSpec {
            id: ComponentId::SearchUi,
            executable: PathBuf::from("dummy.exe"),
            args: vec![],
            restart_policy: RestartPolicy {
                circuit_breaker_threshold: 3,
                ..Default::default()
            },
        };

        let mut child = ManagedChild::new(spec);
        let _ = child.evaluate_crash(Some(1)); // Crash 1
        let _ = child.evaluate_crash(Some(1)); // Crash 2
        let action = child.evaluate_crash(Some(1)); // Crash 3 -> Trips breaker

        assert_eq!(
            action,
            SupervisorAction::CircuitBroken(ComponentId::SearchUi)
        );
        assert_eq!(
            child.status,
            ProcessStatus::CircuitBroken {
                consecutive_crashes: 3
            }
        );
    }

    #[test]
    fn test_backoff_delay_calculation() {
        let spec = ProcessSpec {
            id: ComponentId::Desktop,
            executable: PathBuf::from("dummy.exe"),
            args: vec![],
            restart_policy: RestartPolicy {
                max_retries_in_window: 1,
                window_secs: 60,
                initial_backoff_ms: 500,
                max_backoff_ms: 10000,
                circuit_breaker_threshold: 10,
            },
        };

        let mut child = ManagedChild::new(spec);
        let _ = child.evaluate_crash(Some(1)); // Crash 1: within retry limit
        let action = child.evaluate_crash(Some(1)); // Crash 2: exceeds limit of 1 in window

        match action {
            SupervisorAction::BackoffDelay { id, delay_ms } => {
                assert_eq!(id, ComponentId::Desktop);
                assert!(delay_ms >= 500);
            }
            other => panic!("Expected BackoffDelay, got {:?}", other),
        }
    }

    #[test]
    fn test_supervised_pid_registry_lifecycle() {
        let registry = SupervisedPidRegistry::new(true);
        assert!(registry.is_empty());
        assert!(!registry.is_authorized(1234));

        registry.authorize(1234);
        assert_eq!(registry.len(), 1);
        assert!(registry.is_authorized(1234));
        assert!(!registry.is_authorized(5678));

        registry.authorize(5678);
        assert_eq!(registry.len(), 2);
        assert!(registry.is_authorized(5678));

        registry.deauthorize(1234);
        assert_eq!(registry.len(), 1);
        assert!(!registry.is_authorized(1234));
        assert!(registry.is_authorized(5678));

        let non_strict = SupervisedPidRegistry::new(false);
        assert!(non_strict.is_authorized(99999));

        // Test role authorization
        let role_reg = SupervisedPidRegistry::new(true);
        role_reg.authorize(100);
        assert_eq!(role_reg.get_role(100), Some(ClientRole::SupervisedChild));
        assert!(!role_reg.is_admin(100));

        role_reg.authorize_admin(200);
        assert_eq!(role_reg.get_role(200), Some(ClientRole::AdminController));
        assert!(role_reg.is_admin(200));
    }

    #[test]
    fn test_stable_runtime_resets_crash_counters() {
        let spec = ProcessSpec {
            id: ComponentId::Taskbar,
            executable: PathBuf::from("dummy.exe"),
            args: vec![],
            restart_policy: RestartPolicy {
                window_secs: 5,
                circuit_breaker_threshold: 3,
                ..Default::default()
            },
        };

        let mut child = ManagedChild::new(spec);
        let _ = child.evaluate_crash(Some(1));
        let _ = child.evaluate_crash(Some(1));
        assert_eq!(child.consecutive_crashes, 2);

        // Simulate child running stably for 6 seconds (> window_secs = 5)
        child.last_started_at = Some(Instant::now() - Duration::from_secs(6));
        let action = child.evaluate_crash(Some(1));
        assert_eq!(
            action,
            SupervisorAction::RestartingImmediately(ComponentId::Taskbar)
        );
        assert_eq!(
            child.consecutive_crashes, 1,
            "Crash counters must reset after stable runtime before counting new crash"
        );
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn test_spawn_rejects_duplicate_running_child() {
        let job = JobObject::create("AsterTestDupSpawnJob").expect("JobObject::create");
        let mut supervisor = ProcessSupervisor::new(job);
        supervisor.register(ProcessSpec {
            id: ComponentId::Taskbar,
            executable: PathBuf::from("cmd.exe"),
            args: vec!["/c".into(), "ping -n 6 127.0.0.1 >nul".into()],
            restart_policy: RestartPolicy::default(),
        });

        let pid1 = supervisor
            .spawn(ComponentId::Taskbar)
            .expect("Initial spawn must succeed");
        assert!(supervisor.is_supervised_pid(pid1));
        assert!(supervisor.is_child_in_job(ComponentId::Taskbar).unwrap());

        let dup = supervisor.spawn(ComponentId::Taskbar);
        assert!(
            dup.is_err(),
            "Spawning an already-running component must be rejected to avoid duplicates"
        );
        assert_eq!(supervisor.child_pid(ComponentId::Taskbar), Some(pid1));

        supervisor
            .shutdown_all(Duration::from_millis(200))
            .await
            .expect("shutdown_all");
        assert!(!supervisor.is_supervised_pid(pid1));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn test_poll_and_recover_restarts_and_trips_circuit_breaker() {
        let job = JobObject::create("AsterTestRecoverJob").expect("JobObject::create");
        let mut supervisor = ProcessSupervisor::new(job);
        supervisor.register(ProcessSpec {
            id: ComponentId::Taskbar,
            executable: PathBuf::from("cmd.exe"),
            args: vec!["/c".into(), "exit /b 1".into()],
            restart_policy: RestartPolicy {
                max_retries_in_window: 2,
                window_secs: 10,
                initial_backoff_ms: 50,
                max_backoff_ms: 100,
                circuit_breaker_threshold: 2,
            },
        });

        let pid1 = supervisor
            .spawn(ComponentId::Taskbar)
            .expect("Initial spawn must succeed");
        assert!(supervisor.is_supervised_pid(pid1));

        // Wait for cmd.exe /c exit /b 1 to terminate
        tokio::time::sleep(Duration::from_millis(150)).await;

        let actions1 = supervisor.poll_and_recover().await;
        assert_eq!(
            actions1,
            vec![SupervisorAction::RestartingImmediately(
                ComponentId::Taskbar
            )]
        );
        assert!(
            !supervisor.is_supervised_pid(pid1),
            "Stale PID must be deauthorized after exit"
        );

        let pid2 = supervisor
            .child_pid(ComponentId::Taskbar)
            .expect("Replacement child must be spawned");
        assert_ne!(pid1, pid2);
        assert!(
            supervisor.is_supervised_pid(pid2),
            "Replacement PID must be authorized"
        );

        // Wait for second crash -> should trip circuit breaker (threshold = 2)
        tokio::time::sleep(Duration::from_millis(150)).await;
        let actions2 = supervisor.poll_and_recover().await;
        assert_eq!(
            actions2,
            vec![SupervisorAction::CircuitBroken(ComponentId::Taskbar)]
        );
        assert!(!supervisor.is_supervised_pid(pid2));
        assert!(supervisor.child_pid(ComponentId::Taskbar).is_none());
        assert_eq!(
            supervisor.status(ComponentId::Taskbar),
            Some(ProcessStatus::CircuitBroken {
                consecutive_crashes: 2
            })
        );
    }
}
