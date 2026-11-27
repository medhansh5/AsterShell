//! AsterShell Platform Layer — Windows API and System Abstractions

pub mod hotkey;
pub mod monitor;
pub mod window;

pub use hotkey::{register_global_hotkey, GlobalHotkeyHandle, HotkeyConfig, HotkeyError};
pub use monitor::{
    calculate_monitor_taskbar_coordinates, enumerate_monitors, find_monitor_by_target,
    find_primary_monitor, install_display_change_subclass, reposition_taskbar_on_monitor,
    MonitorInfo,
};
pub use window::{
    apply_dark_mode_and_backdrop, apply_native_taskbar_configuration,
    calculate_spotlight_coordinates, calculate_spotlight_coordinates_with_dpi,
    calculate_taskbar_coordinates_with_dpi, configure_taskbar_native_styles,
    get_active_monitor_work_area, get_primary_monitor_work_area, get_window_dpi, get_window_styles,
    position_taskbar_window, position_window, scale_to_physical_pixels, DwmBackdropType, Rect,
    WS_CAPTION_FLAG, WS_CLIPCHILDREN_FLAG, WS_CLIPSIBLINGS_FLAG, WS_EX_APPWINDOW_FLAG,
    WS_EX_NOACTIVATE_FLAG, WS_EX_TOOLWINDOW_FLAG, WS_EX_TOPMOST_FLAG, WS_POPUP_FLAG,
    WS_SYSMENU_FLAG, WS_THICKFRAME_FLAG, WS_VISIBLE_FLAG,
};

use astercore::{AsterError, Result};
use std::fmt;

#[cfg(windows)]
use windows::Win32::Foundation::{CloseHandle, LocalFree, HANDLE, HLOCAL, INVALID_HANDLE_VALUE};

/// Safe RAII wrapper around a Windows Win32 Job Object.
pub struct JobObject {
    #[cfg(windows)]
    handle: HANDLE,
    name: String,
}

impl JobObject {
    /// Creates a new Win32 Job Object configured with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.
    pub fn create(name: &str) -> Result<Self> {
        #[cfg(windows)]
        {
            use windows::core::HSTRING;
            use windows::Win32::System::JobObjects::{
                CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
                JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            };

            let h_name = HSTRING::from(name);
            // SAFETY: Calling CreateJobObjectW with optional security attributes and a valid name.
            let handle = unsafe { CreateJobObjectW(None, &h_name) }.map_err(|e| {
                AsterError::Platform(format!("Failed to create Job Object '{}': {}", name, e))
            })?;

            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

            // SAFETY: SetInformationJobObject with valid JOBOBJECT_EXTENDED_LIMIT_INFORMATION struct.
            let res = unsafe {
                SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as _,
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                )
            };

            if let Err(e) = res {
                unsafe {
                    let _ = CloseHandle(handle);
                }
                return Err(AsterError::Platform(format!(
                    "Failed to set kill-on-close limit on Job Object: {}",
                    e
                )));
            }

            tracing::info!(
                job_name = name,
                "Windows Job Object created and configured successfully"
            );
            Ok(Self {
                handle,
                name: name.to_string(),
            })
        }

