use crate::app::{Result, RuntimeError};
#[cfg(target_os = "windows")]
use std::collections::HashMap;
#[cfg(target_os = "windows")]
use std::sync::Arc;
#[cfg(target_os = "windows")]
use tokio::sync::RwLock;

#[cfg(target_os = "windows")]
use windows::Win32::{
    Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0},
    System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject, TerminateJobObject,
    },
    System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SET_QUOTA, PROCESS_TERMINATE, PROCESS_SYNCHRONIZE},
};

#[cfg(target_os = "windows")]
pub struct JobHandle {
    handle: HANDLE,
    process: std::sync::Mutex<Option<HANDLE>>,
}

#[cfg(target_os = "windows")]
impl JobHandle {
    pub fn create() -> Result<Self> {
        unsafe {
            let handle = CreateJobObjectW(None, None).map_err(|error| {
                RuntimeError::Internal(format!("Failed to create job object: {}", error))
            })?;

            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
            .map_err(|error| {
                RuntimeError::Internal(format!("Failed to set job object information: {}", error))
            })?;

            Ok(Self { handle, process: std::sync::Mutex::new(None) })
        }
    }

    pub fn assign_process(&self, pid: u32) -> Result<()> {
        unsafe {
            let process_handle = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, false, pid)
                .map_err(|error| {
                    RuntimeError::Internal(format!("Failed to open process {}: {}", pid, error))
                })?;

            if let Err(error) = AssignProcessToJobObject(self.handle, process_handle) {
                let _ = CloseHandle(process_handle);
                return Err(RuntimeError::Internal(format!("Failed to assign process to job: {error}")));
            }
            *self.process.lock().expect("process handle lock") = Some(process_handle);
            Ok(())
        }
    }

    async fn wait_for_exit(self: &Arc<Self>, timeout: std::time::Duration) -> bool {
        let job = self.clone();
        tokio::task::spawn_blocking(move || {
            let process = *job.process.lock().expect("process handle lock");
            process.is_some_and(|process| unsafe { WaitForSingleObject(process, timeout.as_millis().min(u32::MAX as u128) as u32) == WAIT_OBJECT_0 })
        }).await.unwrap_or(false)
    }

    fn terminate(&self) -> Result<()> {
        unsafe { TerminateJobObject(self.handle, 1) }.map_err(|error| RuntimeError::Internal(format!("Cannot terminate owned browser job: {error}")))
    }
}

#[cfg(target_os = "windows")]
impl Drop for JobHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.handle);
            if let Some(process) = self.process.get_mut().ok().and_then(Option::take) { let _ = CloseHandle(process); }
        }
    }
}

#[cfg(target_os = "windows")]
unsafe impl Send for JobHandle {}
#[cfg(target_os = "windows")]
unsafe impl Sync for JobHandle {}

pub struct JobManager {
    #[cfg(target_os = "windows")]
    jobs: Arc<RwLock<HashMap<String, Arc<JobHandle>>>>,
    /// Linux/macOS：环境 → 浏览器主进程 pid（同时是进程组 id，见 launcher 的 process_group(0)）
    #[cfg(unix)]
    pids: tokio::sync::RwLock<std::collections::HashMap<String, i32>>,
}

