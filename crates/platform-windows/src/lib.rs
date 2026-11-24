//! AsterShell Platform Layer — Windows API and System Abstractions

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

    pub fn name(&self) -> &str {
        &self.name
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

pub fn crate_name() -> &'static str {
    "platform-windows"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_object_lifecycle() {
        let job = JobObject::create("AsterShellTestJob").expect("Failed to create JobObject");
        assert_eq!(job.name(), "AsterShellTestJob");
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
}
