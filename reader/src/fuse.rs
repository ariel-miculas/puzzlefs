extern crate time;

use std::convert::TryInto;
use std::ffi::OsStr;
use std::os::raw::c_int;
use std::path::Path;

use fuse::{FileAttr, FileType, Filesystem, ReplyData, ReplyEntry, ReplyOpen, Request};
use nix::errno::Errno;
use nix::fcntl::OFlag;
use time::Timespec;

use format::{Result, WireFormatError};

use super::puzzlefs::{file_read, Inode, InodeMode, PuzzleFS};

pub struct Fuse<'a> {
    pfs: PuzzleFS<'a>,
    // TODO: LRU cache inodes or something. I had problems fiddling with the borrow checker for the
    // cache, so for now we just do each lookup every time.
}

fn mode_to_fuse_type(inode: &Inode) -> Result<FileType> {
    Ok(match inode.mode {
        InodeMode::File { .. } => FileType::RegularFile,
        InodeMode::Dir { .. } => FileType::Directory,
        InodeMode::Other => match inode.inode.mode {
            format::InodeMode::Fifo => FileType::NamedPipe,
            format::InodeMode::Chr { .. } => FileType::CharDevice,
            format::InodeMode::Blk { .. } => FileType::BlockDevice,
            format::InodeMode::Lnk => FileType::Symlink,
            format::InodeMode::Sock => FileType::Socket,
            _ => return Err(WireFormatError::from_errno(Errno::EINVAL)),
        },
    })
}

impl<'a> Fuse<'a> {
    pub fn new(pfs: PuzzleFS<'a>) -> Fuse<'a> {
        Fuse { pfs }
    }

    fn _lookup(&mut self, parent: u64, name: &OsStr) -> Result<FileAttr> {
        let dir = self.pfs.find_inode(parent)?;
        let ino = dir.dir_lookup(name)?;
        self._getattr(ino)
    }

    fn _getattr(&mut self, ino: u64) -> Result<FileAttr> {
        let ic = self.pfs.find_inode(ino)?;
        let kind = mode_to_fuse_type(&ic)?;
        let len = ic.file_len().unwrap_or(0);
        Ok(FileAttr {
            ino: ic.inode.ino,
            size: len,
            blocks: 0,
            atime: time::Timespec::new(0, 0),
            mtime: time::Timespec::new(0, 0),
            ctime: time::Timespec::new(0, 0),
            crtime: time::Timespec::new(0, 0),
            kind,
            perm: 0o644,
            nlink: 0,
            uid: ic.inode.uid,
            gid: ic.inode.gid,
            rdev: 0,
            flags: 0,
        })
    }

    fn _open(&self, flags_i: u32, reply: ReplyOpen) {
        let allowed_flags =
            OFlag::O_RDONLY | OFlag::O_PATH | OFlag::O_NONBLOCK | OFlag::O_DIRECTORY;
        let flags = OFlag::from_bits_truncate(flags_i.try_into().unwrap());
        if !allowed_flags.contains(flags) {
            reply.error(Errno::EROFS as i32)
        } else {
            // stateless open for now, slower maybe
            reply.opened(0, flags_i);
        }
    }

    fn _read(&mut self, ino: u64, offset: u64, size: u32) -> Result<Vec<u8>> {
        let inode = self.pfs.find_inode(ino)?;
        let mut buf = vec![0_u8; size as usize];
        let read = file_read(self.pfs.oci, &inode, offset as usize, &mut buf)?;
        buf.truncate(read);
        Ok(buf)
    }

    fn _readdir(&mut self, ino: u64, offset: i64, reply: &mut fuse::ReplyDirectory) -> Result<()> {
        let inode = self.pfs.find_inode(ino)?;
        let entries = inode.dir_entries()?;
        for (index, (name, ino_r)) in entries.iter().enumerate().skip(offset as usize) {
            let ino = *ino_r;
            let inode = self.pfs.find_inode(ino)?;
            let kind = mode_to_fuse_type(&inode)?;

            // if the buffer is full, let's skip the extra lookups
            if reply.add(ino, (index + 1) as i64, kind, name) {
                break;
            }
        }

        Ok(())
    }
}

impl Filesystem for Fuse<'_> {
    fn init(&mut self, _req: &Request) -> std::result::Result<(), c_int> {
        Ok(())
    }

