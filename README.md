# temp-dir-builder [![Rust](https://github.com/IohannRabeson/temp-dir-builder/actions/workflows/rust.yml/badge.svg)](https://github.com/IohannRabeson/temp-dir-builder/actions/workflows/rust.yml)

Oftentimes, testing scenarios involve interactions with the file system. `temp-dir-builder` provides a convenient solution for creating file system trees tailored to the needs of your tests. This library offers:

- An easy way to generate a tree with recursive paths.
- Tree creation within a temporary folder.
- The ability to create a tree using a builder.

## Usage

With the builder API, you can define file paths and contents in a structured way. Here’s how to create a tree with the builder:
When the `temp_dir` instance is dropped, the temporary folder and its contents are automatically deleted, which is particularly useful for tests that require a clean state.

<!-- <snip id="example-builder" inject_from="code" strip_prefix="/// " template="rust"> -->
```rust
use temp_dir_builder::TempDirectoryBuilder;
let mut builder = TempDirectoryBuilder::default();
builder.add_text_file("test/foo.txt", "bar");
builder.add_binary_file("test/foo2.txt", &[98u8, 97u8, 114u8]);
builder.add_empty_file("test/folder-a/folder-b/bar.txt");
builder.add_file("test/file.rs", file!());
builder.add_directory("test/dir");
let temp_dir = builder.build().expect("create temp dir");
println!("created successfully in {}", temp_dir.path().display());
```
<!-- </snip> -->

Each `add_*` method returns an `EntryKey`, which can be used afterwards to configure that entry's permissions or to resolve its path:

<!-- <snip id="example-set-readonly" inject_from="code" strip_prefix="    /// " template="rust"> -->
```rust
use temp_dir_builder::TempDirectoryBuilder;
let mut builder = TempDirectoryBuilder::default();
let foo = builder.add_text_file("test/foo.txt", "bar");
builder.set_readonly(&foo, true);
builder.add_directory("test/dir");
let temp_dir = builder.build().expect("create temp dir");
```
<!-- </snip> -->

`set_executable` sets the owner execute bit on Unix and does nothing on other
platforms, a portable spelling for "this file must be runnable":

<!-- <snip id="example-set-executable" inject_from="code" strip_prefix="    /// " template="rust"> -->
```rust
use temp_dir_builder::TempDirectoryBuilder;
let mut builder = TempDirectoryBuilder::default();
let script = builder.add_text_file("run.sh", "#!/bin/sh");
builder.set_executable(&script, true);
let temp_dir = builder.build().expect("create temp dir");
```
<!-- </snip> -->

A directory only needed as the parent of another entry can still be declared
explicitly to get a key for it, instead of naming it again with `join`:

<!-- <snip id="example-path-of" inject_from="code" strip_prefix="    /// " template="rust"> -->
```rust
use temp_dir_builder::TempDirectoryBuilder;
let mut builder = TempDirectoryBuilder::default();
let repository = builder.add_directory("repository");
builder.add_text_file("repository/.gitignore", "*.log");
let temp_dir = builder.build().expect("create temp dir");
let repository_path = temp_dir.path_of(&repository);
assert_eq!(repository_path, temp_dir.join("repository"));
```
<!-- </snip> -->

An entry nested under a directory already declared on the builder can be
declared with `in_directory` instead of naming that directory again in every
child's path:

<!-- <snip id="example-in-directory" inject_from="code" strip_prefix="    /// " template="rust"> -->
```rust
use temp_dir_builder::TempDirectoryBuilder;
let mut builder = TempDirectoryBuilder::default();
let repository = builder.add_directory("repository");
let gitignore = builder.in_directory(&repository, |builder| {
    builder.add_empty_file("a");
    builder.add_text_file(".gitignore", "a
")
});
let temp_dir = builder.build().expect("create temp dir");
assert_eq!(temp_dir.path_of(&gitignore), temp_dir.join("repository/.gitignore"));
```
<!-- </snip> -->

On Unix platforms, `set_mode` can be used to set the raw permission bits:

<!-- <snip id="example-set-mode" inject_from="code" strip_prefix="    /// " template="rust"> -->
```rust
# #[cfg(unix)]
# {
use temp_dir_builder::TempDirectoryBuilder;
let mut builder = TempDirectoryBuilder::default();
let foo = builder.add_text_file("test/foo.txt", "bar");
builder.set_mode(&foo, 0o744);
builder.add_directory("test/dir");
let temp_dir = builder.build().expect("create temp dir");
# }
```
<!-- </snip> -->

When a file's content needs to embed the root path of the temporary directory
itself (a config file listing an absolute path, a script referring to a file
next to itself), `add_text_file_with` computes the content from the root path
at `build()` time:

