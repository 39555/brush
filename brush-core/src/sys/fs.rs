#[allow(unused_imports)]
pub(crate) use super::platform::fs::*;

use std::borrow::Cow;
#[cfg(unix)]
pub(crate) use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};
#[cfg(not(unix))]
pub(crate) use StubMetadataExt as MetadataExt;

pub(crate) trait PathExt {
    fn readable(&self) -> bool;
    fn writable(&self) -> bool;
    fn executable(&self) -> bool;

    fn exists_and_is_block_device(&self) -> bool;
    fn exists_and_is_char_device(&self) -> bool;
    fn exists_and_is_fifo(&self) -> bool;
    fn exists_and_is_socket(&self) -> bool;
    fn exists_and_is_setgid(&self) -> bool;
    fn exists_and_is_setuid(&self) -> bool;
    fn exists_and_is_sticky_bit(&self) -> bool;
}

/// An error when you trying to pass not absolute path into the [`AbsolutePath::from_absolute`]
#[derive(thiserror::Error, Debug)]
#[error("path is not absolute {0}")]
pub struct IsNotAbsolute<P>(P);

/// A wrapper around [`std::path::Path`] to indicate that some functions require an absolute path in
/// order to work correctly for example [`normalize_virtually`]
/// Does not allocate any memory if the path is already absolute
pub struct AbsolutePath<'a>(Cow<'a, Path> /* May contain either &Path or PathBuf */);
impl<'a> AbsolutePath<'a> {
    pub fn into_inner(self) -> Cow<'a, Path> {
        self.0
    }
    /// Construct and absolute path from the `path` relative to `relative_to`
    pub fn new<R>(relative_to: R, path: TildaExpandedPath<'a>) -> Self
    where
        // can use &Path or PathBuf
        std::path::PathBuf: From<R>,
        Cow<'a, Path>: From<R>,
        std::path::PathBuf: From<&'a str>,
        R: AsRef<Path>,
    {
        AbsolutePath(make_absolute(relative_to, path))
    }
    /// Try to create this type from the any path that is already been absolute. Return and error
    /// if the path is not absolute.
    pub fn from_absolute<P>(path: P) -> Result<Self, IsNotAbsolute<P>>
    where
        P: AsRef<Path>,
        Cow<'a, Path>: From<P>,
    {
        if path.as_ref().is_absolute() {
            Ok(AbsolutePath(Cow::from(path)))
        } else {
            Err(IsNotAbsolute(path))
        }
    }
}

impl AsRef<Path> for AbsolutePath<'_> {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

/// The type to indicate that some path does not contait a telde. Constructed fromn the
/// [`expand_tilde_with_home`]
/// Doesn not allocate any memory if the path already without a tilde
pub struct TildaExpandedPath<'a>(Cow<'a, Path> /* May contain either &Path or PathBuf */);
impl<'a> TildaExpandedPath<'a> {
    pub fn into_inner(self) -> Cow<'a, Path> {
        self.0
    }
}

/// Make some path absolute
/// NOTE: we need [`TildaExpandedPath`] to ensure that the given path does not contain a tilde
/// because otherwise we can end up with "/some/path/~" or
// "/some/path/~user"
pub fn make_absolute<'a, R>(relative_to: R, path: TildaExpandedPath<'a>) -> Cow<'a, Path>
where
    // if it is a &Path convert it to_path_buf() only if nessesarry, if it is PathBuf return the
    // argument unchanged
    std::path::PathBuf: From<R>,
    std::path::PathBuf: From<&'a str>,
    // if it is a &Path and we don't need to convert anything, return it unchanged without any
    // allocations
    R: AsRef<Path>,
    Cow<'a, Path>: From<R>,
{
    let path = path.into_inner();
    if path.as_ref().is_absolute() {
        // just return unchanged
        path.into()
    } else {
        if path.as_ref().as_os_str().as_encoded_bytes() == b"." {
            // Joining a Path with '.' appends a '.' at the end,
            // so we don't do anything, which should result in an equal
            // path on all supported systems.
            if relative_to.as_ref().as_os_str().is_empty() {
                return PathBuf::from("/").into();
            }
            return relative_to.into();
        }

        let relative_to = if relative_to.as_ref().as_os_str().is_empty() {
            PathBuf::from("/")
        } else {
            PathBuf::from(relative_to)
        };

        relative_to.join(path).into()
    }
}

