//! The one-time move of a folder or a file to a new place, for an app that changes where it keeps
//! its files.
//!
//! On one volume, one rename moves the entry. Across two volumes, the caller selects the action:
//! leave the old entry, or copy it, compare the copy and then remove the old entry. The function
//! returns what it did, and the app logs it.

use std::fs::{self, FileType};
use std::io::{self, Read as _};
use std::path::{Path, PathBuf};

#[cfg(test)]
mod tests;

/// The end of the name of the part copy beside the new place.
const PART_SUFFIX: &str = ".part";

/// The action of `move_once` when the old and the new place are on two volumes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OtherVolume {
    /// Leave the old entry and move nothing, for a cache that the app can fill again.
    Leave,
    /// Copy the old entry, compare the copy and put it at the new place. With `remove_old`, the
    /// old entry goes after the copy, else it stays for the app.
    Copy {
        /// Whether the old entry goes after a copy that the compare found equal.
        remove_old: bool,
    },
}

/// What `move_once` found and did, so that the app can log the step.
#[derive(Debug)]
pub enum Moved {
    /// Nothing is at the old place, so nothing moved.
    NoOld,
    /// An entry (also a link that points nowhere) is at the new place. It wins, and the old
    /// entry stays.
    NewExists,
    /// One rename moved the old entry to the new place.
    Renamed,
    /// The old entry is on another volume. The new place holds a copy that the compare found
    /// equal. The old entry went, or it stays when the caller set `remove_old` to false.
    Copied,
    /// The new place holds a copy that the compare found equal, but the removal of the old entry
    /// failed with this error. The app tells the user to remove the old entry.
    CopiedOldStays(io::Error),
    /// The old entry is on another volume and stays, because the caller selected
    /// `OtherVolume::Leave`. Nothing moved.
    LeftOnOtherVolume,
}

/// Moves the folder or the file `old` to `new` one time, so that an app can change where it keeps
/// its files.
///
/// Nothing at `old` gives `NoOld`. An entry at `new` gives `NewExists`, also a link that points
/// nowhere, and `old` stays. Else the parent of `new` comes first, then one rename. A rename
/// across two volumes follows `other_volume`. A copy goes to `.<name>.part` beside `new` and keeps
/// the modes, the file times and the links. A socket or a FIFO in a folder stays behind. After the
/// compare, the part copy becomes `new`, and only then does `old` go. A failed copy removes the
/// part copy and leaves `old` as it was. A part copy of an earlier run that stopped goes first.
///
/// Only one process at a time must move to `new`: the app holds a lock for that, if it needs one.
pub fn move_once(old: &Path, new: &Path, other_volume: OtherVolume) -> io::Result<Moved> {
    move_once_with(old, new, other_volume, |from, to| fs::rename(from, to))
}

/// `move_once` with the rename of the caller, so that a test can act as two volumes.
///
/// A rename that fails with `ErrorKind::CrossesDevices` makes the copy of `other_volume`. The copy
/// and the rename of the part copy use `std::fs`.
pub fn move_once_with(
    old: &Path,
    new: &Path,
    other_volume: OtherVolume,
    rename: impl Fn(&Path, &Path) -> io::Result<()>,
) -> io::Result<Moved> {
    if !is_present(old)? {
        return Ok(Moved::NoOld);
    }
    if is_present(new)? {
        return Ok(Moved::NewExists);
    }
    if let Some(parent) = new.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .map_err(|error| context(&error, &format!("cannot create {}", parent.display())))?;
    }
    match rename(old, new) {
        Ok(()) => return Ok(Moved::Renamed),
        Err(error) if error.kind() == io::ErrorKind::CrossesDevices => {}
        Err(error) => {
            return Err(context(
                &error,
                &format!("cannot rename {} to {}", old.display(), new.display()),
            ));
        }
    }
    let OtherVolume::Copy { remove_old } = other_volume else {
        return Ok(Moved::LeftOnOtherVolume);
    };
    copy_into_place(old, new)?;
    if !remove_old {
        return Ok(Moved::Copied);
    }
    Ok(match remove_entry(old) {
        Ok(()) => Moved::Copied,
        Err(error) => Moved::CopiedOldStays(error),
    })
}

/// Whether an entry is at `path`, without a follow of a link. An error other than `NotFound` is
/// an error, because the move must not guess.
fn is_present(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(context(&error, &format!("cannot read {}", path.display()))),
    }
}

/// Copies `old` to the part copy beside `new`, compares the two and renames the part copy to
/// `new`. A failed step removes the part copy, never `old`.
fn copy_into_place(old: &Path, new: &Path) -> io::Result<()> {
    let part = part_path(new)?;
    remove_entry(&part)
        .map_err(|error| context(&error, &format!("cannot remove {}", part.display())))?;
    let copied = copy_and_compare(old, &part).and_then(|()| {
        fs::rename(&part, new).map_err(|error| {
            context(
                &error,
                &format!("cannot rename {} to {}", part.display(), new.display()),
            )
        })
    });
    if copied.is_err() {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the error of the copy matters more, and the next move removes a part copy that stays"
        )]
        let _ = remove_entry(&part);
    }
    copied
}