    fn destroy(&mut self, _req: &Request) {}
    fn forget(&mut self, _req: &Request, _ino: u64, _nlookup: u64) {}

    // puzzlefs is readonly, so we can ignore a bunch of requests
    fn setattr(
        &mut self,
        _req: &Request,
        _ino: u64,
        _mode: Option<u32>,
        _uid: Option<u32>,
        _gid: Option<u32>,
        _size: Option<u64>,
        _atime: Option<Timespec>,
        _mtime: Option<Timespec>,
        _fh: Option<u64>,
        _crtime: Option<Timespec>,
        _chgtime: Option<Timespec>,
        _bkuptime: Option<Timespec>,
        _flags: Option<u32>,
        reply: fuse::ReplyAttr,
    ) {
        reply.error(Errno::EROFS as i32)
    }

    fn mknod(
        &mut self,
        _req: &Request,
        _parent: u64,
        _name: &OsStr,
        _mode: u32,
        _rdev: u32,
        reply: ReplyEntry,
    ) {
        reply.error(Errno::EROFS as i32)
    }

    fn mkdir(
        &mut self,
        _req: &Request,
        _parent: u64,
        _name: &OsStr,
        _mode: u32,
        reply: ReplyEntry,
    ) {
        reply.error(Errno::EROFS as i32)
    }

    fn unlink(&mut self, _req: &Request, _parent: u64, _name: &OsStr, reply: fuse::ReplyEmpty) {
        reply.error(Errno::EROFS as i32)
    }

    fn rmdir(&mut self, _req: &Request, _parent: u64, _name: &OsStr, reply: fuse::ReplyEmpty) {
        reply.error(Errno::EROFS as i32)
    }

    fn symlink(
        &mut self,
        _req: &Request,
        _parent: u64,
        _name: &OsStr,
        _link: &Path,
        reply: ReplyEntry,
    ) {
        reply.error(Errno::EROFS as i32)
    }

    fn rename(
        &mut self,
        _req: &Request,
        _parent: u64,
        _name: &OsStr,
        _newparent: u64,
        _newname: &OsStr,
        reply: fuse::ReplyEmpty,
    ) {
        reply.error(Errno::EROFS as i32)
    }

    fn link(
        &mut self,
        _req: &Request,
        _ino: u64,
        _newparent: u64,
        _newname: &OsStr,
        reply: ReplyEntry,
    ) {
        reply.error(Errno::EROFS as i32)
    }

    fn write(
        &mut self,
        _req: &Request,
        _ino: u64,
        _fh: u64,
        _offset: i64,
        _data: &[u8],
        _flags: u32,
        reply: fuse::ReplyWrite,
    ) {
        reply.error(Errno::EROFS as i32)
    }

    fn flush(
        &mut self,
        _req: &Request,
        _ino: u64,
        _fh: u64,
        _lock_owner: u64,
        reply: fuse::ReplyEmpty,
    ) {
        reply.error(Errno::EROFS as i32)
    }

    fn fsync(
        &mut self,
        _req: &Request,
        _ino: u64,
        _fh: u64,
        _datasync: bool,
        reply: fuse::ReplyEmpty,
    ) {
        reply.error(Errno::EROFS as i32)
    }

    fn fsyncdir(
        &mut self,
        _req: &Request,
        _ino: u64,
        _fh: u64,
        _datasync: bool,
        reply: fuse::ReplyEmpty,
    ) {
        reply.error(Errno::EROFS as i32)
    }

    fn setxattr(
        &mut self,
        _req: &Request,
        _ino: u64,
        _name: &OsStr,
        _value: &[u8],
        _flags: u32,
        _position: u32,
        reply: fuse::ReplyEmpty,
    ) {
        reply.error(Errno::EROFS as i32)
    }

