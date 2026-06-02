use narada_agent_tui::provider_process_tree::{ProviderProcess, ProviderProcessTerminationKind};
use std::fs::{create_dir_all, remove_dir_all, write};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn temp_dir(name: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock works")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "narada-agent-tui-{name}-{}-{unique}",
        std::process::id()
    ))
}

#[cfg(windows)]
#[test]
fn windows_provider_process_termination_kills_descendant_processes() {
    let dir = temp_dir("provider-process-tree");
    create_dir_all(&dir).expect("fixture dir created");
    let started_path = dir.join("child-started.txt");
    let survived_path = dir.join("child-survived.txt");
    let child_path = dir.join("child.ps1");
    let parent_path = dir.join("provider-parent.cmd");

    write(
        &child_path,
        format!(
            "Set-Content -LiteralPath '{}' -Value started\r\nStart-Sleep -Seconds 2\r\nSet-Content -LiteralPath '{}' -Value survived\r\n",
            started_path.display(),
            survived_path.display()
        ),
    )
    .expect("child fixture written");
    write(
        &parent_path,
        "@echo off\r\nstart \"\" /b powershell.exe -NoProfile -ExecutionPolicy Bypass -File \"%~dp0child.ps1\"\r\nping -n 30 127.0.0.1 >nul\r\n",
    )
    .expect("parent fixture written");

    let mut process =
        ProviderProcess::spawn(&parent_path, &[], &dir).expect("provider process spawned");
    assert_eq!(
        process.termination_kind(),
        ProviderProcessTerminationKind::ProcessTree
    );

    for _ in 0..60 {
        if started_path.exists() {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    assert!(started_path.exists(), "descendant fixture process started");

    process.terminate_tree();
    let _ = process.wait();
    thread::sleep(Duration::from_secs(3));

    assert!(
        !survived_path.exists(),
        "descendant process survived provider tree termination"
    );
    remove_dir_all(dir).ok();
}
