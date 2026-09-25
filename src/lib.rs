#![doc = include_str!("../README.md")]

use std::{
    env,
    fs::File,
    io::Write,
    path::{Path, PathBuf},
};

use path_clean::PathClean;
use rand::{RngExt, distr::Alphanumeric, rng};

/// Represents a temporary directory.\
/// By default this temporary directory is deleted when this struct is dropped.
#[derive(Debug)]
pub struct TempDirectory {
    path: PathBuf,
    delete_on_drop: bool,
}

impl TempDirectory {
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Joins `path` to the root of the temporary directory.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use temp_dir_builder::TempDirectoryBuilder;
    /// let mut builder = TempDirectoryBuilder::default();
    /// builder.add_text_file("foo.txt", "bar");
    /// let temp_dir = builder.build().expect("create temp dir");
    /// let content = std::fs::read_to_string(temp_dir.join("foo.txt")).unwrap();
    /// assert_eq!(content, "bar");
    /// ```
    #[must_use]
    pub fn join(&self, path: impl AsRef<Path>) -> PathBuf {
        self.path.join(path)
    }

    /// Returns an owned copy of the root path.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use temp_dir_builder::TempDirectoryBuilder;
    /// let temp_dir = TempDirectoryBuilder::default()
    ///     .build()
    ///     .expect("create temp dir");
    /// assert_eq!(temp_dir.to_path_buf(), temp_dir.path());
    /// ```
    #[must_use]
    pub fn to_path_buf(&self) -> PathBuf {
        self.path.clone()
    }

    /// Resolves an `EntryKey` to the path of the entry it identifies.
    ///
    /// A directory only needed as the parent of another entry can still be
    /// declared explicitly to get a key for it, instead of naming it again
    /// with `join`:
    ///
    /// # Examples
    ///
    /// ```rust
    // <snip id="example-path-of">
    /// use temp_dir_builder::TempDirectoryBuilder;
    /// let mut builder = TempDirectoryBuilder::default();
    /// let repository = builder.add_directory("repository");
    /// builder.add_text_file("repository/.gitignore", "*.log");
    /// let temp_dir = builder.build().expect("create temp dir");
    /// let repository_path = temp_dir.path_of(&repository);
    /// assert_eq!(repository_path, temp_dir.join("repository"));
    // </snip>
    /// ```
    #[must_use]
    pub fn path_of(&self, key: &EntryKey) -> PathBuf {
        self.path.join(&key.path)
    }
}

impl AsRef<Path> for TempDirectory {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

/// Identifies an entry declared on a `TempDirectoryBuilder`.
///
/// Returned by the `add_*` methods. There is no other way to build one, so a
/// key always designates an entry that `build()` creates.
#[derive(Debug, Clone)]
pub struct EntryKey {
    index: usize,
    path: Box<Path>,
}

/// Error happening when creating the directory tree.
#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("Failed to create the root directory '{0}': {1}")]
    FailedToCreateRootDirectory(PathBuf, std::io::Error),
    #[error("Failed to create directory '{0}': {1}")]
    FailedToCreateDirectory(PathBuf, std::io::Error),
    #[error("Failed to delete directory '{0}': {1}")]
    FailedToDeleteDirectory(PathBuf, std::io::Error),
    #[error("Failed to create file '{0}': {1}")]
    FailedToCreateFile(PathBuf, std::io::Error),
    #[error("Failed to read source file '{0}': {1}")]
    FailedToCopyFile(PathBuf, std::io::Error),
    #[error("Failed to write file '{0}': {1}")]
    FailedToWriteFile(PathBuf, std::io::Error),
    #[error("The entry '{0}' is outside the temporary directory")]
    EntryOutsideDirectory(PathBuf),
    #[error("The entry {0} has an empty name")]
    EmptyEntryName(usize),
    #[error("The entry '{0}' is already existing")]
    DuplicateEntry(PathBuf),
    #[error("Failed to set permissions on '{0}': {1}")]
    FailedToSetPermissions(PathBuf, std::io::Error),
    #[error("Failed to create symlink '{0}': {1}")]
    FailedToCreateSymlink(PathBuf, std::io::Error),
    #[error("Cannot set permissions on symlink '{0}'")]
    PermissionsOnSymlink(PathBuf),
}

/// A temporary directory builder that contains a list of entries to be created.
///
/// # Examples
///
/// ```rust
// <snip id="example-builder">
/// use temp_dir_builder::TempDirectoryBuilder;
/// let mut builder = TempDirectoryBuilder::default();
/// builder.add_text_file("test/foo.txt", "bar");
/// builder.add_binary_file("test/foo2.txt", &[98u8, 97u8, 114u8]);
/// builder.add_empty_file("test/folder-a/folder-b/bar.txt");
/// builder.add_file("test/file.rs", file!());
/// builder.add_directory("test/dir");
/// let temp_dir = builder.build().expect("create temp dir");
/// println!("created successfully in {}", temp_dir.path().display());
// </snip>
/// ```
#[derive(Debug)]
pub struct TempDirectoryBuilder<'a> {
    /// Root folder where the tree will be created.
    root: Root,
    /// List of file metadata entries in the tree.
    entries: Vec<Entry<'a>>,
    /// Flag indicating whether the temporary directory created must be deleted when the instance is dropped.
    delete_on_drop: bool,
}

impl Default for TempDirectoryBuilder<'_> {
    /// Creates a default `TempDirectoryBuilder` instance with an empty file list,
    fn default() -> Self {
        Self {
            entries: vec![],
            root: Root::Random,
            delete_on_drop: true,
        }
    }
}

/// Root folder where the tree will be created.
#[derive(Debug)]
enum Root {
    /// A random temporary directory will be generated and atomically created during `build()`.
    Random,
    /// A random directory will be generated under a caller-provided base directory.
    RandomIn(PathBuf),
    /// A fixed, caller-provided directory.
    Fixed(PathBuf),
}