        #[cfg(not(windows))]
        {
            Ok(Self {
                name: name.to_string(),
            })
        }
    }

    /// Assigns a running process to this Job Object.
    #[cfg(windows)]
    pub fn assign_process(&self, process_handle: HANDLE) -> Result<()> {
        use windows::Win32::System::JobObjects::AssignProcessToJobObject;

        // SAFETY: AssignProcessToJobObject with verified valid job and process handles.
        unsafe {
            AssignProcessToJobObject(self.handle, process_handle).map_err(|e| {
                AsterError::Platform(format!(
                    "Failed to assign process to Job Object '{}': {}",
                    self.name, e
                ))
            })?;
        }

        tracing::debug!(
            job_name = self.name.as_str(),
            "Process assigned to Job Object"
        );
        Ok(())
    }

    /// Assigns a process by its raw OS handle pointer.
    pub fn assign_raw_handle(&self, raw_handle: *mut std::ffi::c_void) -> Result<()> {
        #[cfg(windows)]
        {
            use windows::Win32::Foundation::HANDLE;
            self.assign_process(HANDLE(raw_handle as _))
        }
        #[cfg(not(windows))]
        {
            let _ = raw_handle;
            Ok(())
        }
    }

    /// Assigns the current process to this Job Object.
    pub fn assign_current_process(&self) -> Result<()> {
        #[cfg(windows)]
        {
            use windows::Win32::System::Threading::GetCurrentProcess;
            // SAFETY: GetCurrentProcess returns a pseudo-handle for the current process.
            let cur_proc = unsafe { GetCurrentProcess() };
            self.assign_process(cur_proc)
        }

        #[cfg(not(windows))]
        Ok(())
    }

    /// Checks whether the specified process is assigned to this Job Object using the Win32 `IsProcessInJob` API.
    #[cfg(windows)]
    pub fn is_process_in_job(&self, process_handle: HANDLE) -> Result<bool> {
        use windows::Win32::Foundation::BOOL;
        use windows::Win32::System::JobObjects::IsProcessInJob;

        let mut in_job = BOOL(0);
        // SAFETY: Calling IsProcessInJob with valid process and job handles.
        let res = unsafe { IsProcessInJob(process_handle, self.handle, &mut in_job) };
        if let Err(e) = res {
            return Err(AsterError::Platform(format!(
                "IsProcessInJob query failed for Job Object '{}': {}",
                self.name, e
            )));
        }
        Ok(in_job.as_bool())
    }

    /// Checks whether a process identified by raw handle pointer is assigned to this Job Object.
    pub fn contains_process_handle(&self, raw_handle: *mut std::ffi::c_void) -> Result<bool> {
        #[cfg(windows)]
        {
            use windows::Win32::Foundation::HANDLE;
            self.is_process_in_job(HANDLE(raw_handle as _))
        }
        #[cfg(not(windows))]
        {
            let _ = raw_handle;
            Ok(true)
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Queries the OS via the Win32 `QueryInformationJobObject` API to verify whether
    /// the `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` flag is active on the kernel Job Object.
    #[cfg(windows)]
    pub fn query_kill_on_close(&self) -> Result<bool> {
        use windows::Win32::System::JobObjects::{
            JobObjectExtendedLimitInformation, QueryInformationJobObject,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };

        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        let mut return_length = 0u32;
        // SAFETY: Calling QueryInformationJobObject with valid job handle and correctly sized buffer.
        let res = unsafe {
            QueryInformationJobObject(
                self.handle,
                JobObjectExtendedLimitInformation,
                &mut info as *mut _ as _,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                Some(&mut return_length),
            )
        };
        res.map_err(|e| AsterError::Platform(format!("Failed to query Job Object limits: {}", e)))?;
        let flags = info.BasicLimitInformation.LimitFlags;
        Ok((flags.0 & JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE.0) != 0)
    }

    #[cfg(not(windows))]
    pub fn query_kill_on_close(&self) -> Result<bool> {
        Ok(true)
    }
}

impl Drop for JobObject {
    fn drop(&mut self) {
        #[cfg(windows)]
        if !self.handle.is_invalid() && self.handle != INVALID_HANDLE_VALUE {
            // SAFETY: Cleanly closing valid job handle.
            unsafe {
                let _ = CloseHandle(self.handle);
            }
            tracing::debug!(job_name = self.name.as_str(), "Job Object handle closed");
        }
    }
}

// SAFETY: Windows kernel Job Object HANDLEs are safe to transfer and reference across threads.
unsafe impl Send for JobObject {}
unsafe impl Sync for JobObject {}

impl fmt::Debug for JobObject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JobObject")
            .field("name", &self.name)
            .finish()
    }
}

/// Retrieves the current user's SID string representation.
pub fn get_current_user_sid() -> Result<String> {
    #[cfg(windows)]
    {
        use windows::core::PWSTR;
        use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
        use windows::Win32::Security::{GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER};
        use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

        // SAFETY: Opening token for the current process with TOKEN_QUERY rights.
        unsafe {
            let mut token_handle = HANDLE::default();
            OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token_handle)
                .map_err(|e| AsterError::Platform(format!("OpenProcessToken failed: {}", e)))?;

            let mut req_len = 0u32;
            let _ = GetTokenInformation(token_handle, TokenUser, None, 0, &mut req_len);

            let mut buffer = vec![0u8; req_len as usize];
            let res = GetTokenInformation(
                token_handle,
                TokenUser,
                Some(buffer.as_mut_ptr() as _),
                req_len,
                &mut req_len,
            );

            let _ = CloseHandle(token_handle);
            res.map_err(|e| AsterError::Platform(format!("GetTokenInformation failed: {}", e)))?;

            let token_user = &*(buffer.as_ptr() as *const TOKEN_USER);
            let mut str_ptr = PWSTR::null();
            ConvertSidToStringSidW(token_user.User.Sid, &mut str_ptr).map_err(|e| {
                AsterError::Platform(format!("ConvertSidToStringSidW failed: {}", e))
            })?;

            let sid_str = str_ptr
                .to_string()
                .map_err(|e| AsterError::Platform(format!("Invalid SID UTF-16: {}", e)))?;

            let _ = LocalFree(HLOCAL(str_ptr.as_ptr() as _));
            Ok(sid_str)
        }
    }

    #[cfg(not(windows))]
    Ok("S-1-5-21-0000000000-0000000000-0000000000-1000".to_string())
}

