#![allow(
    clippy::pedantic,
    clippy::nursery,
    clippy::unwrap_used,
    clippy::expect_used
)]

use temp_dir_builder::TempDirectoryBuilder;

fn main() {
    let mut builder = TempDirectoryBuilder::default();
    let readonly_file = builder.add_text_file("test/foo.txt", "bar");
    builder.set_readonly(&readonly_file, true);
    builder.add_empty_file("test/folder-a/folder-b/bar.txt");
    builder.add_file("test_file.rs", file!());
    let temp_directory = builder.build().expect("create temp dir");

    println!(
        "created successfully in {}",
        temp_directory.path().display()
    );

    let path = temp_directory.path().to_path_buf();

    assert!(path.exists());

    drop(temp_directory);

    assert!(!path.exists());
}