/// The part copy of a move to `new`: `.<name>.part` in the same folder.
fn part_path(new: &Path) -> io::Result<PathBuf> {
    let name = new.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} has no file name", new.display()),
        )
    })?;
    let mut part = std::ffi::OsString::from(".");
    part.push(name);
    part.push(PART_SUFFIX);
    Ok(new.with_file_name(part))
}

/// Copies `old` to `part` and compares the two. Only a folder, a file or a link can move.
fn copy_and_compare(old: &Path, part: &Path) -> io::Result<()> {
    let kind = fs::symlink_metadata(old)?.file_type();
    if !(kind.is_dir() || kind.is_file() || kind.is_symlink()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} is not a folder, a file or a link", old.display()),
        ));
    }
    copy_entry(old, part, kind).map_err(|error| {
        context(
            &error,
            &format!("cannot copy {} to {}", old.display(), part.display()),
        )
    })?;
    same_entry(old, part, kind)
}

/// Copies one entry of the kind `kind`. A socket or a FIFO stays behind: its program makes it
/// again.
fn copy_entry(from: &Path, to: &Path, kind: FileType) -> io::Result<()> {
    if kind.is_dir() {
        copy_tree(from, to)
    } else if kind.is_symlink() {
        std::os::unix::fs::symlink(fs::read_link(from)?, to)
    } else if kind.is_file() {
        copy_file(from, to)
    } else {
        Ok(())
    }
}

/// Copies the folder `from` to `to` with its mode and all its entries.
fn copy_tree(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        copy_entry(
            &entry.path(),
            &to.join(entry.file_name()),
            entry.file_type()?,
        )?;
    }
    // The mode comes last, so that a folder without write access still gets its entries.
    fs::set_permissions(to, fs::symlink_metadata(from)?.permissions())
}

/// Copies one file with its mode and its times. A cache trim of an app reads the times.
fn copy_file(from: &Path, to: &Path) -> io::Result<()> {
    fs::copy(from, to)?;
    let metadata = fs::metadata(from)?;
    let times = fs::FileTimes::new()
        .set_accessed(metadata.accessed()?)
        .set_modified(metadata.modified()?);
    fs::File::open(to)?.set_times(times)
}

/// Whether the copy `to` of the entry `from` has the same kind, the same mode, the same bytes and
/// the same link targets. A difference is an `InvalidData` error that names the entry.
fn same_entry(from: &Path, to: &Path, kind: FileType) -> io::Result<()> {
    if kind.is_dir() {
        same_tree(from, to)
    } else if kind.is_symlink() {
        if fs::read_link(from)? == fs::read_link(to)? {
            Ok(())
        } else {
            Err(differs(from, "the link target"))
        }
    } else if kind.is_file() {
        if same_file(from, to)? {
            Ok(())
        } else {
            Err(differs(from, "the size, the mode or the bytes"))
        }
    } else {
        Ok(())
    }
}

/// `same_entry` for a folder and each entry in it.
fn same_tree(from: &Path, to: &Path) -> io::Result<()> {
    let copy = fs::symlink_metadata(to)?;
    if !copy.is_dir() || fs::symlink_metadata(from)?.permissions() != copy.permissions() {
        return Err(differs(from, "the kind or the mode"));
    }
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        same_entry(
            &entry.path(),
            &to.join(entry.file_name()),
            entry.file_type()?,
        )?;
    }
    Ok(())
}

/// The error for a copy that differs from the entry `path` in `what`.
fn differs(path: &Path, what: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("the copy of {} differs: {what}", path.display()),
    )
}

/// Whether the files `a` and `b` have the same mode and the same bytes.
fn same_file(a: &Path, b: &Path) -> io::Result<bool> {
    let (metadata_a, metadata_b) = (fs::symlink_metadata(a)?, fs::symlink_metadata(b)?);
    if !metadata_b.is_file()
        || metadata_a.len() != metadata_b.len()
        || metadata_a.permissions() != metadata_b.permissions()
    {
        return Ok(false);
    }
    let (mut file_a, mut file_b) = (fs::File::open(a)?, fs::File::open(b)?);
    let (mut buffer_a, mut buffer_b) = (vec![0; 1 << 16], vec![0; 1 << 16]);
    loop {
        let read_a = fill(&mut file_a, &mut buffer_a)?;
        let read_b = fill(&mut file_b, &mut buffer_b)?;
        if buffer_a.get(..read_a) != buffer_b.get(..read_b) {
            return Ok(false);
        }
        if read_a == 0 {
            return Ok(true);
        }
    }
}

/// Reads until `buffer` is full or the file ends, and returns the number of bytes.
fn fill(file: &mut fs::File, buffer: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while let Some(rest) = buffer.get_mut(filled..).filter(|rest| !rest.is_empty()) {
        match file.read(rest)? {
            0 => break,
            count => filled += count,
        }
    }
    Ok(filled)
}

/// Removes the folder, the file or the link at `path`. Nothing at `path` is no error.
fn remove_entry(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// `error` with the same kind and `text` before its text, so that the log names the path.
fn context(error: &io::Error, text: &str) -> io::Error {
    io::Error::new(error.kind(), format!("{text}: {error}"))
}
