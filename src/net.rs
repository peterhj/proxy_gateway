use unix2::{FdSet, select};

use std::io::{Error as IoError};
use std::os::unix::io::{AsRawFd};
use std::time::{Duration as StdDuration};

pub fn select_read_fd_timeout<F: AsRawFd>(fd: &F, timeout: StdDuration) -> Result<Option<()>, IoError> {
  let mut read = FdSet::new();
  let mut write = FdSet::new();
  let mut except = FdSet::new();
  read.insert(fd);
  let fd = fd.as_raw_fd();
  let end_fd = fd + 1;
  assert!(fd < end_fd);
  select(end_fd, &mut read, &mut write, &mut except, timeout)
}
