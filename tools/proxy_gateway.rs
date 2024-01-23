extern crate proxy_gateway;

fn main() {
  let mut config = proxy_gateway::Config::default();
  // FIXME: Replace the following with your root domain.
  config.set_primary_host("example.com");
  config.set_default_port(9000);
  config.service_main();
}