impl Drop for TempDirectory {
    fn drop(&mut self) {
        if self.delete_on_drop {
            make_deletable(&self.path);
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

impl<'a> TempDirectoryBuilder<'a> {
    /// Sets the root folder where the tree will be created.\
    /// By default this is the temporary directory path returned by `std::env::temp_dir()`.
    pub fn root_folder(&mut self, dir: impl AsRef<Path>) {
        self.root = Root::Fixed(dir.as_ref().to_path_buf());
    }

    /// Generates a random directory under `base` instead of under
    /// `std::env::temp_dir()`. `base` is created if it does not exist.
    ///
    /// Useful when `std::env::temp_dir()` is unsuitable for what is being
    /// tested, for example because it is excluded from backups and the test
    /// is checking backup exclusion.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use temp_dir_builder::TempDirectoryBuilder;
    /// let mut builder = TempDirectoryBuilder::default();
    /// builder.random_root_in(std::env::current_dir().unwrap());
    /// let temp_dir = builder.build().expect("create temp dir");
    /// assert!(temp_dir.path().starts_with(std::env::current_dir().unwrap()));
    /// ```
    pub fn random_root_in(&mut self, base: impl AsRef<Path>) {
        self.root = Root::RandomIn(base.as_ref().to_path_buf());
    }

    /// Specifies whether to automatically delete the temporary folder when the `TempDirectory` instance is dropped.\
    /// By default this is value is set to `true`.
    pub const fn delete_on_drop(&mut self, delete_on_drop: bool) {
        self.delete_on_drop = delete_on_drop;
    }

    /// Sets whether an entry is read-only.
    ///
    /// # Panics
    /// Panics if `key` was not returned by this builder.
    ///
    /// # Examples
    ///
    /// ```rust
    // <snip id="example-set-readonly">
    /// use temp_dir_builder::TempDirectoryBuilder;
    /// let mut builder = TempDirectoryBuilder::default();
    /// let foo = builder.add_text_file("test/foo.txt", "bar");
    /// builder.set_readonly(&foo, true);
    /// builder.add_directory("test/dir");
    /// let temp_dir = builder.build().expect("create temp dir");
    // </snip>
    /// ```
    pub fn set_readonly(&mut self, key: &EntryKey, readonly: bool) {
        self.entry_mut(key).readonly = Some(readonly);
    }

    /// Sets whether an entry is executable. On Unix this sets the owner
    /// execute bit; on other platforms it does nothing.
    ///
    /// # Panics
    /// Panics if `key` was not returned by this builder.
    ///
    /// # Examples
    ///
    /// ```rust
    // <snip id="example-set-executable">
    /// use temp_dir_builder::TempDirectoryBuilder;
    /// let mut builder = TempDirectoryBuilder::default();
    /// let script = builder.add_text_file("run.sh", "#!/bin/sh");
    /// builder.set_executable(&script, true);
    /// let temp_dir = builder.build().expect("create temp dir");
    // </snip>
    /// ```
    pub fn set_executable(&mut self, key: &EntryKey, executable: bool) {
        self.entry_mut(key).executable = Some(executable);
    }

    /// Sets the Unix permission bits of an entry, e.g. `0o744`.
    ///
    /// # Panics
    /// Panics if `key` was not returned by this builder.
    ///
    /// # Examples
    ///
    /// On Unix platforms, `set_mode` can be used to set the raw permission bits:
    ///
    /// ```rust
    // <snip id="example-set-mode">
    /// # #[cfg(unix)]
    /// # {
    /// use temp_dir_builder::TempDirectoryBuilder;
    /// let mut builder = TempDirectoryBuilder::default();
    /// let foo = builder.add_text_file("test/foo.txt", "bar");
    /// builder.set_mode(&foo, 0o744);
    /// builder.add_directory("test/dir");
    /// let temp_dir = builder.build().expect("create temp dir");
    /// # }
    // </snip>
    /// ```
    #[cfg(unix)]
    pub fn set_mode(&mut self, key: &EntryKey, mode: u32) {
        self.entry_mut(key).mode = Some(mode);
    }

    fn assert_owns(&self, key: &EntryKey) {
        assert!(
            self.entries
                .get(key.index)
                .is_some_and(|entry| entry.path.clean().as_path() == &*key.path),
            "EntryKey for '{}' does not belong to this builder",
            key.path.display()
        );
    }

    fn entry_mut(&mut self, key: &EntryKey) -> &mut Entry<'a> {
        self.assert_owns(key);
        &mut self.entries[key.index]
    }

    fn add(&mut self, path: impl AsRef<Path>, kind: Kind<'a>) -> EntryKey {
        let path = path.as_ref().to_path_buf();
        let key_path = path.clean().into_boxed_path();
        let index = self.entries.len();

        self.entries.push(Entry {
            path,
            kind,
            readonly: None,
            executable: None,
            #[cfg(unix)]
            mode: None,
        });

        EntryKey {
            index,
            path: key_path,
        }
    }

    /// Adds an empty file.
    /// * `path` - Path of the file to create. This path must be relative to the created directory. If the path is outside
    ///   the created directory (e.g: "../foo") the error `BuildError::EntryOutsideDirectory` will be returned.
    pub fn add_empty_file<P: AsRef<Path>>(&mut self, path: P) -> EntryKey {
        self.add(path, Kind::EmptyFile)
    }

    /// Adds a directory.
    /// * `path` - Path of the directory to create. This path must be relative to the created directory.
    ///   If the path is outside the created directory (e.g: "../foo") the error `BuildError::EntryOutsideDirectory` will be returned.
    pub fn add_directory(&mut self, path: impl AsRef<Path>) -> EntryKey {
        self.add(path, Kind::Directory)
    }

    /// Adds a text file specifying the content.
    /// * `path` - Path of the text file to create. This path must be relative to the created directory.
    ///   If the path is outside the created directory (e.g: "../foo") the error `BuildError::EntryOutsideDirectory` will be returned.
    /// * `text` - Text to be written in the new file created.
    pub fn add_text_file(&mut self, path: impl AsRef<Path>, text: impl Into<String>) -> EntryKey {
        self.add(path, Kind::TextFile(text.into()))
    }

    /// Adds a binary file specifying the content.
    /// * `path` - Path of the binary file to create. This path must be relative to the created directory.
    ///   If the path is outside the created directory (e.g: "../foo") the error `BuildError::EntryOutsideDirectory` will be returned.
    /// * `content` - The bytes to be written in the new file created.
    pub fn add_binary_file(&mut self, path: impl AsRef<Path>, content: &[u8]) -> EntryKey {
        self.add(path, Kind::BinaryFile(content.to_vec()))
    }

    /// Adds a text file whose content is computed from the root path of the
    /// temporary directory when `build()` runs.
    /// * `path` - Path of the text file to create. This path must be relative to the created directory.
    ///   If the path is outside the created directory (e.g: "../foo") the error `BuildError::EntryOutsideDirectory` will be returned.
    /// * `content` - Called with the root path of the temporary directory to produce the text to be written in the new file created.
    ///
    /// # Examples
    ///
    /// ```rust
    // <snip id="example-add-text-file-with">
    /// use temp_dir_builder::TempDirectoryBuilder;
    /// let mut builder = TempDirectoryBuilder::default();
    /// builder.add_text_file_with("config.toml", |root| {
    ///     format!("data_dir = {:?}", root.join("data"))
    /// });
    /// builder.add_directory("data");
    /// let temp_dir = builder.build().expect("create temp dir");
    // </snip>
    /// ```
    pub fn add_text_file_with<F>(&mut self, path: impl AsRef<Path>, content: F) -> EntryKey
    where
        F: Fn(&Path) -> String + 'a,
    {
        self.add(path, Kind::TextFileWith(Box::new(content)))
    }

    /// Adds a file specifying a source file to be copied.
    /// * `path` - Path of the file to create. This path must be relative to the created directory.
    ///   If the path is outside the created directory (e.g: "../foo") the error `BuildError::EntryOutsideDirectory` will be returned.
    /// * `file` - Path of the file to be copied. If relative, it is resolved against the current working directory.
    pub fn add_file(&mut self, path: impl AsRef<Path>, file: impl AsRef<Path>) -> EntryKey {
        self.add(path, Kind::FileToCopy(file.as_ref().to_path_buf()))
    }

    /// Adds a symbolic link.
    ///
    /// * `path` - Path of the link, relative to the root of the temporary directory.
    /// * `target` - Target of the link. A relative target is resolved against the
    ///   root of the temporary directory and written as an absolute path; an
    ///   absolute target is written verbatim. The target does not have to exist,
    ///   nor be inside the temporary directory. Use `add_relative_symlink` to
    ///   write the target verbatim instead, or `add_symlink_to` when the target
    ///   is an entry already declared on this builder.
    ///
    /// # Examples
    ///
    /// ```rust
    // <snip id="example-add-symlink">
    /// use temp_dir_builder::TempDirectoryBuilder;
    /// let mut builder = TempDirectoryBuilder::default();
    /// builder.add_text_file("data/file.txt", "content");
    /// builder.add_symlink("link_to_data", "data");
    /// builder.add_symlink("link_to_file", "data/file.txt");
    /// let temp_dir = builder.build().expect("create temp dir");
    // </snip>
    /// ```
    pub fn add_symlink(&mut self, path: impl AsRef<Path>, target: impl AsRef<Path>) -> EntryKey {
        self.add(path, Kind::Symlink(target.as_ref().to_path_buf()))
    }

    /// Adds a symbolic link targeting an entry already declared on this
    /// builder.
    ///
    /// Unlike `add_symlink`, `target` can only be an `EntryKey`, so a target
    /// that was renamed or removed is a compile error at the call site, not a
    /// silently dangling link. Use `add_symlink` instead when the target is
    /// meant to be dangling or to live outside the temporary directory.
    ///
    /// * `path` - Path of the link, relative to the root of the temporary directory.
    /// * `target` - Key of the entry to link to, as returned by an `add_*` method.
    ///
    /// # Panics
    /// Panics if `target` was not returned by this builder.
    ///
    /// # Examples
    ///
    /// ```rust
    // <snip id="example-add-symlink-to">
    /// use temp_dir_builder::TempDirectoryBuilder;
    /// let mut builder = TempDirectoryBuilder::default();
    /// let data = builder.add_directory("data");
    /// builder.add_symlink_to("link_to_data", &data);
    /// let temp_dir = builder.build().expect("create temp dir");
    // </snip>
    /// ```
    pub fn add_symlink_to(&mut self, path: impl AsRef<Path>, target: &EntryKey) -> EntryKey {
        self.assert_owns(target);
        self.add(path, Kind::Symlink(target.path.to_path_buf()))
    }

    /// Adds a symbolic link whose target is written verbatim, interpreted by
    /// the OS relative to the link's parent directory.
    ///
    /// * `path` - Path of the link, relative to the root of the temporary directory.
    /// * `target` - Target of the link, written as-is. The target does not have
    ///   to exist, nor be inside the temporary directory.
    ///
    /// # Examples
    ///
    /// ```rust
    // <snip id="example-add-relative-symlink">
    /// use temp_dir_builder::TempDirectoryBuilder;
    /// let mut builder = TempDirectoryBuilder::default();
    /// builder.add_text_file("data/file.txt", "content");
    /// builder.add_relative_symlink("dir/link", "../data");
    /// let temp_dir = builder.build().expect("create temp dir");
    // </snip>
    /// ```
    pub fn add_relative_symlink(
        &mut self,
        path: impl AsRef<Path>,
        target: impl AsRef<Path>,
    ) -> EntryKey {
        self.add(path, Kind::RelativeSymlink(target.as_ref().to_path_buf()))
    }

    /// Adds a symbolic link pointing at a target that doesn't exist yet,
    /// explicitly created as a directory link.
    ///
    /// Only needed when `target` is dangling: `add_symlink` inspects an
    /// existing target to pick file vs. directory automatically.
    ///
    /// # Errors
    /// Creating symlinks on Windows requires developer mode or elevated
    /// privileges.
    #[cfg(windows)]
    pub fn add_symlink_dir(
        &mut self,
        path: impl AsRef<Path>,
        target: impl AsRef<Path>,
    ) -> EntryKey {
        self.add(path, Kind::SymlinkDir(target.as_ref().to_path_buf()))
    }

    /// Adds a symbolic link pointing at a target that doesn't exist yet,
    /// explicitly created as a file link.
    ///
    /// Only needed when `target` is dangling: `add_symlink` inspects an
    /// existing target to pick file vs. directory automatically.
    ///
    /// # Errors
    /// Creating symlinks on Windows requires developer mode or elevated
    /// privileges.
    #[cfg(windows)]
    pub fn add_symlink_file(
        &mut self,
        path: impl AsRef<Path>,
        target: impl AsRef<Path>,
    ) -> EntryKey {
        self.add(path, Kind::SymlinkFile(target.as_ref().to_path_buf()))
    }

    /// Builds the file tree by generating files and directories based on the
    /// list of `Entry`s.
    ///
    /// The returned `TempDirectory::path()` is always canonical (symlinks
    /// resolved), so it can be compared directly with paths reported by the
    /// operating system.
    ///
    /// # Errors
    /// A `BuildError` is returned in case of error.
    pub fn build(&self) -> Result<TempDirectory, BuildError> {
        let root = match &self.root {
            Root::Fixed(root) => {
                create_or_validate_fixed_root(root)?;
                canonicalize(root)
                    .map_err(|err| BuildError::FailedToCreateRootDirectory(root.clone(), err))?
            }
            Root::Random => create_random_temp_directory(&env::temp_dir())?,
            Root::RandomIn(base) => create_random_temp_directory(base)?,
        };

        let mut created_paths = Vec::with_capacity(self.entries.len());

        for (entry_index, entry) in self.entries.iter().enumerate() {
            if entry.path.as_os_str().is_empty() {
                return Err(BuildError::EmptyEntryName(entry_index));
            }

            let entry_path = root.join(&entry.path).clean();

            if !entry_path.starts_with(&root) {
                return Err(BuildError::EntryOutsideDirectory(entry.path.clone()));
            }

            if std::fs::symlink_metadata(&entry_path).is_ok() {
                return Err(BuildError::DuplicateEntry(entry_path));
            }

            if let Some(parent_dir) = Path::new(&entry_path).parent() {
                std::fs::create_dir_all(parent_dir).map_err(|err| {
                    BuildError::FailedToCreateDirectory(parent_dir.to_path_buf(), err)
                })?;
            }

            create_entry(&root, &entry_path, &entry.kind)?;
            created_paths.push(entry_path);
        }

        for (entry, entry_path) in self.entries.iter().zip(created_paths) {
            apply_permissions(&entry_path, entry)?;
        }

        Ok(TempDirectory {
            path: root,
            delete_on_drop: self.delete_on_drop,
        })
    }
}

fn create_entry(root: &Path, entry_path: &Path, kind: &Kind<'_>) -> Result<(), BuildError> {
    match kind {
        Kind::Directory => {
            std::fs::create_dir(entry_path).map_err(|err| {
                BuildError::FailedToCreateDirectory(entry_path.to_path_buf(), err)
            })?;
        }
        Kind::EmptyFile => {
            File::create(entry_path)
                .map_err(|err| BuildError::FailedToCreateFile(entry_path.to_path_buf(), err))?;
        }
        Kind::TextFile(text) => {
            let mut new_file = File::create(entry_path)
                .map_err(|err| BuildError::FailedToCreateFile(entry_path.to_path_buf(), err))?;

            new_file
                .write_all(text.as_bytes())
                .map_err(|err| BuildError::FailedToWriteFile(entry_path.to_path_buf(), err))?;
        }
        Kind::TextFileWith(content) => {
            let text = content(root);
            let mut new_file = File::create(entry_path)
                .map_err(|err| BuildError::FailedToCreateFile(entry_path.to_path_buf(), err))?;

            new_file
                .write_all(text.as_bytes())
                .map_err(|err| BuildError::FailedToWriteFile(entry_path.to_path_buf(), err))?;
        }
        Kind::BinaryFile(bytes) => {
            let mut new_file = File::create(entry_path)
                .map_err(|err| BuildError::FailedToCreateFile(entry_path.to_path_buf(), err))?;

            new_file
                .write_all(bytes)
                .map_err(|err| BuildError::FailedToWriteFile(entry_path.to_path_buf(), err))?;
        }
        Kind::FileToCopy(source_path) => {
            std::fs::copy(source_path, entry_path)
                .map_err(|err| BuildError::FailedToCopyFile(source_path.clone(), err))?;
        }
        Kind::Symlink(target) => {
            let target = resolve_symlink_target(root, target);
            create_symlink(&target, entry_path)
                .map_err(|err| BuildError::FailedToCreateSymlink(entry_path.to_path_buf(), err))?;
        }
        Kind::RelativeSymlink(target) => {
            create_symlink(target, entry_path)
                .map_err(|err| BuildError::FailedToCreateSymlink(entry_path.to_path_buf(), err))?;
        }
        #[cfg(windows)]
        Kind::SymlinkDir(target) => {
            let target = windows_reparse_target(&resolve_symlink_target(root, target));
            std::os::windows::fs::symlink_dir(&target, entry_path)
                .map_err(|err| BuildError::FailedToCreateSymlink(entry_path.to_path_buf(), err))?;
        }
        #[cfg(windows)]
        Kind::SymlinkFile(target) => {
            let target = windows_reparse_target(&resolve_symlink_target(root, target));
            std::os::windows::fs::symlink_file(&target, entry_path)
                .map_err(|err| BuildError::FailedToCreateSymlink(entry_path.to_path_buf(), err))?;
        }
    }

    Ok(())
}

fn resolve_symlink_target(root: &Path, target: &Path) -> PathBuf {
    if target.is_absolute() {
        target.to_path_buf()
    } else {
        root.join(target).clean()
    }
}

#[cfg(unix)]
fn create_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn create_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    let resolved_target = link
        .parent()
        .map_or_else(|| target.to_path_buf(), |parent| parent.join(target));
    let target = windows_reparse_target(target);

    if resolved_target.is_dir() {
        std::os::windows::fs::symlink_dir(target, link)
    } else {
        std::os::windows::fs::symlink_file(target, link)
    }
}

/// Windows stores a symlink's target verbatim in the reparse point, and the
/// kernel only accepts `\` as a separator there (unlike regular path APIs,
/// which accept `/` too). Normalize before handing the target to
/// `symlink_dir`/`symlink_file`, otherwise forward slashes in the target
/// (e.g. from `add_relative_symlink("dir/link", "../data")`) make the link
/// unreadable by anything that actually traverses it.
#[cfg(windows)]
fn windows_reparse_target(target: &Path) -> PathBuf {
    PathBuf::from(target.to_string_lossy().replace('/', "\\"))
}

#[cfg(not(any(unix, windows)))]
fn create_symlink(_target: &Path, _link: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "symlinks are not supported on this platform",
    ))
}

