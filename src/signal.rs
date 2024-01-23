pub use service_base::signal::{signals};
use service_base::signal::*;

pub fn init_signals() {
  let mut cfg = SignalsConfigOnce::default();
  cfg.hup = true;
  cfg.int_ = true;
  cfg.term = true;
  //cfg.quit = true;
  cfg.init();
}