/// Builds a standard user-restricted named pipe address.
pub fn default_pipe_address() -> String {
    let sid = get_current_user_sid().unwrap_or_else(|_| "default".into());
    let hash = format!(
        "{:x}",
        sid.bytes().fold(0u64, |acc, b| acc.wrapping_add(b as u64))
    );
    format!(r"\\.\pipe\astershell-ipc-{}", hash)
}

/// Generates the standard user-restricted Named Pipe SDDL string.
pub fn default_pipe_sddl() -> Result<String> {
    let sid = get_current_user_sid()?;
    Ok(format!("D:(A;;GRGW;;;{})(A;;GA;;;BA)", sid))
}

/// RAII wrapper for Windows SECURITY_ATTRIBUTES created from SDDL for Named Pipes.
pub struct PipeSecurityDescriptor {
    #[cfg(windows)]
    sd: windows::Win32::Security::PSECURITY_DESCRIPTOR,
    #[cfg(windows)]
    sa: windows::Win32::Security::SECURITY_ATTRIBUTES,
    sddl: String,
}

impl PipeSecurityDescriptor {
    /// Creates a new security descriptor from an SDDL string.
    pub fn from_sddl(sddl: &str) -> Result<Self> {
        #[cfg(windows)]
        {
            use windows::core::HSTRING;
            use windows::Win32::Security::Authorization::{
                ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
            };
            use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};

            let h_sddl = HSTRING::from(sddl);
            let mut sd = PSECURITY_DESCRIPTOR::default();
            // SAFETY: ConvertStringSecurityDescriptorToSecurityDescriptorW parses the SDDL and allocates *sd.
            unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    &h_sddl,
                    SDDL_REVISION_1,
                    &mut sd,
                    None,
                )
                .map_err(|e| {
                    AsterError::Platform(format!("Failed to parse SDDL '{}': {}", sddl, e))
                })?;
            }

            let sa = SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: sd.0,
                bInheritHandle: windows::Win32::Foundation::BOOL(0),
            };

            Ok(Self {
                sd,
                sa,
                sddl: sddl.to_string(),
            })
        }

        #[cfg(not(windows))]
        {
            Ok(Self {
                sddl: sddl.to_string(),
            })
        }
    }

    /// Returns a raw pointer to the `SECURITY_ATTRIBUTES` structure.
    pub fn as_raw_mut_ptr(&mut self) -> *mut std::ffi::c_void {
        #[cfg(windows)]
        {
            &mut self.sa as *mut _ as *mut std::ffi::c_void
        }
        #[cfg(not(windows))]
        {
            std::ptr::null_mut()
        }
    }

    pub fn sddl(&self) -> &str {
        &self.sddl
    }
}

impl Drop for PipeSecurityDescriptor {
    fn drop(&mut self) {
        #[cfg(windows)]
        {
            if !self.sd.0.is_null() {
                // SAFETY: Freeing memory allocated by ConvertStringSecurityDescriptorToSecurityDescriptorW.
                unsafe {
                    let _ = LocalFree(HLOCAL(self.sd.0));
                }
            }
        }
    }
}

// SAFETY: Security descriptor is self-contained.
unsafe impl Send for PipeSecurityDescriptor {}
unsafe impl Sync for PipeSecurityDescriptor {}

/// Queries the client process ID connected to a named pipe handle.
#[cfg(windows)]
pub fn get_named_pipe_client_pid(pipe_handle: HANDLE) -> Result<u32> {
    use windows::Win32::System::Pipes::GetNamedPipeClientProcessId;

    let mut client_pid = 0u32;
    // SAFETY: GetNamedPipeClientProcessId takes a valid pipe handle and writes client PID to pointer.
    unsafe {
        GetNamedPipeClientProcessId(pipe_handle, &mut client_pid).map_err(|e| {
            AsterError::Platform(format!("Failed to query named pipe client PID: {}", e))
        })?;
    }
    Ok(client_pid)
}