#[cfg(windows)]
fn canonicalize(path: impl AsRef<Path>) -> std::io::Result<PathBuf> {
    dunce::canonicalize(path)
}

#[cfg(not(windows))]
fn canonicalize(path: impl AsRef<Path>) -> std::io::Result<PathBuf> {
    path.as_ref().canonicalize()
}

fn apply_permissions(entry_path: &Path, entry: &Entry<'_>) -> Result<(), BuildError> {
    #[cfg(unix)]
    let mode = entry.mode;
    #[cfg(not(unix))]
    let mode: Option<u32> = None;

    if mode.is_none() && entry.readonly.is_none() && entry.executable.is_none() {
        return Ok(());
    }

    if entry.kind.is_symlink() {
        return Err(BuildError::PermissionsOnSymlink(entry_path.to_path_buf()));
    }

    let mut permissions = std::fs::metadata(entry_path)
        .map_err(|err| BuildError::FailedToSetPermissions(entry_path.to_path_buf(), err))?
        .permissions();

    #[cfg(unix)]
    if let Some(mode) = mode {
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode(mode);
    }

    if let Some(readonly) = entry.readonly {
        set_readonly(&mut permissions, readonly);
    }

    if let Some(executable) = entry.executable {
        set_executable(&mut permissions, executable);
    }

    std::fs::set_permissions(entry_path, permissions)
        .map_err(|err| BuildError::FailedToSetPermissions(entry_path.to_path_buf(), err))
}

