// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Platform-specific code (STRUCT-10): positioned reads (`pread`-style) on every supported OS.

use std::fs::File;
use std::io;

/// Fills `buf` from `file` starting at byte `offset`, without using a shared cursor on Unix.
/// On Windows the file cursor moves; callers never mix this with cursor-based reads.
pub(crate) fn read_exact_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::FileExt::read_exact_at(file, buf, offset)
    }
    #[cfg(windows)]
    {
        let mut done = 0;
        while done < buf.len() {
            let n = std::os::windows::fs::FileExt::seek_read(
                file,
                &mut buf[done..],
                offset + done as u64,
            )?;
            if n == 0 {
                return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
            }
            done += n;
        }
        Ok(())
    }
}
