pub use service_base::daemon::{protect};

use std::env::{set_current_dir};
use std::fs::{create_dir_all};
use std::io::{Error as IoError};
use std::os::unix::fs::{chroot};
use std::path::{Path};
use std::process::{Command};

pub fn mount<P: AsRef<Path>>(chroot_dir: P) -> Result<(), IoError> {
  let chroot_dir = chroot_dir.as_ref().to_owned();
  let mount = |suffix| -> Result<(), IoError> {
    let res = Command::new("mount")
      //.arg("--bind")
      .arg("-o").arg("bind,ro")
      //.arg("-o").arg("remount,bind,ro")
      .arg(suffix)
      .arg(chroot_dir.join(suffix))
      .status().ok().unwrap();
    assert!(res.success());
    Ok(())
  };
  // TODO: any other paths to bind mount?
  mount("/dev/random")?;
  mount("/dev/urandom")?;
  mount("/etc/ssl")?;
  Ok(())
}

/*pub fn mkdir<P: AsRef<Path>>(chroot_dir: P) -> Result<(), IoError> {
  let chroot_dir = chroot_dir.as_ref().to_owned();
  // TODO: do not hide the following error.
  create_dir_all(&chroot_dir.join("var/tmp/acme")).ok();
  Ok(())
}*/

pub fn mkdir() -> Result<(), IoError> {
  // TODO: do not hide following errors.
  create_dir_all("/var/tmp/acme-staging").ok();
  create_dir_all("/var/tmp/acme").ok();
  Ok(())
}