    fn removexattr(&mut self, _req: &Request, _ino: u64, _name: &OsStr, reply: fuse::ReplyEmpty) {
        reply.error(Errno::EROFS as i32)
    }

    fn create(
        &mut self,
        _req: &Request,
        _parent: u64,
        _name: &OsStr,
        _mode: u32,
        _flags: u32,
        reply: fuse::ReplyCreate,
    ) {
        reply.error(Errno::EROFS as i32)
    }

    fn getlk(
        &mut self,
        _req: &Request,
        _ino: u64,
        _fh: u64,
        _lock_owner: u64,
        _start: u64,
        _end: u64,
        _typ: u32,
        _pid: u32,
        reply: fuse::ReplyLock,
    ) {
        reply.error(Errno::EROFS as i32)
    }

    fn setlk(
        &mut self,
        _req: &Request,
        _ino: u64,
        _fh: u64,
        _lock_owner: u64,
        _start: u64,
        _end: u64,
        _typ: u32,
        _pid: u32,
        _sleep: bool,
        reply: fuse::ReplyEmpty,
    ) {
        reply.error(Errno::EROFS as i32)
    }

    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        match self._lookup(parent, name) {
            Ok(attr) => {
                // http://libfuse.github.io/doxygen/structfuse__entry__param.html
                let ttl = Timespec::new(std::i64::MAX, 0);
                let generation = 0;
                reply.entry(&ttl, &attr, generation)
            }
            Err(e) => reply.error(e.to_errno()),
        }
    }

    fn getattr(&mut self, _req: &Request, ino: u64, reply: fuse::ReplyAttr) {
        match self._getattr(ino) {
            Ok(attr) => {
                // http://libfuse.github.io/doxygen/structfuse__entry__param.html
                let ttl = Timespec::new(std::i64::MAX, 0);
                reply.attr(&ttl, &attr)
            }
            Err(e) => reply.error(e.to_errno()),
        }
    }

    fn readlink(&mut self, _req: &Request, _ino: u64, reply: ReplyData) {
        reply.error(Errno::EISNAM as i32)
    }

    fn open(&mut self, _req: &Request, _ino: u64, flags: u32, reply: ReplyOpen) {
        self._open(flags, reply)
    }

    fn read(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        size: u32,
        reply: ReplyData,
    ) {
        // TODO: why i64 from the fuse API here?
        let uoffset: u64 = offset.try_into().unwrap();
        match self._read(ino, uoffset, size) {
            Ok(data) => reply.data(data.as_slice()),
            Err(e) => reply.error(e.to_errno()),
        }
    }

    fn release(
        &mut self,
        _req: &Request,
        _ino: u64,
        _fh: u64,
        _flags: u32,
        _lock_owner: u64,
        _flush: bool,
        reply: fuse::ReplyEmpty,
    ) {
        // TODO: purge from our cache here? dcache should save us too...
        reply.ok()
    }

    fn opendir(&mut self, _req: &Request, _ino: u64, flags: u32, reply: ReplyOpen) {
        self._open(flags, reply)
    }

    fn readdir(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: fuse::ReplyDirectory,
    ) {
        match self._readdir(ino, offset, &mut reply) {
            Ok(_) => reply.ok(),
            Err(e) => reply.error(e.to_errno()),
        }
    }

    fn releasedir(
        &mut self,
        _req: &Request,
        _ino: u64,
        _fh: u64,
        _flags: u32,
        reply: fuse::ReplyEmpty,
    ) {
        // TODO: again maybe purge from cache?
        reply.ok()
    }

    fn statfs(&mut self, _req: &Request, _ino: u64, reply: fuse::ReplyStatfs) {
        reply.statfs(
            0,   // blocks
            0,   // bfree
            0,   // bavail
            0,   // files
            0,   // ffree
            0,   // bsize
            256, // namelen
            0,   // frsize
        )
    }

    fn getxattr(
        &mut self,
        _req: &Request,
        _ino: u64,
        _name: &OsStr,
        _size: u32,
        reply: fuse::ReplyXattr,
    ) {
        // TODO: encoding for xattrs
        reply.error(Errno::ENOMEDIUM as i32)
    }

    fn listxattr(&mut self, _req: &Request, _ino: u64, _size: u32, reply: fuse::ReplyXattr) {
        reply.error(Errno::EDQUOT as i32)
    }

    fn access(&mut self, _req: &Request, _ino: u64, _mask: u32, reply: fuse::ReplyEmpty) {
        reply.ok()
    }

    fn bmap(
        &mut self,
        _req: &Request,
        _ino: u64,
        _blocksize: u32,
        _idx: u64,
        reply: fuse::ReplyBmap,
    ) {
        reply.error(Errno::ENOLCK as i32)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::Instant;
    use std::{fs, fs::File};
    use std::{io, io::Read};
    use walkdir::WalkDir;

    extern crate hex;
    use sha2::{Digest, Sha256};
    use tempfile::tempdir;

    use builder::build_test_fs;
    use oci::Image;

    #[test]
    fn test_fuse() {
        let dir = tempdir().unwrap();
        let image = Image::new(dir.path()).unwrap();
        let rootfs_desc = build_test_fs(Path::new("../builder/test"), &image).unwrap();
        image.add_tag("test".to_string(), rootfs_desc).unwrap();
        let mountpoint = tempdir().unwrap();
        let _bg = crate::mount(&image, "test", Path::new(mountpoint.path())).unwrap();
        let ents = fs::read_dir(mountpoint.path())
            .unwrap()
            .collect::<io::Result<Vec<fs::DirEntry>>>()
            .unwrap();
        assert_eq!(ents.len(), 1);
        assert_eq!(
            ents[0].path().strip_prefix(mountpoint.path()).unwrap(),
            Path::new("SekienAkashita.jpg")
        );

        let mut hasher = Sha256::new();
        let mut f = File::open(ents[0].path()).unwrap();
        io::copy(&mut f, &mut hasher).unwrap();
        let digest = hasher.finalize();
        const FILE_DIGEST: &str =
            "d9e749d9367fc908876749d6502eb212fee88c9a94892fb07da5ef3ba8bc39ed";
        assert_eq!(hex::encode(digest), FILE_DIGEST);
    }

    #[test]
    fn test_fuse_read() {
        let dir = tempdir().unwrap();
        let image = Image::new(dir.path()).unwrap();
        let original_path =
            Path::new("/home/amiculas/work/cisco/test-puzzlefs/real_rootfs/barehost/rootfs");
        let rootfs_desc = build_test_fs(original_path, &image).unwrap();
        image.add_tag("test".to_string(), rootfs_desc).unwrap();
        let mountpoint = tempdir().unwrap();

        // cannot use filter_entry because the iterator will not descend into the filtered directories
        let ents = WalkDir::new(original_path)
            .contents_first(false)
            .follow_links(false)
            .same_file_system(true)
            .sort_by(|a, b| a.file_name().cmp(b.file_name()))
            .into_iter()
            .filter(|de| {
                de.as_ref()
                    .unwrap()
                    .metadata()
                    .map(|md| md.is_file())
                    .unwrap_or(true)
            })
            .collect::<Result<Vec<walkdir::DirEntry>, walkdir::Error>>()
            .unwrap();

        for ent in &ents {
            let _bg = crate::mount(&image, "test", Path::new(mountpoint.path())).unwrap();
            let new_path =
                Path::new(mountpoint.path()).join(ent.path().strip_prefix(original_path).unwrap());
            let mut buffer = [0; 1];

            let now = Instant::now();

            let mut f = fs::File::open(&new_path).unwrap();
            f.read(&mut buffer).unwrap();
            let elapsed = now.elapsed();
            println!("file {}, Elapsed: {:.2?}", new_path.display(), elapsed);
        }
    }
}