/// Safe wrapper to query client process ID from any type implementing `AsRawHandle`.
#[cfg(windows)]
pub fn get_pipe_client_pid<H: std::os::windows::io::AsRawHandle>(pipe: &H) -> Result<u32> {
    let raw = pipe.as_raw_handle();
    get_named_pipe_client_pid(HANDLE(raw as _))
}

#[cfg(not(windows))]
pub fn get_pipe_client_pid<H>(_pipe: &H) -> Result<u32> {
    Ok(0)
}

pub fn crate_name() -> &'static str {
    "platform-windows"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_manager_com_initialization() {
        use windows::core::HSTRING;
        use windows::Win32::System::Com::{
            CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
        };
        use windows::Win32::System::Search::{CSearchManager, ISearchManager};

        // SAFETY: Initializing COM in MTA mode on this thread for platform test.
        unsafe {
            let hr = CoInitializeEx(None, COINIT_MULTITHREADED);
            assert!(hr.is_ok(), "CoInitializeEx should succeed");

            let search_mgr: std::result::Result<ISearchManager, windows::core::Error> =
                CoCreateInstance(&CSearchManager, None, CLSCTX_ALL);

            if let Ok(mgr) = search_mgr {
                let cat_name = HSTRING::from("SystemIndex");
                let catalog_res = mgr.GetCatalog(&cat_name);
                assert!(
                    catalog_res.is_ok(),
                    "GetCatalog(SystemIndex) should succeed"
                );

                let catalog = catalog_res.unwrap();
                let helper_res = catalog.GetQueryHelper();
                assert!(helper_res.is_ok(), "GetQueryHelper should succeed");

                let helper = helper_res.unwrap();
                let conn_str = helper.ConnectionString().expect("ConnectionString");
                let _ = LocalFree(HLOCAL(conn_str.as_ptr() as _));

                let query_hstr = HSTRING::from("test");
                if let Ok(sql_pwstr) = helper.GenerateSQLFromUserQuery(&query_hstr) {
                    let _ = LocalFree(HLOCAL(sql_pwstr.as_ptr() as _));
                }
            }

            CoUninitialize();
        }
    }

    #[test]
    fn test_job_object_lifecycle() {
        let job = JobObject::create("AsterShellTestJob").expect("Failed to create JobObject");
        assert_eq!(job.name(), "AsterShellTestJob");
        let kill_on_close = job
            .query_kill_on_close()
            .expect("query_kill_on_close must succeed");
        assert!(
            kill_on_close,
            "JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE must be active on created job object"
        );
    }

    #[test]
    fn test_current_user_sid_retrieval() {
        let sid = get_current_user_sid();
        assert!(sid.is_ok());
        let sid_str = sid.unwrap();
        assert!(sid_str.starts_with("S-1-"));
    }

    #[test]
    fn test_pipe_address_format() {
        let pipe_addr = default_pipe_address();
        assert!(pipe_addr.starts_with(r"\\.\pipe\astershell-ipc-"));
    }

    #[test]
    fn test_default_pipe_sddl_and_security_descriptor() {
        let sddl = default_pipe_sddl().expect("Should compute default pipe SDDL");
        assert!(sddl.starts_with("D:(A;;GRGW;;;"));
        assert!(sddl.ends_with("(A;;GA;;;BA)"));

        let mut desc = PipeSecurityDescriptor::from_sddl(&sddl)
            .expect("Should construct PipeSecurityDescriptor from valid SDDL");
        assert!(!desc.as_raw_mut_ptr().is_null());
        assert_eq!(desc.sddl(), sddl);
    }

    #[test]
    fn test_hotkey_config_defaults() {
        let config = HotkeyConfig::alt_space();
        assert!(config.alt);
        assert!(!config.ctrl);
        assert!(!config.shift);
        assert!(!config.win);
        assert_eq!(config.vk_code, 0x20);
        assert!(config.no_repeat);
    }

    #[test]
    fn test_dpi_scaling_and_coordinates() {
        // Standard 100% DPI (96)
        assert_eq!(scale_to_physical_pixels(640, 96), 640);
        assert_eq!(scale_to_physical_pixels(480, 96), 480);

        // 125% DPI (120)
        assert_eq!(scale_to_physical_pixels(640, 120), 800);
        assert_eq!(scale_to_physical_pixels(480, 120), 600);

        // 150% DPI (144)
        assert_eq!(scale_to_physical_pixels(640, 144), 960);
        assert_eq!(scale_to_physical_pixels(480, 144), 720);

        // Coordinates with 125% DPI
        let res = calculate_spotlight_coordinates_with_dpi(640, 480, 120).unwrap();
        assert_eq!(res.2, 800);
        assert_eq!(res.3, 600);
    }

    #[test]
    fn test_active_monitor_work_area_and_coordinates() {
        let work_res = get_active_monitor_work_area();
        assert!(
            work_res.is_ok(),
            "Should retrieve active monitor work area: {:?}",
            work_res.err()
        );
        let work = work_res.unwrap();
        assert!(work.width() > 0);
        assert!(work.height() > 0);

        let coords_res = calculate_spotlight_coordinates(640, 480);
        assert!(coords_res.is_ok());
        let (x, y) = coords_res.unwrap();
        assert!(x >= work.left);
        assert!(y >= work.top);
    }

    #[test]
    fn test_hotkey_registration_and_clean_drop() {
        // Register a test hotkey (e.g. Ctrl + Alt + F12) to verify registration & unregistration loop
        let config = HotkeyConfig::new(0x7B /* VK_F12 */, true, true, false, false);
        let res = register_global_hotkey(config, || {});
        // In CI or test environments, hotkey registration succeeds or yields already registered
        match res {
            Ok(handle) => {
                assert!(handle.thread_id != 0);
                drop(handle); // Verify drop cleanly stops thread and unregisters
            }
            Err(HotkeyError::AlreadyRegistered) => {
                // Also acceptable if already taken in this environment
            }
            Err(e) => {
                panic!("Unexpected error registering hotkey: {:?}", e);
            }
        }
    }

    #[test]
    fn test_taskbar_style_flags() {
        assert_eq!(WS_POPUP_FLAG, 0x8000_0000);
        assert_eq!(WS_VISIBLE_FLAG, 0x1000_0000);
        assert_eq!(WS_CLIPSIBLINGS_FLAG, 0x0400_0000);
        assert_eq!(WS_CLIPCHILDREN_FLAG, 0x0200_0000);
        assert_eq!(WS_EX_TOPMOST_FLAG, 0x0000_0008);
        assert_eq!(WS_EX_TOOLWINDOW_FLAG, 0x0000_0080);
        assert_eq!(WS_EX_NOACTIVATE_FLAG, 0x0800_0000);
        assert_eq!(WS_EX_APPWINDOW_FLAG, 0x0004_0000);
    }

    #[test]
    fn test_primary_monitor_work_area() {
        let work_res = get_primary_monitor_work_area();
        assert!(
            work_res.is_ok(),
            "Should retrieve primary monitor work area: {:?}",
            work_res.err()
        );
        let work = work_res.unwrap();
        assert!(work.width() > 0, "Work area width must be positive");
        assert!(work.height() > 0, "Work area height must be positive");
    }

    #[test]
    fn test_taskbar_coordinates_signed_math() {
        // Test with real primary monitor
        let coords_res = calculate_taskbar_coordinates_with_dpi(720, 48, 96);
        assert!(coords_res.is_ok());
        let (x, y, w, h) = coords_res.unwrap();
        assert_eq!(w, 720);
        assert_eq!(h, 48);
        let work = get_primary_monitor_work_area().unwrap();
        assert_eq!(y, work.top, "Taskbar Y must match work.top");
        assert_eq!(
            x,
            work.left + (work.width() - 720) / 2,
            "Taskbar X must be horizontally centered"
        );

        // Test DPI scaling (150% = 144 DPI)
        let coords_144 = calculate_taskbar_coordinates_with_dpi(720, 48, 144).unwrap();
        assert_eq!(coords_144.2, 1080); // 720 * 144 / 96 = 1080
        assert_eq!(coords_144.3, 72); // 48 * 144 / 96 = 72

        // Verify signed negative coordinate math manually
        let negative_work = Rect {
            left: -1920,
            top: -1080,
            right: 0,
            bottom: 0,
        };
        assert_eq!(negative_work.width(), 1920);
        assert_eq!(negative_work.height(), 1080);
        let phys_w = scale_to_physical_pixels(720, 96);
        let calc_x = negative_work.left + (negative_work.width() - phys_w) / 2;
        let calc_y = negative_work.top;
        assert_eq!(calc_x, -1920 + (1920 - 720) / 2); // -1320
        assert_eq!(calc_y, -1080);
    }
}
