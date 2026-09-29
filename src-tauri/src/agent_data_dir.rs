use std::path::{Path, PathBuf};

pub fn resolve(configured: Option<PathBuf>, app_data_dir: &Path) -> std::io::Result<PathBuf> {
    let directory = configured.ok_or_else(|| {
        std::io::Error::other("Agent builds require CONDUCTOR_DATA_DIR. Use pnpm agent:start.")
    })?;
    if !directory.is_absolute() {
        return Err(std::io::Error::other(
            "Agent data directory must be absolute.",
        ));
    }
    // Resolve symlinks before comparing, including aliases of the real database directory.
    let directory = directory.canonicalize()?;
    for name in ["com.yaramiri.conductor", "com.yaramiri.conductor.dev"] {
        let protected = app_data_dir.with_file_name(name);
        let protected = match protected.canonicalize() {
            Ok(path) => path,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => protected,
            Err(error) => return Err(error),
        };
        if directory.starts_with(protected) {
            return Err(std::io::Error::other(
                "Agent builds cannot use release or development data. Use pnpm agent:start.",
            ));
        }
    }
    Ok(directory)
}

#[cfg(test)]
mod tests {
    use super::resolve;
    use std::fs;
    use std::path::PathBuf;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("conductor-agent-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(root.join("com.yaramiri.conductor")).unwrap();
            fs::create_dir_all(root.join("com.yaramiri.conductor.dev")).unwrap();
            fs::create_dir_all(root.join("isolated")).unwrap();
            Self(root.canonicalize().unwrap())
        }
        fn app_dir(&self) -> PathBuf {
            self.0.join("com.yaramiri.conductor.agent")
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn agent_refuses_to_start_without_an_explicit_data_directory() {
        let fixture = Fixture::new();
        assert!(resolve(None, &fixture.app_dir()).is_err());
    }

    #[test]
    fn agent_rejects_release_and_development_data() {
        let fixture = Fixture::new();
        for name in ["com.yaramiri.conductor", "com.yaramiri.conductor.dev"] {
            let directory = fixture.0.join(name);
            assert!(resolve(Some(directory.clone()), &fixture.app_dir()).is_err());
            fs::create_dir(directory.join("nested")).unwrap();
            assert!(resolve(Some(directory.join("nested")), &fixture.app_dir()).is_err());
        }
    }

    #[cfg(unix)]
    #[test]
    fn agent_rejects_symlinks_into_real_data() {
        let fixture = Fixture::new();
        let link = fixture.0.join("alias");
        std::os::unix::fs::symlink(fixture.0.join("com.yaramiri.conductor"), &link).unwrap();
        assert!(resolve(Some(link), &fixture.app_dir()).is_err());
    }

    #[test]
    fn agent_accepts_an_existing_isolated_directory() {
        let fixture = Fixture::new();
        let isolated = fixture.0.join("isolated");
        assert_eq!(
            resolve(Some(isolated.clone()), &fixture.app_dir()).unwrap(),
            isolated
        );
    }
}