impl JobManager {
    pub fn new() -> Self {
        Self {
            #[cfg(target_os = "windows")]
            jobs: Arc::new(RwLock::new(HashMap::new())),
            #[cfg(unix)]
            pids: tokio::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }

    #[cfg(target_os = "windows")]
    pub async fn create_and_assign(&self, env_uuid: &str, pid: u32) -> Result<()> {
        let job = JobHandle::create()?;
        job.assign_process(pid)?;

        let mut jobs = self.jobs.write().await;
        jobs.insert(env_uuid.to_string(), Arc::new(job));
        Ok(())
    }

    #[cfg(unix)]
    pub async fn create_and_assign(&self, env_uuid: &str, pid: u32) -> Result<()> {
        let pid = i32::try_from(pid)
            .map_err(|_| RuntimeError::Internal(format!("invalid pid {pid}")))?;
        self.pids.write().await.insert(env_uuid.to_string(), pid);
        Ok(())
    }

    /// 等浏览器自己退出（让它写完 Cookie/缓存），超时就结束整个进程组。
    /// 返回 None 表示没有记录这个环境的进程，Some(forced) 表示是否强制结束。
    #[cfg(unix)]
    pub async fn finish_process(&self, env_uuid: &str) -> Result<Option<bool>> {
        self.finish_process_with_timeout(env_uuid, std::time::Duration::from_secs(15)).await
    }

    #[cfg(unix)]
    async fn finish_process_with_timeout(&self, env_uuid: &str, grace: std::time::Duration) -> Result<Option<bool>> {
        let Some(pid) = self.pids.read().await.get(env_uuid).copied() else { return Ok(None) };
        let forced = !unix_process::wait_for_exit(pid, grace).await;
        if forced {
            unix_process::kill_group(pid);
            if !unix_process::wait_for_exit(pid, std::time::Duration::from_secs(30)).await {
                return Err(RuntimeError::Internal("Owned browser did not exit after termination".into()));
            }
        }
        let mut pids = self.pids.write().await;
        if pids.get(env_uuid) == Some(&pid) {
            pids.remove(env_uuid);
            Ok(Some(forced))
        } else {
            Ok(None)
        }
    }

    #[cfg(target_os = "windows")]
    pub async fn remove(&self, env_uuid: &str) {
        let mut jobs = self.jobs.write().await;
        jobs.remove(env_uuid);
    }

    /// Retain the job until the browser has really exited, allowing Chromium to
    /// flush its profile. The held process handle avoids PID-reuse races.
    #[cfg(target_os = "windows")]
    pub async fn finish_process(&self, env_uuid: &str) -> Result<Option<bool>> {
        // Chromium 关闭时最多等 10 秒让网络服务进程写完 Cookie/缓存并退出
        // （BrowserProcessIOThread::ProcessHostCleanUp）。宽限要比它长，浏览器才能自己走完退出流程，
        // 不在它放弃等待之前被强杀
        self.finish_process_with_timeout(env_uuid, std::time::Duration::from_secs(15)).await
    }

    #[cfg(target_os = "windows")]
    async fn finish_process_with_timeout(&self, env_uuid: &str, grace: std::time::Duration) -> Result<Option<bool>> {
        let job = self.jobs.read().await.get(env_uuid).cloned();
        let Some(job) = job else { return Ok(None) };
        let forced = !job.wait_for_exit(grace).await;
        if forced {
            job.terminate()?;
            // 强制结束后进程退出也可能被机器负载拖慢（编译、杀毒）：3 秒、15 秒都实测过误报失败
            // （满负载编译时强杀后约 17 秒才退出）。进程已经在被结束，这里只是多等一会儿
            if !job.wait_for_exit(std::time::Duration::from_secs(30)).await {
                return Err(RuntimeError::Internal("Owned browser did not exit after termination".into()));
            }
        }
        let mut jobs = self.jobs.write().await;
        if jobs.get(env_uuid).is_some_and(|current| Arc::ptr_eq(current, &job)) {
            jobs.remove(env_uuid);
            Ok(Some(forced))
        } else { Ok(None) }
    }

    #[cfg(unix)]
    pub async fn remove(&self, env_uuid: &str) {
        self.pids.write().await.remove(env_uuid);
    }

    #[cfg(target_os = "windows")]
    pub async fn clear_all(&self) {
        let mut jobs = self.jobs.write().await;
        jobs.clear();
    }

    /// 应用退出：Windows 靠 Job 对象 KILL_ON_JOB_CLOSE 带走浏览器，这里显式结束各进程组
    #[cfg(unix)]
    pub async fn clear_all(&self) {
        let pids: Vec<i32> = self.pids.write().await.drain().map(|(_, pid)| pid).collect();
        for pid in pids {
            unix_process::kill_group(pid);
        }
    }
}

#[cfg(unix)]
mod unix_process {
    /// 进程是否已退出。先 waitpid 回收（浏览器是我们的子进程，不回收会一直是僵尸，
    /// kill(pid, 0) 仍然成功）；已被别处回收（ECHILD）再用 kill(pid, 0) 判断。
    pub fn has_exited(pid: i32) -> bool {
        let mut status = 0;
        let reaped = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
        if reaped == pid {
            return true;
        }
        if reaped == 0 {
            return false;
        }
        unsafe { libc::kill(pid, 0) != 0 }
    }

