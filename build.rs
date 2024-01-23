extern crate time;

use time::{get_time};

use std::fs::{OpenOptions};
use std::io::{BufRead, Write, Cursor};
use std::path::{PathBuf};
use std::process::{Command};

fn main() {
  let t = get_time();
  println!("cargo:rerun-if-changed=build.rs");
  println!("cargo:rerun-if-changed=.git/logs/HEAD");
  let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
  let mut tsp_f = OpenOptions::new().write(true).create(true).truncate(true)
    .open(out_dir.join("timestamp")).unwrap();
  write!(&mut tsp_f, "{}", t.utc().rfc3339()).unwrap();
  let res = Command::new("git")
    .current_dir(std::env::var("CARGO_MANIFEST_DIR").unwrap())
    .arg("log").arg("-n").arg("1").arg("--format=%H")
    .output().unwrap();
  assert!(res.status.success());
  let line = Cursor::new(res.stdout).lines().next().unwrap();
  let line = line.unwrap();
  let mut gch_f = OpenOptions::new().write(true).create(true).truncate(true)
    .open(out_dir.join("git_commit_hash")).unwrap();
  write!(&mut gch_f, "{}", line).unwrap();
}