/// Canonicalize the path. unlike [`std::fs::canonicalize`], it does not use sys calls (such as
/// readlink). Does not convert to absolute form nor does it resolve symlinks.
/// Return a new path where:
/// - Multiple `/`'s are collapsed to a single `/`.
/// - Leading `./`'s and trailing `/.`'s are removed.
/// - `../`'s are handled by removing portions of the path.
/// This function strictly requires an absolute path because
/// it performs this transform lexically, without touching the filesystem.
pub fn normalize_lexically<'a>(path: AbsolutePath<'a>) -> Cow<'a, Path> {
    let path = path.into_inner();

    if is_normalized(&path) {
        return path;
    }
    // NOTE: This is mostly taken from std::path:absolute except we don't use
    // std::env::current_dir()

    // Get the components, skipping the redundant leading "." component if it exists.
    #[allow(unused_mut)] // for unix
    let mut components = path.strip_prefix(".").unwrap_or(&path).components();
    let path_os = path.as_os_str().as_encoded_bytes();

    let mut normalized = {
        // https://pubs.opengroup.org/onlinepubs/9699919799/basedefs/V1_chap04.html#tag_04_13
        // TODO: link to posix
        // Posix: "If a pathname begins with two successive <slash> characters, the
        // first component following the leading <slash> characters may be
        // interpreted in an implementation-defined manner, although more than
        // two leading <slash> characters shall be treated as a single <slash>
        // character."
        #[cfg(unix)]
        {
            if path_os.starts_with(b"//") && !path_os.starts_with(b"///") {
                components.next();
                PathBuf::from("//")
            } else {
                PathBuf::new()
            }
        }
        #[cfg(not(unix))]
        {
            PathBuf::new()
        }
    };

    for component in components {
        match component {
            // TODO: windows path prefix
            Component::Prefix(..) => normalized.push(component.as_os_str()),
            Component::RootDir => {
                normalized.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(c) => {
                normalized.push(c);
            }
        }
    }

    // Empty string is really the `/' because we started with an absolute path
    if normalized.as_os_str().is_empty() {
        normalized.push(Component::RootDir);
    }

    // "Interfaces using pathname resolution may specify additional constraints
    // when a pathname that does not name an existing directory contains at
    // least one non- <slash> character and contains one or more trailing
    // <slash> characters".
    // A trailing <slash> is also meaningful if "a symbolic link is
    // encountered during pathname resolution".
    if path_os.ends_with(b"/") || path_os.ends_with(std::path::MAIN_SEPARATOR_STR.as_bytes()) {
        normalized.push("");
    }
    Cow::from(normalized)
}

/// Make a Windows path absolute.
// pub fn normalize_virtually_win<'a>(path: AbsolutePath<'a>) -> Cow<'a, Path> {
//     let path = path.into_inner().as_os_str();
//     let prefix = std::sys::path::parse_prefix(path);
//     // // Verbatim paths should not be modified.
//     // if prefix.map(|x| x.is_verbatim()).unwrap_or(false) {
//     //     // NULs in verbatim paths are rejected for consistency.
//     //     if path.as_encoded_bytes().contains(&0) {
//     //         return Err(io::const_io_error!(
//     //             io::ErrorKind::InvalidInput,
//     //             "strings passed to WinAPI cannot contain NULs",
//     //         ));
//     //     }
//     //     return Ok(path.to_owned().into());
//     // }
//     //
//     // let path = to_u16s(path)?;
//     // let lpfilename = path.as_ptr();
//     // fill_utf16_buf(
//     //     // SAFETY: `fill_utf16_buf` ensures the `buffer` and `size` are valid.
//     //     // `lpfilename` is a pointer to a null terminated string that is not
//     //     // invalidated until after `GetFullPathNameW` returns successfully.
//     //     |buffer, size| unsafe { c::GetFullPathNameW(lpfilename, size, buffer, ptr::null_mut())
// },     //     os2path,
//     // )
//
//     Cow::from(PathBuf::new())
// }

