extern crate time;

fn bench_second() {
  let n = 1_000_000;
  let t0 = time::get_time();
  let _ = time::get_time_second();
  let t1 = time::get_time();
  let mut t_ = time::get_time_second();
  for _ in 2 .. n {
    t_ = time::get_time_second();
  }
  t_ = time::get_time();
  let dt1 = (t_ - t1).to_std().unwrap();
  let dt1 = dt1.as_secs() as f64 + dt1.subsec_nanos() as f64 * 1.0e-9;
  println!("DEBUG:  n  ={}", n);
  println!("DEBUG:  dt1={:.09} s", dt1);
}

fn bench_coarse() {
  let n = 1_000_000;
  let t0 = time::get_time();
  let _ = time::get_time_coarse();
  let t1 = time::get_time();
  let mut t_ = time::get_time_coarse();
  for _ in 2 .. n {
    t_ = time::get_time_coarse();
  }
  t_ = time::get_time();
  let dt1 = (t_ - t1).to_std().unwrap();
  let dt1 = dt1.as_secs() as f64 + dt1.subsec_nanos() as f64 * 1.0e-9;
  println!("DEBUG:  n  ={}", n);
  println!("DEBUG:  dt1={:.09} s", dt1);
}

fn bench_usec() {
  let n = 1_000_000;
  let t0 = time::get_time();
  let _ = time::get_time_usec();
  let t1 = time::get_time();
  let mut t_ = time::get_time_usec();
  for _ in 2 .. n {
    t_ = time::get_time_usec();
  }
  t_ = time::get_time();
  let dt1 = (t_ - t1).to_std().unwrap();
  let dt1 = dt1.as_secs() as f64 + dt1.subsec_nanos() as f64 * 1.0e-9;
  println!("DEBUG:  n  ={}", n);
  println!("DEBUG:  dt1={:.09} s", dt1);
}

fn bench() {
  let n = 1_000_000;
  let t0 = time::get_time();
  let t1 = time::get_time();
  let mut t_ = time::get_time();
  for _ in 3 ..= n {
    t_ = time::get_time();
  }
  let dt1 = (t_ - t1).to_std().unwrap();
  let dt1 = dt1.as_secs() as f64 + dt1.subsec_nanos() as f64 * 1.0e-9;
  println!("DEBUG:  n  ={}", n);
  println!("DEBUG:  dt1={:.09} s", dt1);
}

fn main() {
  bench_second();
  bench_coarse();
  bench_usec();
  bench();
}
