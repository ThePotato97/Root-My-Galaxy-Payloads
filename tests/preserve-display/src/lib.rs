//! Compile the actual restart source on Linux with Android service/property
//! adapters replaced. This validates orchestration, not Android HAL behavior.
#![allow(dead_code)]
extern crate self as prop_rs_android;
use anyhow::{Result, bail};
use std::{fs, path::PathBuf};
fn root() -> PathBuf { PathBuf::from(std::env::var_os("RESTART_TEST_ROOT").unwrap()) }
fn record(message: &str) {
    use std::io::Write;
    writeln!(fs::OpenOptions::new().create(true).append(true).open(root().join("events")).unwrap(), "{message}").unwrap();
}
pub mod resetprop {
    use super::*;
    pub struct ResetProp {
        pub skip_svc: bool, pub persistent: bool, pub persist_only: bool,
        pub verbose: bool, pub show_context: bool, pub rebuild: bool,
    }
    impl ResetProp {
        pub fn set(&self, name: &str, value: &str) -> Result<()> {
            let fail = root().join(format!("fail-set-{name}"));
            if fail.exists() { fs::remove_file(fail)?; bail!("injected property failure"); }
            record(&format!("set {name} {value}"));
            fs::write(root().join(name), value)?; Ok(())
        }
        pub fn wait(&self, name: &str, _old: Option<&str>, _timeout: Option<()>) -> Result<()> {
            anyhow::ensure!(fs::read_to_string(root().join(name))? == "1", "boot not completed"); Ok(())
        }
    }
}
pub mod sys_prop { pub fn init() -> anyhow::Result<()> { Ok(()) } }
mod assets { pub fn get_asset_data(_: &str) -> Option<Vec<u8>> { None } }
mod defs {
    use std::time::Duration;
    pub const EMULATED_SOFT_REBOOT_TIMEOUT: Duration = Duration::from_secs(1);
    pub const WAITSYS_READY_TIMEOUT: Duration = Duration::from_secs(1);
    pub const WAITSYS_STOP_TIMEOUT: Duration = Duration::from_secs(1);
}
mod module { pub enum ScriptWait { Until(std::time::Instant) } }
mod ksucalls { pub fn ensure_uapi_version_matched() -> anyhow::Result<()> { Ok(()) } }
mod utils {
    pub fn switch_mnt_ns(_: i32) -> anyhow::Result<()> { Ok(()) }
    pub fn daemonize_with<F: FnOnce() -> anyhow::Result<()>>(_: bool, f: F) -> anyhow::Result<()> { f() }
}
mod init_event {
    use super::*;
    pub fn run_stage(stage: &str, _: crate::module::ScriptWait) { record(stage); }
    pub fn on_post_fs_data() -> Result<()> {
        record("post-fs-data");
        if root().join("fail-post-fs-data").exists() { bail!("injected mount failure"); } Ok(())
    }
    pub fn on_services() { record("services"); }
    pub fn on_boot_completed() { record("boot-completed"); }
}
mod actual {
    include!(env!("KSUD_RESTART_SOURCE"));
    #[cfg(test)]
    mod orchestration_tests {
        use super::*;
        use std::{fs, os::unix::fs::PermissionsExt};
        struct Device { _dir: tempfile::TempDir, old_path: std::ffi::OsString }
        impl Device {
            fn new() -> Self {
                let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
                let old_path = std::env::var_os("PATH").unwrap();
                unsafe {
                    std::env::set_var("RESTART_TEST_ROOT", dir.path());
                    std::env::set_var("PATH", format!("{}:{}", dir.path().display(), old_path.to_string_lossy()));
                }
                for (name, value) in [("sys.boot_completed","1"), ("init.svc.zygote","running"),
                    ("init.svc.zygote_secondary","running"), ("init.svc.surfaceflinger","running"),
                    ("crashrecovery.rescue_boot_count","4"), ("sys.rescue_boot_count","4"),
                    ("persist.sys.rescue_level","2")] { fs::write(dir.path().join(name),value).unwrap(); }
                let scripts = [
                    ("getprop", "#!/bin/sh\ncat \"$RESTART_TEST_ROOT/$1\" 2>/dev/null || true\n"),
                    ("stop", "#!/bin/sh\necho \"stop $1\" >> \"$RESTART_TEST_ROOT/events\"\nprintf stopped > \"$RESTART_TEST_ROOT/init.svc.$1\"\n"),
                    ("start", "#!/bin/sh\necho \"start $1\" >> \"$RESTART_TEST_ROOT/events\"\nif [ -f \"$RESTART_TEST_ROOT/fail-start-$1\" ]; then rm \"$RESTART_TEST_ROOT/fail-start-$1\"; exit 1; fi\nprintf running > \"$RESTART_TEST_ROOT/init.svc.$1\"\nif [ \"$1\" = zygote ]; then printf 1 > \"$RESTART_TEST_ROOT/sys.boot_completed\"; fi\n")];
                for (name, body) in scripts {
                    let p=dir.path().join(name); fs::write(&p,body).unwrap(); fs::set_permissions(p,fs::Permissions::from_mode(0o755)).unwrap();
                }
                Self {_dir:dir, old_path}
            }
            fn value(&self, name: &str) -> String { fs::read_to_string(crate::root().join(name)).unwrap() }
            fn set(&self, name: &str, value: &str) { fs::write(crate::root().join(name),value).unwrap(); }
            fn events(&self) -> String { fs::read_to_string(crate::root().join("events")).unwrap_or_default() }
        }
        impl Drop for Device { fn drop(&mut self) { unsafe { std::env::set_var("PATH", &self.old_path); std::env::remove_var("RESTART_TEST_ROOT"); } } }
        #[test]
        fn restart_keeps_display_services_and_retains_failure_history() {
            let d=Device::new(); preserve_display_reboot().unwrap();
            let events=d.events();
            assert!(events.contains("stop zygote_secondary\nstop zygote\npost-fs-data"));
            assert!(events.contains("start zygote\nstart zygote_secondary"));
            assert!(events.ends_with("services\nboot-completed\n"));
            assert!(!events.contains("surfaceflinger")); assert!(!events.contains("composer"));
            assert_eq!(d.value("init.svc.surfaceflinger"),"running");
            assert_eq!(d.value("crashrecovery.rescue_boot_count"),"3");
            assert_eq!(d.value("sys.rescue_boot_count"),"3");
            assert_eq!(d.value("persist.sys.rescue_level"),"2");
        }
        #[test]
        fn malformed_counter_is_rejected_before_teardown() {
            let d=Device::new(); d.set("sys.rescue_boot_count","bad");
            assert!(preserve_display_reboot().is_err()); assert_eq!(d.events(),"");
        }
        #[test]
        fn incomplete_boot_is_rejected_before_teardown() {
            let d=Device::new(); d.set("sys.boot_completed","0");
            assert!(preserve_display_reboot().is_err()); assert_eq!(d.events(),"");
        }
        #[test]
        fn failed_mount_attempts_framework_recovery() {
            let d=Device::new(); d.set("fail-post-fs-data","");
            assert!(preserve_display_reboot().is_err());
            assert_eq!(d.value("init.svc.zygote"),"running");
            assert_eq!(d.value("init.svc.zygote_secondary"),"running");
            assert!(!d.events().contains("boot-completed"));
        }
        #[test]
        fn absent_alias_is_not_created() {
            let _d=Device::new(); fs::remove_file(crate::root().join("sys.rescue_boot_count")).unwrap();
            preserve_display_reboot().unwrap(); assert!(!crate::root().join("sys.rescue_boot_count").exists());
        }
        #[test]
        fn failed_property_adjustment_attempts_framework_recovery() {
            let d=Device::new(); d.set("fail-set-sys.rescue_boot_count","");
            assert!(preserve_display_reboot().is_err());
            assert_eq!(d.value("init.svc.zygote"),"running");
            assert_eq!(d.value("init.svc.zygote_secondary"),"running");
        }
        #[test]
        fn failed_primary_start_is_retried() {
            let d=Device::new(); d.set("fail-start-zygote","");
            assert!(preserve_display_reboot().is_err());
            assert_eq!(d.value("init.svc.zygote"),"running");
            assert_eq!(d.value("init.svc.zygote_secondary"),"running");
        }
    }
}
