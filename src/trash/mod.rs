use chrono::Local;
use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const TRASH_PATH_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'`')
    .add(b'{')
    .add(b'}');

#[derive(Debug, Clone)]
pub struct TrashDir {
    root: PathBuf,
}

impl TrashDir {
    pub fn system() -> Self {
        let root = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from(".local/share"))
            .join("Trash");
        Self { root }
    }

    pub fn with_root(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn files_dir(&self) -> PathBuf {
        self.root.join("files")
    }

    pub fn info_dir(&self) -> PathBuf {
        self.root.join("info")
    }

    pub fn item_count(&self) -> usize {
        fs::read_dir(self.files_dir())
            .map(|rd| rd.flatten().count())
            .unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.item_count() == 0
    }

    pub fn move_to_trash(&self, source: &Path) -> io::Result<PathBuf> {
        if !source.exists() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("source does not exist: {}", source.display()),
            ));
        }

        let files_dir = self.files_dir();
        let info_dir = self.info_dir();
        fs::create_dir_all(&files_dir)?;
        fs::create_dir_all(&info_dir)?;

        let file_name = source
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid file name"))?;

        let unique_name = self.next_unique_name(&files_dir, &info_dir, file_name);
        let dest_file = files_dir.join(&unique_name);
        let info_file = info_dir.join(format!("{unique_name}.trashinfo"));

        let abs_source = fs::canonicalize(source).unwrap_or_else(|_| source.to_path_buf());
        let encoded_path = utf8_percent_encode(
            abs_source.to_string_lossy().as_ref(),
            TRASH_PATH_ENCODE_SET,
        )
        .to_string();
        let deletion_date = Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
        let trashinfo = format!("[Trash Info]\nPath={encoded_path}\nDeletionDate={deletion_date}\n");
        fs::write(&info_file, trashinfo)?;

        if let Err(_rename_err) = fs::rename(source, &dest_file) {
            if source.is_dir() {
                copy_dir_recursive(source, &dest_file)?;
                fs::remove_dir_all(source)?;
            } else {
                fs::copy(source, &dest_file)?;
                fs::remove_file(source)?;
            }
        }

        Ok(dest_file)
    }

    pub fn empty_trash(&self) -> io::Result<()> {
        for dir in [self.files_dir(), self.info_dir()] {
            if !dir.exists() {
                continue;
            }
            for entry in fs::read_dir(&dir)?.flatten() {
                let p = entry.path();
                if p.is_dir() && !p.is_symlink() {
                    fs::remove_dir_all(p)?;
                } else {
                    fs::remove_file(p)?;
                }
            }
        }
        Ok(())
    }

    pub fn open_trash() -> io::Result<()> {
        Command::new("xdg-open")
            .arg("trash:///")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        Ok(())
    }

    fn next_unique_name(&self, files_dir: &Path, info_dir: &Path, original: &str) -> String {
        let candidate_free = |name: &str| {
            !files_dir.join(name).exists() && !info_dir.join(format!("{name}.trashinfo")).exists()
        };
        if candidate_free(original) {
            return original.to_string();
        }
        let path = Path::new(original);
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(original);
        let ext = path.extension().and_then(|s| s.to_str());
        for idx in 2..10_000 {
            let candidate = match ext {
                Some(e) => format!("{stem}.{idx}.{e}"),
                None => format!("{stem}.{idx}"),
            };
            if candidate_free(&candidate) {
                return candidate;
            }
        }
        format!("{original}.{}", Local::now().timestamp())
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let target = dst.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn move_collision_percent_encode_and_empty() {
        let tmp = tempdir().unwrap();
        let trash = TrashDir::with_root(tmp.path().join("Trash"));
        assert!(trash.is_empty());

        let f1 = tmp.path().join("my file.txt");
        fs::write(&f1, "hello").unwrap();
        let dest1 = trash.move_to_trash(&f1).unwrap();
        assert!(!f1.exists());
        assert!(dest1.exists());
        assert_eq!(trash.item_count(), 1);

        let info1 =
            fs::read_to_string(tmp.path().join("Trash/info/my file.txt.trashinfo")).unwrap();
        assert!(info1.contains("[Trash Info]"));
        assert!(info1.contains("my%20file.txt"));
        assert!(info1.contains("DeletionDate="));

        fs::write(&f1, "world").unwrap();
        let dest2 = trash.move_to_trash(&f1).unwrap();
        assert_ne!(dest1, dest2);
        assert_eq!(trash.item_count(), 2);

        trash.empty_trash().unwrap();
        assert!(trash.is_empty());
    }

    #[test]
    fn missing_source_returns_error() {
        let tmp = tempdir().unwrap();
        let trash = TrashDir::with_root(tmp.path().join("Trash"));
        assert!(trash.move_to_trash(&tmp.path().join("nope.txt")).is_err());
    }
}
