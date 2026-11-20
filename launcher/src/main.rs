use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{self, Command};

const APP_NAME: &str = if cfg!(windows) { "opendrop-app.exe" } else { "opendrop-app" };

/// Path of the app binary that sits next to the real launcher file.
/// `exe` may be a symlink (the winget alias); the loader looks for DLLs next
/// to the symlink, so the app has to be started from its own folder.
fn app_path(exe: &Path) -> io::Result<PathBuf> {
    let real = exe.canonicalize()?;
    let dir = real
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "launcher has no parent folder"))?;
    Ok(dir.join(APP_NAME))
}

fn main() {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let result = std::env::current_exe()
        .and_then(|exe| app_path(&exe))
        .and_then(|app| Command::new(app).args(args).status());
    match result {
        Ok(status) => process::exit(status.code().unwrap_or(1)),
        Err(err) => {
            eprintln!("opendrop-vj: cannot start {APP_NAME}: {err}");
            process::exit(1);
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::symlink;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("opendrop-launcher-{name}-{}", process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir.canonicalize().unwrap()
    }

    #[test]
    fn finds_the_app_next_to_the_real_file_when_started_through_a_symlink() {
        let root = temp_dir("symlink");
        let real = root.join("package");
        let links = root.join("links");
        fs::create_dir_all(&real).unwrap();
        fs::create_dir_all(&links).unwrap();
        fs::write(real.join("opendrop-vj"), b"").unwrap();
        symlink(real.join("opendrop-vj"), links.join("opendrop-vj")).unwrap();

        let app = app_path(&links.join("opendrop-vj")).unwrap();

        assert_eq!(app, real.join(APP_NAME));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn finds_the_app_next_to_a_plain_file() {
        let root = temp_dir("plain");
        fs::write(root.join("opendrop-vj"), b"").unwrap();

        assert_eq!(app_path(&root.join("opendrop-vj")).unwrap(), root.join(APP_NAME));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn reports_a_missing_launcher_file() {
        let root = temp_dir("missing");

        assert!(app_path(&root.join("nope")).is_err());
        fs::remove_dir_all(&root).unwrap();
    }
}