fn set_readonly(permissions: &mut std::fs::Permissions, readonly: bool) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = permissions.mode();
        permissions.set_mode(if readonly {
            mode & !0o222
        } else {
            mode | 0o200
        });
    }
    #[cfg(not(unix))]
    permissions.set_readonly(readonly);
}

#[cfg(unix)]
fn set_executable(permissions: &mut std::fs::Permissions, executable: bool) {
    use std::os::unix::fs::PermissionsExt;
    let mode = permissions.mode();
    permissions.set_mode(if executable {
        mode | 0o100
    } else {
        mode & !0o111
    });
}

#[cfg(not(unix))]
const fn set_executable(_permissions: &mut std::fs::Permissions, _executable: bool) {}

fn make_deletable(path: &Path) {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return;
    };
    let file_type = metadata.file_type();

    if file_type.is_symlink() {
        return;
    }

    let mut permissions = metadata.permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if !file_type.is_dir() {
            return;
        }
        permissions.set_mode(permissions.mode() | 0o700);
    }
    #[cfg(not(unix))]
    set_readonly(&mut permissions, false);

    let _ = std::fs::set_permissions(path, permissions);

    if file_type.is_dir() {
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                make_deletable(&entry.path());
            }
        }
    }
}

fn create_or_validate_fixed_root(root: &Path) -> Result<(), BuildError> {
    match std::fs::create_dir(root) {
        Ok(()) => return Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            if let Some(parent) = root.parent() {
                std::fs::create_dir_all(parent).map_err(|err| {
                    BuildError::FailedToCreateRootDirectory(root.to_path_buf(), err)
                })?;
            }

            match std::fs::create_dir(root) {
                Ok(()) => return Ok(()),
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(err) => {
                    return Err(BuildError::FailedToCreateRootDirectory(
                        root.to_path_buf(),
                        err,
                    ));
                }
            }
        }
        Err(err) => {
            return Err(BuildError::FailedToCreateRootDirectory(
                root.to_path_buf(),
                err,
            ));
        }
    }

    // Residual TOCTOU: root could be swapped for a symlink between this check and later use, see issue #16.
    match std::fs::symlink_metadata(root) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(BuildError::FailedToCreateRootDirectory(
                root.to_path_buf(),
                std::io::Error::new(std::io::ErrorKind::AlreadyExists, "root path is a symlink"),
            ))
        }
        Ok(metadata) if !metadata.is_dir() => Err(BuildError::FailedToCreateRootDirectory(
            root.to_path_buf(),
            std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "root path exists and is not a directory",
            ),
        )),
        Ok(_) => Ok(()),
        Err(err) => Err(BuildError::FailedToCreateRootDirectory(
            root.to_path_buf(),
            err,
        )),
    }
}

