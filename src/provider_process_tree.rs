use std::io;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderProcessTerminationKind {
    ProcessTree,
    DirectChild,
}

pub struct ProviderProcess {
    child: Child,
    #[cfg(windows)]
    job: Option<WindowsKillOnCloseJob>,
    termination_kind: ProviderProcessTerminationKind,
}

impl ProviderProcess {
    pub fn spawn(
        command: impl AsRef<Path>,
        args: &[String],
        cwd: &Path,
    ) -> io::Result<ProviderProcess> {
        let mut command_builder = Command::new(command.as_ref());
        command_builder
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            unsafe {
                command_builder.pre_exec(|| {
                    if libc::setsid() == -1 {
                        return Err(io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
        }

        let child = command_builder.spawn()?;

        #[cfg(windows)]
        {
            match WindowsKillOnCloseJob::create_and_assign(&child) {
                Ok(job) => Ok(ProviderProcess {
                    child,
                    job: Some(job),
                    termination_kind: ProviderProcessTerminationKind::ProcessTree,
                }),
                Err(error) => {
                    let mut child = child;
                    let _ = child.kill();
                    let _ = child.wait();
                    Err(error)
                }
            }
        }

        #[cfg(not(windows))]
        {
            Ok(ProviderProcess {
                child,
                termination_kind: if cfg!(unix) {
                    ProviderProcessTerminationKind::ProcessTree
                } else {
                    ProviderProcessTerminationKind::DirectChild
                },
            })
        }
    }

    pub fn child_mut(&mut self) -> &mut Child {
        &mut self.child
    }

    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }

    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        self.child.wait()
    }

    pub fn termination_kind(&self) -> ProviderProcessTerminationKind {
        self.termination_kind
    }

    pub fn terminate_tree(&mut self) {
        #[cfg(windows)]
        {
            self.job.take();
            let _ = self.child.kill();
            return;
        }

        #[cfg(unix)]
        {
            let process_group_id = -(self.child.id() as i32);
            unsafe {
                libc::kill(process_group_id, libc::SIGKILL);
            }
            let _ = self.child.kill();
            return;
        }

        #[cfg(not(any(windows, unix)))]
        {
            let _ = self.child.kill();
        }
    }
}

#[cfg(windows)]
struct WindowsKillOnCloseJob {
    handle: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
impl WindowsKillOnCloseJob {
    fn create_and_assign(child: &Child) -> io::Result<Self> {
        use std::mem::{size_of, zeroed};
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
        use windows_sys::Win32::System::JobObjects::{
            AssignProcessToJobObject, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject,
        };

        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn CreateJobObjectW(
                lpjobattributes: *const std::ffi::c_void,
                lpname: *const u16,
            ) -> HANDLE;
        }

        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                return Err(io::Error::last_os_error());
            }

            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let set_result = SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &mut info as *mut _ as *mut _,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if set_result == 0 {
                CloseHandle(handle);
                return Err(io::Error::last_os_error());
            }

            let process_handle = child.as_raw_handle() as HANDLE;
            let assign_result = AssignProcessToJobObject(handle, process_handle);
            if assign_result == 0 {
                CloseHandle(handle);
                return Err(io::Error::last_os_error());
            }

            Ok(Self { handle })
        }
    }
}

#[cfg(windows)]
impl Drop for WindowsKillOnCloseJob {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.handle);
        }
    }
}