    pub async fn wait_for_exit(pid: i32, timeout: std::time::Duration) -> bool {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if has_exited(pid) {
                return true;
            }
            if tokio::time::Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }

    pub fn kill_group(pid: i32) {
        unsafe {
            // 进程组 id = 浏览器 pid；组不存在时退回只杀主进程
            if libc::kill(-pid, libc::SIGKILL) != 0 {
                libc::kill(pid, libc::SIGKILL);
            }
        }
    }
}

#[cfg(all(test, unix))]
mod unix_tests {
    use super::*;

    fn spawn(script: &str) -> std::process::Child {
        use std::os::unix::process::CommandExt;
        std::process::Command::new("sh").arg("-c").arg(script).process_group(0).spawn().unwrap()
    }

    #[tokio::test]
    async fn a_browser_that_exits_by_itself_is_not_forced() {
        let manager = JobManager::new();
        let child = spawn("sleep 0.2");
        manager.create_and_assign("env", child.id()).await.unwrap();
        assert_eq!(manager.finish_process("env").await.unwrap(), Some(false));
        assert!(manager.pids.read().await.is_empty());
    }

    #[tokio::test]
    async fn a_hung_browser_is_killed_with_its_process_group() {
        let manager = JobManager::new();
        let child = spawn("sleep 30 & sleep 30");
        manager.create_and_assign("env", child.id()).await.unwrap();
        let result = manager
            .finish_process_with_timeout("env", std::time::Duration::from_millis(50))
            .await;
        assert_eq!(result.unwrap(), Some(true));
        assert_eq!(manager.finish_process("env").await.unwrap(), None);
    }
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    fn child(script: &str) -> std::process::Child {
        let shell = std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap())
            .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        Command::new(shell).args(["-NoProfile", "-NonInteractive", "-Command", script])
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
            .creation_flags(0x08000000).spawn().unwrap()
    }

    #[tokio::test]
    async fn graceful_stop_retains_the_job_until_owned_process_flushes_and_exits() {
        let file = std::env::temp_dir().join(format!("mz-flush-{}", rand::random::<u64>()));
        let script = format!("Start-Sleep -Milliseconds 200; Set-Content -LiteralPath '{}' -Value flushed", file.to_string_lossy().replace('\'', "''"));
        let mut child = child(&script);
        let manager = JobManager::new();
        manager.create_and_assign("test-owned", child.id()).await.unwrap();
        let result = manager.finish_process("test-owned").await;
        let _ = child.kill(); let _ = child.wait();
        assert_eq!(result.unwrap(), Some(false));
        assert!(std::fs::read_to_string(&file).unwrap().contains("flushed"));
        std::fs::remove_file(file).unwrap();
    }

    #[tokio::test]
    async fn timeout_terminates_only_the_owned_job_and_reports_forced_shutdown() {
        let mut child = child("Start-Sleep -Seconds 30");
        let manager = JobManager::new();
        manager.create_and_assign("test-owned", child.id()).await.unwrap();
        let result = manager.finish_process_with_timeout("test-owned", std::time::Duration::from_millis(10)).await;
        let _ = child.kill(); let _ = child.wait();
        assert_eq!(result.unwrap(), Some(true));
        assert!(manager.jobs.read().await.is_empty());
    }
}

impl Default for JobManager {
    fn default() -> Self {
        Self::new()
    }
}