const MAX_RANDOM_DIRECTORY_ATTEMPTS: u32 = 100;

fn create_random_temp_directory(base: &Path) -> Result<PathBuf, BuildError> {
    std::fs::create_dir_all(base)
        .map_err(|err| BuildError::FailedToCreateRootDirectory(base.to_path_buf(), err))?;

    for _ in 0..MAX_RANDOM_DIRECTORY_ATTEMPTS {
        let random_string: String = rng()
            .sample_iter(&Alphanumeric)
            .take(5)
            .map(char::from)
            .collect();

        let path = base.join(random_string);

        match std::fs::create_dir(&path) {
            Ok(()) => {
                return canonicalize(&path)
                    .map_err(|err| BuildError::FailedToCreateRootDirectory(path, err));
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(err) => return Err(BuildError::FailedToCreateRootDirectory(path, err)),
        }
    }

    Err(BuildError::FailedToCreateRootDirectory(
        base.to_path_buf(),
        std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "exhausted attempts to generate an unused random directory name",
        ),
    ))
}

enum Kind<'a> {
    Directory,
    EmptyFile,
    TextFile(String),
    TextFileWith(Box<dyn Fn(&Path) -> String + 'a>),
    BinaryFile(Vec<u8>),
    FileToCopy(PathBuf),
    Symlink(PathBuf),
    RelativeSymlink(PathBuf),
    #[cfg(windows)]
    SymlinkDir(PathBuf),
    #[cfg(windows)]
    SymlinkFile(PathBuf),
}

impl std::fmt::Debug for Kind<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Directory => f.write_str("Directory"),
            Self::EmptyFile => f.write_str("EmptyFile"),
            Self::TextFile(text) => f.debug_tuple("TextFile").field(text).finish(),
            Self::TextFileWith(_) => f.write_str("TextFileWith(..)"),
            Self::BinaryFile(bytes) => f.debug_tuple("BinaryFile").field(bytes).finish(),
            Self::FileToCopy(path) => f.debug_tuple("FileToCopy").field(path).finish(),
            Self::Symlink(path) => f.debug_tuple("Symlink").field(path).finish(),
            Self::RelativeSymlink(path) => f.debug_tuple("RelativeSymlink").field(path).finish(),
            #[cfg(windows)]
            Self::SymlinkDir(path) => f.debug_tuple("SymlinkDir").field(path).finish(),
            #[cfg(windows)]
            Self::SymlinkFile(path) => f.debug_tuple("SymlinkFile").field(path).finish(),
        }
    }
}

impl Kind<'_> {
    const fn is_symlink(&self) -> bool {
        match self {
            Self::Symlink(_) | Self::RelativeSymlink(_) => true,
            #[cfg(windows)]
            Self::SymlinkDir(_) | Self::SymlinkFile(_) => true,
            _ => false,
        }
    }
}

