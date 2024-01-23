extern crate time;

use std::thread::{spawn};

fn main() {
  let n = 100_000;
  let t0 = time::get_time();
  let _ = spawn(move || {});
  let t1 = time::get_time();
  for _ in 1 ..= n {
    let _ = spawn(move || {});
  }
  let t_ = time::get_time();
  let dt1 = (t_ - t1).to_std().unwrap();
  let dt1 = dt1.as_secs() as f64 + dt1.subsec_nanos() as f64 * 1.0e-9;
  println!("DEBUG:  n  ={}", n);
  println!("DEBUG:  dt1={:.09} s", dt1);
}
