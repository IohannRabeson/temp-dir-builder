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
let temp_dir = TempDirectoryBuilder::default()
    .add_text_file("test/foo.txt", "bar")
    .add_binary_file("test/foo2.txt", &[98u8, 97u8, 114u8])
    .add_empty_file("test/folder-a/folder-b/bar.txt")
    .add_file("test/file.rs", file!())
    .add_directory("test/dir")
    .build()
    .expect("create temp dir");
println!("created successfully in {}", temp_dir.path().display());
```
<!-- </snip> -->

Right after adding a file or a directory, you can configure its permissions before continuing the chain:

<!-- <snip id="example-set-readonly" inject_from="code" strip_prefix="/// " template="rust"> -->
```rust
use temp_dir_builder::TempDirectoryBuilder;
let temp_dir = TempDirectoryBuilder::default()
    .add_text_file("test/foo.txt", "bar").set_readonly(true)
    .add_directory("test/dir")
    .build()
    .expect("create temp dir");
```
<!-- </snip> -->

On Unix platforms, `set_mode` can be used to set the raw permission bits:

<!-- <snip id="example-set-mode" inject_from="code" strip_prefix="/// " template="rust"> -->
```rust
# #[cfg(unix)]
# {
use temp_dir_builder::TempDirectoryBuilder;
let temp_dir = TempDirectoryBuilder::default()
    .add_text_file("test/foo.txt", "bar").set_mode(0o744)
    .add_directory("test/dir")
    .build()
    .expect("create temp dir");
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
let temp_dir = TempDirectoryBuilder::default()
    .add_text_file_with("config.toml", |root| {
        format!("data_dir = {:?}", root.join("data"))
    })
    .add_directory("data")
    .build()
    .expect("create temp dir");
```
<!-- </snip> -->

A symbolic link can be added with `add_symlink`. A relative target is resolved
against the root of the temporary directory:

<!-- <snip id="example-add-symlink" inject_from="code" strip_prefix="    /// " template="rust"> -->
```rust
use temp_dir_builder::TempDirectoryBuilder;
let temp_dir = TempDirectoryBuilder::default()
    .add_text_file("data/file.txt", "content")
    .add_symlink("link_to_data", "data")
    .add_symlink("link_to_file", "data/file.txt")
    .build()
    .expect("create temp dir");
```
<!-- </snip> -->

`add_relative_symlink` writes the target verbatim instead, interpreted by the
OS relative to the link's parent directory:

<!-- <snip id="example-add-relative-symlink" inject_from="code" strip_prefix="    /// " template="rust"> -->
```rust
use temp_dir_builder::TempDirectoryBuilder;
let temp_dir = TempDirectoryBuilder::default()
    .add_text_file("data/file.txt", "content")
    .add_relative_symlink("dir/link", "../data")
    .build()
    .expect("create temp dir");
```
<!-- </snip> -->

## Credits
This is a fork of [tree-fs](https://github.com/kaplanelad/tree-fs) I heavily rewritten, original idea by Elad Kaplan.  

The reason I forked is that the layer allowing directory trees to be defined via YAML causes more issues than it fixes, because YAML lets you introduce errors that won't appear at compile time.