/// Represents an entry, file or directory, to be created.
#[derive(Debug)]
struct Entry<'a> {
    /// Path of the entry relative to the root folder.
    path: PathBuf,
    /// The kind of the entry.
    kind: Kind<'a>,
    /// Whether the entry must be made read-only.
    readonly: Option<bool>,
    /// Whether the entry must be made executable.
    executable: Option<bool>,
    /// The Unix permission bits to apply to the entry.
    #[cfg(unix)]
    mode: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_join() {
        let temp_dir = TempDirectoryBuilder::default().build().unwrap();

        assert_eq!(temp_dir.join("a/b"), temp_dir.path().join("a/b"));
    }

    #[test]
    fn test_to_path_buf() {
        let temp_dir = TempDirectoryBuilder::default().build().unwrap();

        assert_eq!(temp_dir.to_path_buf(), temp_dir.path());
    }

    #[test]
    fn test_as_ref_path() {
        fn accepts_path(path: impl AsRef<Path>) -> PathBuf {
            path.as_ref().to_path_buf()
        }

        let temp_dir = TempDirectoryBuilder::default().build().unwrap();

        assert_eq!(accepts_path(&temp_dir), temp_dir.path());
    }

    #[test]
    fn test_temp_dir() {
        let temp_dir = TempDirectoryBuilder::default().build().unwrap();

        assert!(temp_dir.path().exists());
        assert!(temp_dir.path().is_dir());
    }

    #[test]
    fn test_random_root_is_canonical() {
        let mut builder = TempDirectoryBuilder::default();
        builder.add_empty_file("foo");
        let temp_dir = builder.build().unwrap();

        assert_eq!(temp_dir.path(), canonicalize(&temp_dir).unwrap());
        assert!(temp_dir.path().join("foo").exists());
    }

    #[test]
    fn test_random_root_in_uses_custom_base() {
        let base = std::env::temp_dir().join(format!(
            "test_random_root_in_uses_custom_base_{}",
            std::process::id()
        ));
        let mut builder = TempDirectoryBuilder::default();
        builder.random_root_in(&base);
        let temp_dir = builder.build().unwrap();

        assert!(temp_dir.path().starts_with(canonicalize(&base).unwrap()));
        assert_eq!(temp_dir.path(), canonicalize(&temp_dir).unwrap());

        drop(temp_dir);
        std::fs::remove_dir(&base).unwrap();
    }

    #[test]
    fn test_random_root_in_creates_base_if_missing() {
        let base = std::env::temp_dir().join(format!(
            "test_random_root_in_creates_base_if_missing_{}",
            std::process::id()
        ));
        assert!(!base.exists());

        let mut builder = TempDirectoryBuilder::default();
        builder.random_root_in(&base);
        let temp_dir = builder.build().unwrap();

        assert!(base.exists());
        assert!(temp_dir.path().exists());

        drop(temp_dir);
        std::fs::remove_dir(&base).unwrap();
    }

    #[test]
    fn test_root_folder_relative_path_is_canonical() {
        let dir_name = format!(
            "test_root_folder_relative_path_is_canonical_{}",
            std::process::id()
        );
        let mut builder = TempDirectoryBuilder::default();
        builder.root_folder(&dir_name);
        let temp_dir = builder.build().unwrap();

        assert!(temp_dir.path().is_absolute());
        assert_eq!(temp_dir.path(), canonicalize(&temp_dir).unwrap());
    }

    #[test]
    #[cfg(unix)]
    fn test_root_folder_under_symlinked_directory_is_canonical() {
        let real_base = TempDirectoryBuilder::default().build().unwrap();
        let link = std::env::temp_dir().join(format!(
            "test_root_folder_under_symlinked_directory_is_canonical_{}",
            std::process::id()
        ));

        std::os::unix::fs::symlink(real_base.path(), &link).unwrap();

        let root = link.join("child");
        let mut builder = TempDirectoryBuilder::default();
        builder.root_folder(&root);
        let temp_dir = builder.build().unwrap();

        assert_eq!(temp_dir.path(), &real_base.path().join("child"));

        std::fs::remove_file(&link).unwrap();
    }

    #[test]
    fn test_add_text_file() {
        let expected_content = "bar";
        let entry_name = "foo.txt";
        let mut builder = TempDirectoryBuilder::default();
        builder.add_text_file(entry_name, expected_content);
        let temp_dir = builder.build().unwrap();
        let entry_path = temp_dir.path().join(entry_name);

        assert!(entry_path.exists());

        let content = std::fs::read_to_string(entry_path).expect("read text in foo.txt");

        assert_eq!(content, expected_content);
    }

    #[test]
    fn test_add_text_file_with() {
        let entry_name = "foo.txt";
        let mut builder = TempDirectoryBuilder::default();
        builder.add_text_file_with(entry_name, |root| format!("root is {}", root.display()));
        let temp_dir = builder.build().unwrap();
        let entry_path = temp_dir.path().join(entry_name);

        let content = std::fs::read_to_string(entry_path).expect("read text in foo.txt");

        assert_eq!(content, format!("root is {}", temp_dir.path().display()));
    }

    #[test]
    fn test_add_text_file_with_borrowed_local() {
        let entry_name = "foo.txt";
        let template = String::from("root is");
        let mut builder = TempDirectoryBuilder::default();
        builder.add_text_file_with(entry_name, |root| format!("{template} {}", root.display()));
        let temp_dir = builder.build().unwrap();
        let entry_path = temp_dir.path().join(entry_name);

        let content = std::fs::read_to_string(entry_path).expect("read text in foo.txt");

        assert_eq!(content, format!("{template} {}", temp_dir.path().display()));
    }

    #[test]
    #[cfg(unix)]
    fn test_add_text_file_with_set_mode() {
        use std::os::unix::fs::PermissionsExt;

        let entry_name = "hook.sh";
        let mut builder = TempDirectoryBuilder::default();
        let hook = builder.add_text_file_with(entry_name, |root| {
            format!("#!/bin/sh\necho executed > {:?}\n", root.join("marker"))
        });
        builder.set_mode(&hook, 0o755);
        let temp_dir = builder.build().unwrap();
        let entry_path = temp_dir.path().join(entry_name);

        let mode = std::fs::metadata(&entry_path).unwrap().permissions().mode();

        assert_eq!(mode & 0o777, 0o755);
    }

    #[test]
    fn test_add_text_file_with_duplicate_entry() {
        let mut builder = TempDirectoryBuilder::default();
        builder.add_text_file_with("foo", |_| String::new());
        builder.add_text_file_with("foo", |_| String::new());
        let error = builder.build().unwrap_err();

        assert!(matches!(error, BuildError::DuplicateEntry(_)));
    }

    #[test]
    fn test_add_text_file_with_outside_directory() {
        let mut builder = TempDirectoryBuilder::default();
        builder.add_text_file_with("../foo", |_| String::new());
        let error = builder.build().unwrap_err();

        assert!(matches!(error, BuildError::EntryOutsideDirectory(_)));
    }

    #[test]
    fn test_add_binary_file() {
        let expected_content = [98u8, 97u8, 114u8];
        let entry_name = "foo.txt";
        let mut builder = TempDirectoryBuilder::default();
        builder.add_binary_file(entry_name, &expected_content);
        let temp_dir = builder.build().unwrap();
        let entry_path = temp_dir.path().join(entry_name);

        assert!(entry_path.exists());

        let content = std::fs::read(entry_path).expect("read foo.txt");

        assert_eq!(content, expected_content);
    }

    #[test]
    fn test_add_empty_file() {
        let entry_name = "empty_file.txt";
        let mut builder = TempDirectoryBuilder::default();
        builder.add_empty_file(entry_name);
        let temp_dir = builder.build().unwrap();
        let entry_path = temp_dir.path().join(entry_name);

        assert!(entry_path.exists());

        let created_entry_metadata = std::fs::metadata(entry_path).expect("get entry metadata");

        assert_eq!(created_entry_metadata.len(), 0);
    }

    #[test]
    fn test_add_directory() {
        let entry_name = "empty_directory";
        let mut builder = TempDirectoryBuilder::default();
        builder.add_directory(entry_name);
        let temp_dir = builder.build().unwrap();
        let entry_path = temp_dir.path().join(entry_name);

        assert!(entry_path.exists());
        assert!(entry_path.is_dir());
    }

    #[test]
    fn test_add_file() {
        let entry_name = "test.rs";
        let source_file_path = file!();
        let mut builder = TempDirectoryBuilder::default();
        builder.add_file(entry_name, source_file_path);
        let temp_dir = builder.build().unwrap();
        let entry_path = temp_dir.path().join(entry_name);

        assert!(entry_path.exists());
        assert!(entry_path.is_file());

        let entry_content = std::fs::read_to_string(entry_path).unwrap();
        let source_content = std::fs::read_to_string(source_file_path).unwrap();

        assert_eq!(entry_content, source_content);
    }

    #[test]
    fn test_temp_dir_is_dropped() {
        let temp_dir = TempDirectoryBuilder::default().build().unwrap();

        let temp_dir_path = temp_dir.path().to_path_buf();

        assert!(temp_dir_path.exists());
        assert!(temp_dir_path.is_dir());

        drop(temp_dir);

        assert!(!temp_dir_path.exists());
    }

    #[test]
    fn test_entry_outside_temp_dir() {
        let path_outside_temp_dir = std::env::temp_dir().join("outside");
        let mut builder = TempDirectoryBuilder::default();
        builder.add_empty_file(path_outside_temp_dir);
        let error = builder.build().unwrap_err();

        assert!(matches!(error, BuildError::EntryOutsideDirectory(_)));
    }

    #[test]
    fn test_source_file_does_not_exists() {
        let source_file_path = std::env::temp_dir().join("not existing file");
        let mut builder = TempDirectoryBuilder::default();
        builder.add_file("foo", source_file_path);
        let error = builder.build().unwrap_err();

        assert!(matches!(error, BuildError::FailedToCopyFile(..)));
    }

    #[test]
    fn test_duplicated_entries() {
        let mut builder = TempDirectoryBuilder::default();
        builder.add_empty_file("foo");
        builder.add_empty_file("foo");
        let error = builder.build().unwrap_err();

        assert!(matches!(error, BuildError::DuplicateEntry(..)));
    }

    #[test]
    fn test_entry_outside_directory() {
        let mut builder = TempDirectoryBuilder::default();
        builder.add_empty_file("../foo");
        let error = builder.build().unwrap_err();

        assert!(matches!(error, BuildError::EntryOutsideDirectory(..)));
    }

    #[test]
    fn test_empty_entry_name() {
        let mut builder = TempDirectoryBuilder::default();
        builder.add_empty_file("");
        let error = builder.build().unwrap_err();

        assert!(matches!(error, BuildError::EmptyEntryName(0)));
    }

    #[test]
    fn test_set_readonly_file() {
        let mut builder = TempDirectoryBuilder::default();
        let readonly = builder.add_text_file("readonly.txt", "foo");
        builder.set_readonly(&readonly, true);
        let writable = builder.add_text_file("writable.txt", "bar");
        builder.set_readonly(&writable, false);
        builder.add_text_file("default.txt", "baz");
        let temp_dir = builder.build().unwrap();

        let readonly_path = temp_dir.path().join("readonly.txt");
        let writable_path = temp_dir.path().join("writable.txt");
        let default_path = temp_dir.path().join("default.txt");

        assert!(
            std::fs::metadata(&readonly_path)
                .unwrap()
                .permissions()
                .readonly()
        );
        assert!(
            !std::fs::metadata(&writable_path)
                .unwrap()
                .permissions()
                .readonly()
        );
        assert!(
            !std::fs::metadata(&default_path)
                .unwrap()
                .permissions()
                .readonly()
        );
        assert!(std::fs::write(&readonly_path, "changed").is_err());
        assert!(std::fs::write(&writable_path, "changed").is_ok());
    }

    #[test]
    fn test_set_readonly_directory() {
        let mut builder = TempDirectoryBuilder::default();
        let dir = builder.add_directory("dir");
        builder.set_readonly(&dir, true);
        builder.add_text_file("dir/foo.txt", "foo");
        let temp_dir = builder.build().unwrap();

        let dir_path = temp_dir.path().join("dir");

        assert!(
            std::fs::metadata(&dir_path)
                .unwrap()
                .permissions()
                .readonly()
        );
        assert_eq!(
            std::fs::read_to_string(dir_path.join("foo.txt")).unwrap(),
            "foo"
        );
    }

    #[test]
    fn test_readonly_entries_are_deleted_on_drop() {
        let mut builder = TempDirectoryBuilder::default();
        let dir = builder.add_directory("dir");
        builder.set_readonly(&dir, true);
        let file = builder.add_text_file("dir/foo.txt", "foo");
        builder.set_readonly(&file, true);
        let nested = builder.add_directory("dir/nested");
        builder.set_readonly(&nested, true);
        let nested_file = builder.add_empty_file("dir/nested/bar.txt");
        builder.set_readonly(&nested_file, true);
        let temp_dir = builder.build().unwrap();
        let root = temp_dir.path().to_path_buf();

        drop(temp_dir);

        assert!(!root.exists());
    }

    #[test]
    #[cfg(unix)]
    fn test_set_mode() {
        use std::os::unix::fs::PermissionsExt;

        let mut builder = TempDirectoryBuilder::default();
        let script = builder.add_text_file("script.sh", "#!/bin/sh");
        builder.set_mode(&script, 0o744);
        let dir = builder.add_directory("dir");
        builder.set_mode(&dir, 0o500);
        builder.add_empty_file("dir/foo.txt");
        let temp_dir = builder.build().unwrap();

        let script_mode = std::fs::metadata(temp_dir.path().join("script.sh"))
            .unwrap()
            .permissions()
            .mode();
        let dir_mode = std::fs::metadata(temp_dir.path().join("dir"))
            .unwrap()
            .permissions()
            .mode();

        assert_eq!(script_mode & 0o777, 0o744);
        assert_eq!(dir_mode & 0o777, 0o500);
        assert!(temp_dir.path().join("dir/foo.txt").exists());

        let root = temp_dir.path().to_path_buf();

        drop(temp_dir);

        assert!(!root.exists());
    }

    #[test]
    #[cfg(unix)]
    fn test_set_mode_then_readonly() {
        use std::os::unix::fs::PermissionsExt;

        let mut builder = TempDirectoryBuilder::default();
        let foo = builder.add_empty_file("foo");
        builder.set_mode(&foo, 0o766);
        builder.set_readonly(&foo, true);
        let temp_dir = builder.build().unwrap();

        let mode = std::fs::metadata(temp_dir.path().join("foo"))
            .unwrap()
            .permissions()
            .mode();

        assert_eq!(mode & 0o777, 0o544);
    }

    #[test]
    #[cfg(unix)]
    fn test_set_executable() {
        use std::os::unix::fs::PermissionsExt;

        let mut builder = TempDirectoryBuilder::default();
        let script = builder.add_text_file("run.sh", "#!/bin/sh\necho hi\n");
        builder.set_mode(&script, 0o644);
        builder.set_executable(&script, true);
        let temp_dir = builder.build().unwrap();

        let mode = std::fs::metadata(temp_dir.path_of(&script))
            .unwrap()
            .permissions()
            .mode();

        assert_eq!(
            mode & 0o777,
            0o744,
            "owner execute bit added, rest untouched"
        );
    }

    #[test]
    #[cfg(unix)]
    fn test_set_executable_false_clears_exec_bits() {
        use std::os::unix::fs::PermissionsExt;

        let mut builder = TempDirectoryBuilder::default();
        let script = builder.add_empty_file("script");
        builder.set_mode(&script, 0o777);
        builder.set_executable(&script, false);
        let temp_dir = builder.build().unwrap();

        let mode = std::fs::metadata(temp_dir.path_of(&script))
            .unwrap()
            .permissions()
            .mode();

        assert_eq!(
            mode & 0o777,
            0o666,
            "exec bits cleared, read and write kept"
        );
    }

    #[test]
    fn test_set_executable_is_portable() {
        let mut builder = TempDirectoryBuilder::default();
        let script = builder.add_empty_file("script");
        builder.set_executable(&script, true);

        assert!(builder.build().is_ok());
    }

    #[test]
    fn test_set_readonly_applies_to_keyed_entry_not_last() {
        let mut builder = TempDirectoryBuilder::default();
        let first = builder.add_text_file("first.txt", "a");
        builder.add_text_file("second.txt", "b");
        builder.set_readonly(&first, true);
        let temp_dir = builder.build().unwrap();

        assert!(
            std::fs::metadata(temp_dir.path_of(&first))
                .unwrap()
                .permissions()
                .readonly()
        );
        assert!(
            !std::fs::metadata(temp_dir.join("second.txt"))
                .unwrap()
                .permissions()
                .readonly()
        );
    }

    #[test]
    #[cfg(unix)]
    #[should_panic(expected = "does not belong to this builder")]
    fn test_set_mode_key_from_unrelated_builder_panics() {
        let mut other = TempDirectoryBuilder::default();
        other.add_empty_file("a");
        other.add_empty_file("b");
        let key = other.add_empty_file("c");

        let mut builder = TempDirectoryBuilder::default();
        builder.add_empty_file("only");

        builder.set_mode(&key, 0o644);
    }

    #[test]
    #[cfg(unix)]
    #[should_panic(expected = "does not belong to this builder")]
    fn test_set_mode_key_index_collision_panics() {
        let mut other = TempDirectoryBuilder::default();
        let key = other.add_empty_file("other-entry");

        let mut builder = TempDirectoryBuilder::default();
        builder.add_empty_file("foo");

        builder.set_mode(&key, 0o644);
    }

    #[test]
    fn test_path_of() {
        let mut builder = TempDirectoryBuilder::default();
        let file = builder.add_text_file("dir/file.txt", "content");
        let dir = builder.add_directory("dir/nested");
        let link = builder.add_symlink("dir/link", "dir/file.txt");
        let temp_dir = builder.build().unwrap();

        assert_eq!(temp_dir.path_of(&file), temp_dir.join("dir/file.txt"));
        assert_eq!(temp_dir.path_of(&dir), temp_dir.join("dir/nested"));
        assert_eq!(temp_dir.path_of(&link), temp_dir.join("dir/link"));
    }

    #[test]
    fn test_path_of_returns_cleaned_path() {
        let mut builder = TempDirectoryBuilder::default();
        let dotted = builder.add_empty_file("./a");
        let dotdot = builder.add_empty_file("dir/../b");
        let temp_dir = builder.build().unwrap();

        assert_eq!(temp_dir.path_of(&dotted), temp_dir.join("a"));
        assert_eq!(temp_dir.path_of(&dotdot), temp_dir.join("b"));
    }

    #[test]
    fn test_key_resolves_under_each_build_root() {
        let mut builder = TempDirectoryBuilder::default();
        let file = builder.add_text_file("foo.txt", "bar");

        let first = builder.build().unwrap();
        assert_eq!(
            std::fs::read_to_string(first.path_of(&file)).unwrap(),
            "bar"
        );
        let first_root = first.path().to_path_buf();
        drop(first);

        let second = builder.build().unwrap();
        assert_eq!(
            std::fs::read_to_string(second.path_of(&file)).unwrap(),
            "bar"
        );

        assert_ne!(first_root, second.path());
    }

    #[test]
    fn test_symlink_target_from_key() {
        let mut builder = TempDirectoryBuilder::default();
        let data = builder.add_text_file("outside/precious.txt", "precious data");
        builder.add_symlink_to("link", &data);
        let temp_dir = builder.build().unwrap();

        assert_eq!(
            std::fs::read_link(temp_dir.path().join("link")).unwrap(),
            temp_dir.path_of(&data)
        );
    }

    #[test]
    fn test_add_symlink_relative_target_resolves_to_absolute() {
        let mut builder = TempDirectoryBuilder::default();
        builder.add_text_file("data/file.txt", "content");
        builder.add_symlink("link_to_data", "data");
        let temp_dir = builder.build().unwrap();

        let link_path = temp_dir.path().join("link_to_data");
        let target = std::fs::read_link(&link_path).unwrap();

        assert_eq!(target, temp_dir.path().join("data"));
    }

    #[test]
    fn test_add_symlink_to_matches_add_symlink_with_the_key() {
        let mut builder = TempDirectoryBuilder::default();
        let data = builder.add_directory("data");
        builder.add_symlink_to("link_to_data", &data);
        let temp_dir = builder.build().unwrap();

        assert_eq!(
            std::fs::read_link(temp_dir.path().join("link_to_data")).unwrap(),
            temp_dir.path_of(&data)
        );
    }

    #[test]
    #[should_panic(expected = "does not belong to this builder")]
    fn test_add_symlink_to_foreign_key_panics() {
        let mut other = TempDirectoryBuilder::default();
        let target = other.add_directory("data");

        let mut builder = TempDirectoryBuilder::default();
        builder.add_symlink_to("link", &target);
    }

    #[test]
    fn test_add_symlink_absolute_target_outside_root_is_not_removed() {
        let mut outside_builder = TempDirectoryBuilder::default();
        outside_builder.add_text_file("precious.txt", "precious data");
        let outside = outside_builder.build().unwrap();
        let target_path = outside.path().join("precious.txt");

        let mut builder = TempDirectoryBuilder::default();
        builder.add_symlink("link", &target_path);
        let temp_dir = builder.build().unwrap();

        assert_eq!(
            std::fs::read_link(temp_dir.path().join("link")).unwrap(),
            target_path
        );

        drop(temp_dir);

        assert!(target_path.exists());
    }

    #[test]
    fn test_add_relative_symlink() {
        let mut builder = TempDirectoryBuilder::default();
        builder.add_text_file("data", "content");
        builder.add_directory("dir");
        builder.add_relative_symlink("dir/link", "../data");
        let temp_dir = builder.build().unwrap();

        let link_path = temp_dir.path().join("dir/link");

        assert_eq!(
            std::fs::read_link(&link_path).unwrap(),
            Path::new("../data")
        );
        assert_eq!(
            canonicalize(&link_path).unwrap(),
            temp_dir.path().join("data")
        );
    }

    #[test]
    fn test_add_symlink_dangling_target() {
        let mut builder = TempDirectoryBuilder::default();
        builder.add_symlink("link", "missing");
        let temp_dir = builder.build().unwrap();

        let link_path = temp_dir.path().join("link");

        assert!(std::fs::symlink_metadata(&link_path).is_ok());
        assert!(!link_path.exists());
    }

    #[test]
    fn test_add_symlink_with_permissions_fails() {
        let mut builder = TempDirectoryBuilder::default();
        let link = builder.add_symlink("link", "data");
        builder.set_readonly(&link, true);
        let error = builder.build().unwrap_err();

        assert!(matches!(error, BuildError::PermissionsOnSymlink(_)));
    }

    #[test]
    fn test_dropping_symlink_does_not_affect_readonly_target() {
        let mut target_builder = TempDirectoryBuilder::default();
        let readonly = target_builder.add_text_file("readonly.txt", "foo");
        target_builder.set_readonly(&readonly, true);
        let target_dir = target_builder.build().unwrap();
        let readonly_file_path = target_dir.path().join("readonly.txt");

        let mut builder = TempDirectoryBuilder::default();
        builder.add_symlink("link", target_dir.path());
        let temp_dir = builder.build().unwrap();

        drop(temp_dir);

        assert!(readonly_file_path.exists());
        assert!(
            std::fs::metadata(&readonly_file_path)
                .unwrap()
                .permissions()
                .readonly()
        );
    }

    #[test]
    fn test_dangling_symlink_is_a_duplicate_entry() {
        let mut first_builder = TempDirectoryBuilder::default();
        first_builder.add_symlink("link", "missing");
        first_builder.delete_on_drop(false);
        let temp_dir = first_builder.build().unwrap();
        let root = temp_dir.path().to_path_buf();
        drop(temp_dir);

        let mut builder = TempDirectoryBuilder::default();
        builder.root_folder(&root);
        builder.add_symlink("link", "other-missing");
        let error = builder.build().unwrap_err();

        std::fs::remove_dir_all(&root).unwrap();

        assert!(matches!(error, BuildError::DuplicateEntry(_)));
    }

    #[test]
    #[cfg(windows)]
    fn test_add_symlink_dir_windows() {
        let mut builder = TempDirectoryBuilder::default();
        builder.add_symlink_dir("link", "missing");
        let temp_dir = builder.build().unwrap();

        let link_path = temp_dir.path().join("link");
        let metadata = std::fs::symlink_metadata(&link_path).unwrap();

        assert!(metadata.file_type().is_symlink());
    }

    #[test]
    #[cfg(windows)]
    fn test_add_symlink_file_windows() {
        let mut builder = TempDirectoryBuilder::default();
        builder.add_symlink_file("link", "missing");
        let temp_dir = builder.build().unwrap();

        let link_path = temp_dir.path().join("link");
        let metadata = std::fs::symlink_metadata(&link_path).unwrap();

        assert!(metadata.file_type().is_symlink());
    }
}