fn is_normalized(path: &Path) -> bool {
    let path_os = path.as_os_str().as_encoded_bytes();
    // require ending `/`
    if !(path_os.ends_with(b"/")
        // check '\'
        || (cfg!(windows) && path_os.ends_with(std::path::MAIN_SEPARATOR_STR.as_bytes())))
    {
        return false;
    }

    // does not have any of `.`, `..`
    if path
        .components()
        .any(|c| matches!(c, Component::CurDir | Component::ParentDir))
    {
        return false;
    }

    // has any if doubled slashes a/b//d

    #[cfg(unix)]
    {
        if path.as_os_str().len() > 2 {
            return
                // Skip Posix first //
                path_os[2..]
                .windows(2)
                // any doubled path separators
                .position(|window| window == b"//")
                .is_none();
        }
    };

    #[cfg(windows)]
    {
        return path_os
            .windows(2)
            .position(|window| window == b"//" || window == b"\\\\")
            .is_none();
    };
    // TODO: refactor without that
    #[allow(unreachable_code)]
    true
}

/// Performs tilde expansion
/// Returns a [`TildaExpandedPath`] type that indicates that path is expanded and ready for further
/// processing
pub fn expand_tilde_with_home<'a, P, H>(path: &'a P, home: H) -> TildaExpandedPath<'a>
where
    std::path::PathBuf: From<H>,
    H: AsRef<Path> + 'a,
    P: AsRef<Path> + ?Sized,
    Cow<'a, Path>: From<&'a Path>,
    Cow<'a, Path>: From<H>,
{
    let path = path.as_ref();

    match path.components().next() {
        Some(Component::Normal(p)) if p.as_encoded_bytes() == b"~" => (),
        // already expanded
        _ => return TildaExpandedPath(path.into()),
    }

    if home.as_ref().as_os_str().is_empty() || home.as_ref().as_os_str().as_encoded_bytes() == b"/"
    {
        // Corner case: `home` is a root directory;
        // don't prepend extra `/`, just drop the tilde.
        if let Ok(p) = path.strip_prefix("~") {
            TildaExpandedPath(Cow::from(p))
        } else {
            TildaExpandedPath(path.into())
        }
    } else {
        for prefix in [
            "~/",
            #[cfg(windows)]
            &"~\\",
        ] {
            if let Ok(p) = path.strip_prefix(prefix) {
                // Corner case: `p` is empty;
                // Don't append extra '/', just keep `home` as is.
                // This happens because PathBuf.push will always
                // add a separator if the pushed path is relative,
                // even if it's empty
                if !p.as_os_str().as_encoded_bytes().is_empty() {
                    let mut home = PathBuf::from(home);
                    home.push(p);
                    return TildaExpandedPath(home.into());
                } else {
                    return TildaExpandedPath(home.into());
                }
            }
        }
        unreachable!("Should return earlier if the given path does not contain a tilde")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_normalized() {
        let mut tests_normalized = vec!["/aa/bb/cc/dd/"];

        #[cfg(unix)]
        tests_normalized.extend_from_slice(&["//aa/special/posix/case"]);

        let tests_not_normalized = vec![
            "/aa/bb/cc/dd",
            "/aa/bb/../cc/dd",
            "///aa/bb/cc/dd",
            "///////aa/bb/cc/dd",
            "./aa/bb/cc/dd",
            "/aa/bb//cc/dd",
            "/aa/bb////cc/./dd",
            "/aa/bb////cc/dd",
        ];

        #[cfg(windows)]
        tests_normalized.extend_from_slice(&["C:/aa/bb/", "C:\\aa\\bb\\", "C:\\aa/b/"]);

        #[cfg(windows)]
        let tests_not_normalized = [
            tests_not_normalized.as_slice(),
            &["C:/aa/bb", "C:\\\\aa\\bb\\", "C:\\aa///b/"], // TODO: unc paths
        ]
        .concat();

        for test in tests_normalized {
            assert!(is_normalized(&Path::new(test)), "{}", test);
        }
        for test in tests_not_normalized {
            assert!(!is_normalized(&Path::new(test)), "{}", test);
        }
    }

    #[test]
    fn test_absolute() {
        #[cfg(unix)]
        let home = PathBuf::from("/home/user/");
        #[cfg(windows)]
        let home = PathBuf::from("C:\\home\\user\\");

        #[cfg(unix)]
        let cwd = PathBuf::from("/relative/to");
        #[cfg(windows)]
        let cwd = PathBuf::from("C:\\relative\\to");

        #[cfg(unix)]
        let tests = vec![
            ("~/aa/bb/", "/home/user/aa/bb"),
            ("./", "/relative/to/"),
            (".", "/relative/to/"),
        ];

        #[cfg(windows)]
        let tests = vec![
            ("~/aa/bb/", "C:/home/user/aa/bb"),
            ("./", "C:/relative/to/"),
            (".", "C:/relative/to/"),
        ];

        for test in tests {
            assert_eq!(
                make_absolute(&cwd, expand_tilde_with_home(&PathBuf::from(test.0), &home)),
                PathBuf::from(test.1)
            );
        }
    }

    // TODO: posix tests //sdf/
    #[test]
    #[cfg(unix)]
    fn test_normalize_lexically() {
        let tests = vec![
            ("/", "/"),
            ("//", "/"),
            ("///", "/"),
            ("/.//", "/"),
            ("//..", "/"),
            ("/..//", "/"),
            ("/..//", "/"),
            ("/.//./", "/"),
            ("/././/./", "/"),
            ("/./././", "/"),
            ("/path//to///thing", "/path/to/thing"),
            ("/aa/bb/../cc/dd", "/aa/cc/dd"),
            ("/../aa/bb/../../cc/dd", "/cc/dd"),
            ("/../../../../aa/bb/../../cc/dd", "/cc/dd"),
            ("/aa/bb/../../cc/dd/../../../../../../../../../", "/"),
            ("/../../../../../../..", "/"),
            ("/../../../../../...", "/..."),
            ("/test/./path/", "/test/path"),
            ("/test/../..", "/"),
        ];

        for test in tests {
            assert_eq!(
                normalize_lexically(AbsolutePath::from_absolute(PathBuf::from(test.0)).unwrap()),
                PathBuf::from(test.1)
            );
        }

        // empty path is a root dir
        assert_eq!(
            normalize_lexically(AbsolutePath::new(
                Path::new(""),
                TildaExpandedPath(Cow::from(Path::new("")))
            )),
            PathBuf::from("/")
        );
        assert_eq!(
            normalize_lexically(AbsolutePath::from_absolute(PathBuf::from("/./././")).unwrap()),
            PathBuf::from("/")
        );
    }
    // TODO: windows tests \\ UTC dicsks

    #[test]
    #[cfg(windows)]
    fn test_normalize_lexically_windows() {
        let tests = vec![
            ("C:\\..", "C:\\"),
            ("C:\\..\\test", "C:\\test"),
            ("C:\\test\\..", "C:\\"),
            ("C:\\test\\path\\..\\..\\..", "C:\\"),
            ("C:\\test\\path/..\\../another\\path", "C:\\another\\path"),
            ("C:\\test\\path\\my/path", "C:\\test\\path\\my\\path"),
            ("C:/dir\\../otherDir/test.json", "C:/otherDir/test.json"),
            ("c:\\test\\..", "c:\\"),
            ("c:/test/..", "c:/"),
        ];

        for test in tests {
            assert_eq!(
                normalize_lexically(AbsolutePath::from_absolute(PathBuf::from(test.0)).unwrap()),
                PathBuf::from(test.1)
            );
        }
    }

    #[test]
    fn test_expand_tilde() {
        fn check_expanded(s: &str) {
            #[cfg(unix)]
            let home = PathBuf::from("/home");
            #[cfg(windows)]
            let home = PathBuf::from("C:\\Users\\Home\\");

            assert!(expand_tilde_with_home(Path::new(s), &home)
                .into_inner()
                .starts_with(home.as_path()));

            // Tests the special case in expand_tilde for "/" as home
            let home = PathBuf::from("/");
            assert!(!expand_tilde_with_home(Path::new(s), home)
                .into_inner()
                .starts_with("//"));
        }

        fn check_not_expanded(s: &str) {
            #[cfg(unix)]
            let home = PathBuf::from("/home");
            #[cfg(windows)]
            let home = PathBuf::from("C:\\Users\\Home\\");
            let expanded = expand_tilde_with_home(Path::new(s), home).into_inner();
            assert_eq!(expanded, Path::new(s));
        }

        let tests_expanded = vec!["~", "~/test/", "~//test/"];
        let tests_not_expanded = vec!["1~1", "~user/", ""];

        for test in tests_expanded {
            check_expanded(test)
        }
        for test in tests_not_expanded {
            check_not_expanded(test)
        }
    }
}
