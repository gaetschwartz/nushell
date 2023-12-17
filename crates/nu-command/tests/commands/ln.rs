use nu_test_support::fs::file_contents;

use nu_test_support::nu;
use nu_test_support::playground::Playground;

use std::path::{Path, PathBuf};

fn get_file_hash(file: impl AsRef<Path>) -> String {
    nu!("open -r {} | to text | hash md5", file.as_ref().display()).out
}

#[test]
fn uses_nu_ln() {
    let actual = nu!("which ln | get type.0").out;

    assert_eq!(actual, "built-in");
}

#[test]
fn symlinks_default() {
    Playground::setup("ln_test_1", |dirs, _| {
        let target_file = dirs.formats().join("sample.ini");
        let link_file = dirs.test().join("linked_sample.ini");

        nu!(
            cwd: dirs.root(),
            "ln `{}` `{}`",
            target_file.display(),
            link_file.display()
        );

        assert!(link_file.exists());
        assert!(link_file.is_file());
        assert!(get_file_hash(link_file) == get_file_hash(target_file));
    });
}

#[test]
fn symlinks_symbolic() {
    Playground::setup("ln_test_2", |dirs, _| {
        let target_file = dirs.formats().join("sample.ini");
        let link_file = dirs.test().join("linked_sample.ini");

        println!(
            "Symlinking {} to {}",
            target_file.display(),
            link_file.display()
        );

        nu!(
            cwd: dirs.root(),
            "ln -s `{}` `{}`",
            target_file.display(),
            link_file.display()
        );

        assert!(link_file.exists());
        assert!(link_file.is_symlink());
        assert!(get_file_hash(link_file) == get_file_hash(target_file));
    });
}

#[test]
fn symlinks_force() {
    Playground::setup("ln_test_3", |dirs, _| {
        let target_file = dirs.formats().join("sample.ini");
        let link_file = dirs.test().join("linked_sample.ini");

        // create a file with the same name as the link
        std::fs::write(&link_file, "This is a file").unwrap();

        let out = nu!(
            cwd: dirs.root(),
            "ln -s `{}` `{}`",
            target_file.display(),
            link_file.display()
        );
        assert!(out.status.code() != Some(0));

        let out = nu!(
            cwd: dirs.root(),
            "ln -f -s `{}` `{}`",
            target_file.display(),
            link_file.display()
        );
        assert!(out.status.code() == Some(0));

        assert!(link_file.exists());
        assert!(link_file.is_symlink());
        assert!(get_file_hash(link_file) == get_file_hash(target_file));
    });
}

#[test]
fn symlinks_relative() {
    Playground::setup("ln_test_4", |dirs, _| {
        let link_file = "linked_sample.txt";
        let (dir1, tgtf1) = create_rel_dir(&dirs, "dir1", "sample.txt");
        let (dir2, tgtf2) = create_rel_dir(&dirs, "dir2", "sample.txt");
        let link_file1 = dir1.join(link_file);
        let link_file2 = dir2.join(link_file);

        let out = nu!(
            cwd: dir1,
            "ln -v -s -r `{}` `{}`",
            tgtf1.file_name().unwrap().to_str().unwrap(),
            link_file1.display()
        )
        .out;

        println!("out = {}", out);

        assert!(link_file1.exists());
        assert!(link_file1.is_symlink());
        assert_eq!(file_contents(&link_file1), file_contents(&tgtf1));

        // move the link to another directory
        std::fs::rename(&link_file1, &link_file2).unwrap();

        // check that the link is still valid
        assert!(link_file2.exists());
        assert!(link_file2.is_symlink());
        assert_eq!(file_contents(&link_file2), file_contents(tgtf2));
    });
}

fn create_rel_dir(
    dirs: &nu_test_support::playground::Dirs,
    dir: &str,
    name: &str,
) -> (PathBuf, PathBuf) {
    let parent = &dirs.test().join(dir);
    let target_file = parent.join(name);

    // create the necessary directories
    std::fs::create_dir_all(target_file.parent().unwrap()).unwrap();
    // write a file to the target
    std::fs::write(&target_file, format!("This is a file in {}/{}", dir, name)).unwrap();

    (parent.to_path_buf(), target_file)
}
