use dynfmt::Format;
use log::{Log, Metadata, Record};
use once_cell::sync::OnceCell;
use std::fmt::{Display, Formatter};
use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::ops::DerefMut;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::sync::Mutex;

/// Logs records in the standard output stream
pub struct StdoutLogger;

/// Logs records in the provided file by path
pub struct FileLogger {
    file: Mutex<BufWriter<File>>,
}

/// Forces flushing buffered records to a destination while dropping
pub struct LogFlushGuard;

pub const fn make_stdout_logger() -> &'static impl Log {
    const LOGGER: StdoutLogger = StdoutLogger;
    &LOGGER
}

pub fn make_file_logger(path: &str) -> std::io::Result<&'static impl Log> {
    static LOGGER: OnceCell<FileLogger> = OnceCell::new();
    assert!(LOGGER.get().is_none());

    LOGGER.get_or_try_init(|| FileLogger::new(path))
}

fn write_record(mut w: impl Write, record: &Record) -> std::io::Result<()> {
    writeln!(
        w,
        "{} [{:?}] [{}] [{}] {}",
        chrono::Local::now().format("%T.%6f"),
        std::thread::current().id(),
        record.level(),
        record.target(),
        record.args(),
    )
}

impl Log for StdoutLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            write_record(std::io::stdout(), record).unwrap();
        }
    }

    fn flush(&self) {}
}

impl FileLogger {
    pub fn new(path: &str) -> std::io::Result<Self> {
        let mut options = OpenOptions::new();
        options.create(true).write(true);
        #[cfg(unix)]
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        let file = options.open(path)?;
        if !file.metadata()?.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "log path must be a regular file",
            ));
        }
        file.set_len(0)?;
        #[cfg(unix)]
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        Ok(Self {
            file: Mutex::new(BufWriter::new(file)),
        })
    }
}

impl Log for FileLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            if let Err(e) = write_record(self.file.lock().unwrap().deref_mut(), record) {
                eprintln!("Log write failure: {}", e);
            }
        }
    }

    fn flush(&self) {
        if let Err(e) = self.file.lock().unwrap().flush() {
            eprintln!("Log flush failure: {}", e);
        }
    }
}

impl Drop for FileLogger {
    fn drop(&mut self) {
        self.flush();
    }
}

impl Drop for LogFlushGuard {
    fn drop(&mut self) {
        log::logger().flush()
    }
}

#[cfg(all(test, unix))]
mod unix_tests {
    use super::FileLogger;
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::{symlink, FileTypeExt, OpenOptionsExt, PermissionsExt};

    #[test]
    fn file_logger_restricts_new_and_existing_files() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("endpoint.log");
        std::fs::write(&path, "old log").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666)).unwrap();

        let logger = FileLogger::new(path.to_str().unwrap()).unwrap();
        drop(logger);

        assert_eq!(
            0o600,
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777
        );
    }

    #[test]
    fn file_logger_refuses_symbolic_links() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("target.log");
        let link = directory.path().join("endpoint.log");
        std::fs::write(&target, "keep").unwrap();
        symlink(&target, &link).unwrap();

        assert!(FileLogger::new(link.to_str().unwrap()).is_err());
        assert_eq!("keep", std::fs::read_to_string(target).unwrap());
    }

    #[test]
    fn file_logger_refuses_non_regular_files() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("endpoint.log");
        let c_path = CString::new(path.as_os_str().as_bytes()).unwrap();
        assert_eq!(0, unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) });
        let _reader = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&path)
            .unwrap();

        let error = FileLogger::new(path.to_str().unwrap()).err().unwrap();

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
        assert!(std::fs::symlink_metadata(path)
            .unwrap()
            .file_type()
            .is_fifo());
    }
}

#[macro_export]
macro_rules! log_id {
    ($lvl:ident, $id_chain:expr, $msg:expr) => {
        $lvl!(std::concat!("[{}] ", $msg), $id_chain)
    };
    ($lvl:ident, $id_chain:expr, $fmt:expr, $($arg:tt)*) => {
        $lvl!(std::concat!("[{}] ", $fmt), $id_chain, $($arg)*)
    };
}

pub(crate) const CLIENT_ID_FMT: &str = "CLIENT={}";
pub(crate) const TUNNEL_ID_FMT: &str = "TUN={}";
pub(crate) const CONNECTION_ID_FMT: &str = "CONN={}";

#[derive(Copy, Clone)]
pub struct IdItem<T: Copy + serde::ser::Serialize> {
    fmt: &'static str,
    id: T,
}

#[derive(Clone)]
pub struct IdChain<T: Copy + serde::ser::Serialize> {
    list: Vec<IdItem<T>>,
}

impl<T: Copy + serde::ser::Serialize> IdItem<T> {
    pub fn new(fmt: &'static str, id: T) -> Self {
        Self { fmt, id }
    }
}

impl<T: Copy + serde::ser::Serialize> IdChain<T> {
    pub fn empty() -> Self {
        Self {
            list: Default::default(),
        }
    }

    pub fn extended(&self, new: IdItem<T>) -> Self {
        let mut x = Self::with_capacity(self.list.len() + 1);
        x.list.extend(self.list.iter());
        x.list.push(new);
        x
    }

    fn with_capacity(cap: usize) -> Self {
        Self {
            list: Vec::with_capacity(cap),
        }
    }
}

impl<T: Copy + serde::ser::Serialize> From<IdItem<T>> for IdChain<T> {
    fn from(x: IdItem<T>) -> Self {
        Self { list: vec![x] }
    }
}

impl<T: Copy + serde::ser::Serialize> Display for IdChain<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let str = self.list.iter().fold(String::new(), |acc, i| {
            let x = dynfmt::curly::SimpleCurlyFormat
                .format(i.fmt, [i.id])
                .unwrap();

            if !acc.is_empty() {
                acc + "/" + x.as_ref()
            } else {
                x.to_string()
            }
        });
        write!(f, "{}", str)
    }
}

#[cfg(test)]
mod tests {
    use crate::log_utils::{IdChain, IdItem};

    #[test]
    fn test() {
        let mut chain = IdChain::from(IdItem::new("hello {}", 42));
        assert_eq!("hello 42", format!("{}", chain));

        chain = chain.extended(IdItem::new("ok {}", 73));
        assert_eq!("hello 42/ok 73", format!("{}", chain));
    }
}