<!-- <snip id="example-add-text-file-with" inject_from="code" strip_prefix="    /// " template="rust"> -->
```rust
use temp_dir_builder::TempDirectoryBuilder;
let mut builder = TempDirectoryBuilder::default();
builder.add_text_file_with("config.toml", |root| {
    format!("data_dir = {:?}", root.join("data"))
});
builder.add_directory("data");
let temp_dir = builder.build().expect("create temp dir");
```
<!-- </snip> -->

A symbolic link can be added with `add_symlink`. A relative target is resolved
against the root of the temporary directory:

<!-- <snip id="example-add-symlink" inject_from="code" strip_prefix="    /// " template="rust"> -->
```rust
use temp_dir_builder::TempDirectoryBuilder;
let mut builder = TempDirectoryBuilder::default();
builder.add_text_file("data/file.txt", "content");
builder.add_symlink("link_to_data", "data");
builder.add_symlink("link_to_file", "data/file.txt");
let temp_dir = builder.build().expect("create temp dir");
```
<!-- </snip> -->

`add_symlink_to` targets an entry already declared on this builder instead of
a path, so a renamed or removed target is a compile error at the call site
rather than a silently dangling link:

<!-- <snip id="example-add-symlink-to" inject_from="code" strip_prefix="    /// " template="rust"> -->
```rust
use temp_dir_builder::TempDirectoryBuilder;
let mut builder = TempDirectoryBuilder::default();
let data = builder.add_directory("data");
builder.add_symlink_to("link_to_data", &data);
let temp_dir = builder.build().expect("create temp dir");
```
<!-- </snip> -->

`add_relative_symlink` writes the target verbatim instead, interpreted by the
OS relative to the link's parent directory:

<!-- <snip id="example-add-relative-symlink" inject_from="code" strip_prefix="    /// " template="rust"> -->
```rust
use temp_dir_builder::TempDirectoryBuilder;
let mut builder = TempDirectoryBuilder::default();
builder.add_text_file("data/file.txt", "content");
builder.add_relative_symlink("dir/link", "../data");
let temp_dir = builder.build().expect("create temp dir");
```
<!-- </snip> -->

## Adding to a tree that already exists

`TempDirectoryAdditions` declares more entries in a tree that has already been
built, for the case where something has to happen between two parts of the
tree: a directory must exist before `git init` runs, and the commit must exist
before the part of the tree the test measures is created.

<!-- <snip id="example-build-into" inject_from="code" strip_prefix="    /// " template="rust"> -->
```rust
use temp_dir_builder::{TempDirectoryBuilder, TempDirectoryAdditions};
let mut builder = TempDirectoryBuilder::default();
let repository = builder.add_directory("repository");
let temp_dir = builder.build().expect("create temp dir");
// git init, or anything else that needs the directory to exist
let mut additions = TempDirectoryAdditions::default();
let gitignore = additions.add_text_file("repository/.gitignore", "*.log");
additions.build_into(&temp_dir).expect("extend temp dir");
assert!(temp_dir.path_of(&gitignore).is_file());
```
<!-- </snip> -->

A collision with something already on disk succeeds only when the declared
entry and the existing one agree on kind: a declared file replaces an existing
file's content, and a declared directory reuses an existing directory. Any
other collision, including a kind mismatch or an existing symlink, fails with
`DuplicateEntry`, so a typo that changes what shape of entry is expected at a
path is still caught; a typo that lands on a same-shaped pre-existing entry is
not, and is overwritten. Anything present and not declared is left untouched.

Every entry is checked against that rule before anything is created, so a
`DuplicateEntry` from one entry never leaves an earlier entry's target
mutated.

`TempDirectoryAdditions` has no `root_folder`, `random_root_in`, or
`delete_on_drop`: it always writes into the `TempDirectory` passed to
`build_into`, which stays responsible for deleting the tree.

## Migrating from 0.3.0

`add_*` methods used to consume and return `self`/`EntryBuilder`, so trees were built as one chained expression. As of 0.4.0 they take `&mut self` and return an `EntryKey` instead, so that key can later resolve the entry's path or configure its permissions without re-typing its declared path. 0.3.0-style chains (`TempDirectoryBuilder::default().add_text_file(..).build()`) no longer compile; declare the builder with `let mut builder = ...` and call each method as its own statement, as shown above.

## Development

On Windows, creating symlinks requires either running elevated or enabling
Developer Mode (Settings > Privacy & security > For developers). Without one
of these, the tests that create a symlink fail with
`ERROR_PRIVILEGE_NOT_HELD` (OS error 1314).

## Credits
This is a fork of [tree-fs](https://github.com/kaplanelad/tree-fs) I heavily rewritten, original idea by Elad Kaplan.  

The reason I forked is that the layer allowing directory trees to be defined via YAML causes more issues than it fixes, because YAML lets you introduce errors that won't appear at compile time.
