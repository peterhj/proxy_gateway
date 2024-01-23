#![forbid(unsafe_code)]

extern crate http1;
extern crate native_tls;
extern crate service_base;
extern crate signal_hook;
extern crate smol_str;
extern crate time;
extern crate uacme;
extern crate unix2;

use native_tls::{TlsAcceptor, TlsStream, MidHandshakeTlsStream};
use service_base::prelude::*;
use service_base::chan::*;
use service_base::route::*;
use smol_str::{SmolStr};
use time::{Duration, Timespec, get_time_coarse, get_time_usec};

use std::cmp::{max};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::convert::{TryInto};
use std::env;
use std::fs::*;
use std::io::{Error as IoError, Cursor, BufWriter, Read, Write};
use std::mem::{replace};
use std::net::{ToSocketAddrs, TcpListener, TcpStream};
use std::os::unix::io::{AsRawFd, RawFd};
use std::path::{PathBuf};
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{Sender, SyncSender, Receiver, channel, sync_channel};
use std::thread::{sleep, spawn};
use std::time::{Duration as StdDuration};

pub mod acme;
pub mod build;
pub mod daemon;
pub mod net;
pub mod signal;

pub type Config = ProxyGatewayConfig;

#[derive(Clone, Default)]
pub struct ProxyGatewayConfig {
  allports: BTreeSet<u16>,
  invhosts: BTreeMap<u16, BTreeSet<SmolStr>>,
  hostport: BTreeMap<SmolStr, u16>,
  primhost: Option<SmolStr>,
  def_port: Option<u16>,
}

impl ProxyGatewayConfig {
  pub fn map_host_to_port<S: AsRef<str>>(&mut self, host: S, port: u16) {
    let host = host.as_ref();
    if port & 1 != 0 {
      println!("ERROR:  ProxyGatewayConfig::map_host_to_port: port = {} must be even (host = {:?})", port, host);
      panic!();
    }
    self.allports.insert(port);
    match self.invhosts.get_mut(&port) {
      None => {
        let mut hs = BTreeSet::new();
        hs.insert(host.into());
        self.invhosts.insert(port, hs);
      }
      Some(hs) => {
        hs.insert(host.into());
      }
    }
    if self.primhost.is_none() {
      println!("INFO:   ProxyGatewayConfig::map_host_to_port: new primary host = {:?}", host);
      self.primhost = Some(host.into());
    }
    self.hostport.insert(host.into(), port);
  }

  pub fn set_primary_host<S: AsRef<str>>(&mut self, host: S) {
    let host = host.as_ref();
    println!("INFO:   ProxyGatewayConfig::set_primary_host: host = {:?}", host);
    self.primhost = Some(host.into());
  }

  pub fn set_default_port(&mut self, port: u16) {
    if port & 1 != 0 {
      println!("ERROR:  ProxyGatewayConfig::set_default_port: port = {} must be even", port);
      panic!();
    }
    self.allports.insert(port);
    self.def_port = Some(port);
  }

  pub fn service_main(self) {
    crate::service_main(self)
  }
}

pub fn service_main(config: Config) {
  let t0 = get_time_usec();
  println!("INFO:   proxy_gateway::service_main: build: {}.{}", crate::build::timestamp(), crate::build::digest());
  println!("INFO:   proxy_gateway::service_main: startup: {}", t0.utc().rfc3339_nsec());
  crate::signal::init_signals();
  let host = "127.0.0.1";
  //let port80: u16 = 80;
  let port443: u16 = 443;
  // TODO: the initial setup should spin for some duration
  // before sleeping.
  /*let mut first = Some(());
  let bind80 = loop {
    if first.take().is_none() {
      sleep(StdDuration::from_secs(2));
    }
    match TcpListener::bind((host, port80)) {
      Err(_) => {
        continue;
      }
      Ok(bind) => {
        println!("INFO:   proxy_gateway::service_main: listening on {}:{}", host, port80);
        break bind;
      }
    }
  };*/
  let mut bind_ct = 0;
  let bind443 = loop {
    if bind_ct >= 50 {
      sleep(StdDuration::from_secs(2));
    } else if bind_ct > 0 {
      sleep(StdDuration::from_millis(100));
    }
    bind_ct += 1;
    match TcpListener::bind((host, port443)) {
      Err(_) => {
        continue;
      }
      Ok(bind) => {
        println!("INFO:   proxy_gateway::service_main: listening on {}:{}", host, port443);
        println!("DEBUG:  proxy_gateway::service_main:   bind ct = {}", bind_ct);
        bind.set_nonblocking(true).unwrap();
        println!("DEBUG:  proxy_gateway::service_main:   set nonblocking");
        break bind;
      }
    }
  };
  // TODO: do openssl-related setup before chroot.
  let chroot_dir = "/var/lib/proxy_gateway/new_root";
  //crate::daemon::mount(chroot_dir).unwrap();
  crate::daemon::protect(chroot_dir, 297, 297).unwrap();
  crate::daemon::mkdir().unwrap();
  // TODO TODO
  let config = Arc::new(config);
  let context = Context::new();
  /*let cfg = config.clone();
  let ctx = context.clone();
  let bind = bind80.try_clone().unwrap();
  let th80 = spawn(move || gateway80(cfg, ctx, bind));*/
  let cfg = config;
  let ctx = context;
  let bind = bind443.try_clone().unwrap();
  let th443 = spawn(move || gateway443(cfg, ctx, bind));
  /*th80.join().unwrap();*/
  th443.join().unwrap();
  //println!("INFO:   proxy_gateway::service_main: hup: received");
  // NB: small delay after HUP and before unbind.
  sleep(StdDuration::from_secs(1));
  drop(bind443);
  let t_hup = get_time_usec();
  println!("INFO:   proxy_gateway::service_main: hup: done: {}", t_hup.utc().rfc3339_nsec());
  loop {
    let sig = crate::signal::signals();
    if sig.get_int() || sig.get_term() {
      break;
    }
    sleep(StdDuration::from_secs(1));
  }
  let t_end = get_time_usec();
  println!("INFO:   proxy_gateway::service_main: shutdown: done: {}", t_end.utc().rfc3339_nsec());
}

pub fn safe_ascii(s: &[u8]) -> SmolStr {
  let mut buf = String::new();
  for &x in s.iter() {
    if x <= 0x20 {
      buf.push(' ');
    } else if x < 0x7f {
      buf.push(x.try_into().unwrap());
    } else {
      buf.push('?');
    }
  }
  buf.into()
}

#[derive(Clone)]
pub struct Context {
  pub router: Arc<Mutex<Router>>,
}

impl Context {
  pub fn new() -> Context {
    let router = Router::new();
    Context{
      router: Arc::new(Mutex::new(router)),
    }
  }
}

pub fn gateway80(_config: Arc<Config>, ctx: Context, bind: TcpListener) -> () {
  let base_url = http1::Url::parse("http://127.0.0.1").unwrap();
  ctx.router.lock().unwrap()
    .insert_get((), Box::new(|_, _, _| {
      ok().with_payload_str_mime("Hello world!\n", http1::Mime::TextHtml).into()
    }));
  ctx.router.lock().unwrap()
    .insert_get("about", Box::new(|_, _, _| {
      ok().with_payload_str_mime("It&rsquo;s about time.\n", http1::Mime::TextHtml).into()
    }));
  /*let acme_ctx = ctx.clone();
  let domain: SmolStr = match config.primhost.as_ref() {
    None => {
      println!("ERROR:  tls: not configured with primary host");
      return;
    }
    Some(s) => s.into()
  };
  let alt_domains: Vec<SmolStr> = config.hostport.iter().filter(|(s, _)| s != &domain).map(|(s, _)| s.into()).collect();
  let tls_identity = crate::acme::Acme::identity(&domain, acme_ctx);
  //let tls_identity = crate::acme::Acme::fresh_identity(&domain, &alt_domains as &[_], acme_ctx);
  if let Err(e) = tls_identity {
    println!("INFO:   tls: error initializing identity: {:?}", e);
  } else {
    println!("INFO:   tls: ok");
  }*/
  // TODO
  let mut seq_nr = 0;
  loop {
    match bind.accept() {
      Err(_) => {
      }
      Ok((mut stream, addr)) => {
        seq_nr += 1;
        println!("INFO:   accepted {}: {:?}", seq_nr, addr);
        let mut rbuf = Vec::new();
        rbuf.resize(8192, 0);
        match stream.read(&mut rbuf) {
          Err(e) => {
            println!("INFO:       read error: {:?}", e);
            continue;
          }
          Ok(n) => {
            println!("INFO:       read {} bytes", n);
            println!("INFO:         buf={:?}", safe_ascii(&rbuf[ .. n]));
            let mut parser = http1::RequestParser::new((&rbuf[ .. n]).iter().map(|&x| x));
            let mut req = http1::Request::default();
            if let Err(e) = parser.parse_first_line(&base_url, &mut req) {
              println!("INFO:       invalid first line: {:?}", e);
              continue;
            }
            if let Err(e) = parser.parse_headers(&mut req) {
              println!("INFO:       invalid headers: {:?}", e);
              continue;
            }
            println!("INFO:       valid request");
            // TODO: payload.
            let req = match HttpRequest::try_from_raw_strip_headers(req) {
              Err(_) => {
                println!("INFO:       request conversion failure");
                continue;
              }
              Ok((req, _)) => req
            };
            let port = 80;
            match ctx.router.lock().unwrap().match_(port, &req) {
              Err(_) => {
                println!("INFO:       match error");
                continue;
              }
              Ok(None) => {
                println!("INFO:       no match");
                //continue;
                let rep = HttpResponse::not_found();
                let rep = rep.to_raw();
                let mut buf = BufWriter::new(&mut stream);
                rep.encode(&mut buf).unwrap();
                buf.flush().unwrap();
                println!("INFO:       write done");
              }
              Ok(Some(rep)) => {
                println!("INFO:       matched response");
                let mut rep = rep.to_raw();
                let mut buf = BufWriter::new(&mut stream);
                rep.encode(&mut buf).unwrap();
                buf.flush().unwrap();
                println!("INFO:       write done");
              }
            }
            /*
            match req.url.as_ref() {
              None => {
                println!("INFO:       invalid url");
                continue;
              }
              Some(url) => {
                let port = 80;
                let method = req.method.unwrap();
                match ctx.router.lock().unwrap().match_url(port, method, url) {
                  Err(_) => {
                    println!("INFO:       match error");
                    continue;
                  }
                  Ok(None) => {
                    println!("INFO:       no match");
                    //continue;
                    let rep = HttpResponse::not_found();
                    let rep = rep.to_raw();
                    let mut buf = BufWriter::new(&mut stream);
                    rep.encode(&mut buf).unwrap();
                    buf.flush().unwrap();
                    println!("INFO:       write done");
                  }
                  Ok(Some(rep)) => {
                    println!("INFO:       matched response");
                    let mut rep = rep.to_raw();
                    /*// FIXME: following should go in `to_raw`.
                    if let Some(buf) = rep.payload.as_ref() {
                      let len = buf.len();
                      //rep.headers.push((http1::HeaderName::ContentLength, format!("{}", len).into()).into());
                      rep.push_header(http1::HeaderName::ContentLength, format!("{}", len).into_bytes());
                      //rep.headers.push((http1::HeaderName::ContentLength, len));
                      //rep.headers.push((HeaderName::ContentType, _));
                      //rep.headers.push((HeaderName::ContentEncoding, _));
                    }*/
                    let mut buf = BufWriter::new(&mut stream);
                    rep.encode(&mut buf).unwrap();
                    buf.flush().unwrap();
                    println!("INFO:       write done");
                  }
                }
              }
            }
            */
          }
        }
        /*let mut buf = BufWriter::new(&mut stream);
        write!(&mut buf, "HTTP/1.1 200 OK\r\n").unwrap();
        write!(&mut buf, "Content-Length: 13\r\n").unwrap();
        write!(&mut buf, "Content-Type: text/html\r\n").unwrap();
        //write!(&mut buf, "Content-Encoding: utf-8\r\n").unwrap();
        write!(&mut buf, "\r\n").unwrap();
        write!(&mut buf, "Hello world!\n").unwrap();
        buf.flush().unwrap();
        println!("INFO:       write done");*/
      }
    }
  }
}

pub fn gateway443(config: Arc<Config>, ctx: Context, bind: TcpListener) -> () {
  let base_url = http1::Url::parse("http://127.0.0.1").unwrap();
  let acme_ctx = ctx;
  let domain: SmolStr = match config.primhost.as_ref() {
    None => {
      println!("ERROR:  tls: not configured with primary host");
      return;
    }
    Some(s) => s.into()
  };
  let tls_identity = match crate::acme::Acme::identity(&domain, acme_ctx) {
    Err(e) => {
      println!("INFO:   tls: error initializing identity: {:?}", e);
      return;
    }
    Ok(i) => {
      println!("INFO:   tls: identity: ok");
      i
    }
  };
  let tls_acceptor = match TlsAcceptor::new(tls_identity) {
    Err(e) => {
      println!("INFO:   tls: failed to create acceptor: {:?}", e);
      return;
    }
    Ok(a) => {
      println!("INFO:   tls: acceptor: ok");
      a
    }
  };
  let mut backends = BTreeMap::new();
  for &port in config.allports.iter() {
  let (front_tx, back_rx) = channel::<(Timespec, HttpRequest, SyncSender<Option<HttpResponse>>)>();
  let _ = spawn(move || {
    println!("INFO:   backend: start");
    let host = "127.0.0.1";
    let port_start = port;
    let port_fin = port + 1;
    let mut port = port_start;
    let mut retry: Option<(Timespec, HttpRequest, SyncSender<Option<HttpResponse>>)> = None;
    let mut first = Some(());
    'outer: loop {
      if first.take().is_none() {
        sleep(StdDuration::from_secs(2));
      }
      let addr = (host, port).to_socket_addrs().unwrap().next().unwrap();
      let stream = match TcpStream::connect_timeout(&addr, StdDuration::from_secs(2)) {
        Ok(stream) => stream,
        Err(_) => {
          //println!("DEBUG:  backend:   connect: failed: port={}", port);
          if port >= port_fin {
            port = port_start;
          } else {
            port += 1;
          }
          continue 'outer;
        }
      };
      let mut chan: Chan = Chan::new(stream);
      match chan.query(&Msg::OKQ) {
        Ok(Msg::OKR) => {}
        /*Ok(Msg::HUP) => {
          // TODO
        }*/
        _ => {
          //println!("DEBUG:  backend:   setup: failed: port={}", port);
          if port >= port_fin {
            port = port_start;
          } else {
            port += 1;
          }
          continue 'outer;
        }
      }
      println!("INFO:   backend: connected on {}:{}", host, port);
      if retry.is_some() {
        // FIXME: soft real-time.
        let t = get_time_coarse();
        for (t0, req, back_tx) in retry.take().into_iter() {
          if (t - t0) >= Duration::seconds(2) {
            continue;
          }
          let req = Msg::H1Q(req);
          let maybe_rep = match chan.query(&req) {
            Ok(Msg::Top) => None,
            Ok(Msg::H1P(rep)) => Some(rep),
            /*Ok(Msg::HUP) => {
              // TODO
            }*/
            _ => {
              println!("DEBUG:  backend:   query: retry failed");
              let req = match req {
                Msg::H1Q(req) => req,
                _ => unreachable!()
              };
              retry = Some((t0, req, back_tx));
              println!("INFO:   backend: disconnected");
              continue 'outer;
            }
          };
          match back_tx.send(maybe_rep) {
            Ok(_) => {}
            _ => {}
          }
        }
      }
      loop {
        match back_rx.recv() {
          Ok((t0, req, back_tx)) => {
            // FIXME: soft real-time.
            let t = get_time_coarse();
            if (t - t0) >= Duration::seconds(2) {
              continue;
            }
            let req = Msg::H1Q(req);
            let maybe_rep = match chan.query(&req) {
              Ok(Msg::Top) => None,
              Ok(Msg::H1P(rep)) => Some(rep),
              /*Ok(Msg::HUP) => {
                // TODO
              }*/
              _ => {
                println!("DEBUG:  backend:   query: failed");
                let req = match req {
                  Msg::H1Q(req) => req,
                  _ => unreachable!()
                };
                retry = Some((t0, req, back_tx));
                println!("INFO:   backend: disconnected");
                continue 'outer;
              }
            };
            match back_tx.send(maybe_rep) {
              Ok(_) => {}
              _ => {}
            }
          }
          _ => {
          }
        }
      }
      unreachable!();
    }
    println!("INFO:   backend: end");
  });
  backends.insert(port, Mutex::new(front_tx));
  }
  let backends = Arc::new(backends);
  let timeout = StdDuration::from_secs(2);
  let mut seq_nr = 0;
  loop {
    if crate::signal::signals().get_hup() {
      // FIXME: clean shutdown backend thread.
      break;
    }
    match crate::net::select_read_fd_timeout(&bind, timeout) {
      Err(_) |
      Ok(None) => {
        continue;
      }
      Ok(Some(_)) => {}
    }
    loop {
      let stream = match bind.accept() {
        Err(_) => {
          break;
        }
        Ok((stream, addr)) => {
          seq_nr += 1;
          println!("INFO:   accepted {}: {:?}", seq_nr, addr);
          stream
        }
      };
      // FIXME: set stream timeout.
      /*
      match stream.set_read_timeout(Some(Duration::from_secs(5))) {
      }
      match stream.set_write_timeout(Some(Duration::from_secs(5))) {
      }
      */
      let config = config.clone();
      let base_url = base_url.clone();
      let backends = backends.clone();
      let tls_acceptor = tls_acceptor.clone();
      let _ = spawn(move || {
        let mut stream = match tls_acceptor.accept(stream) {
          Err(e) => {
            println!("INFO:       tls: failed to accept: {:?}", e);
            return;
          }
          Ok(stream) => stream
        };
        println!("INFO:       tls: accepted");
        let rcap = 8192;
        let mut rbuf = Vec::new();
        rbuf.resize(rcap, 0);
        match stream.read(&mut rbuf) {
          Err(e) => {
            println!("INFO:       read error: {:?}", e);
            return;
          }
          Ok(r_sz) => {
            println!("INFO:       read {} bytes", r_sz);
            println!("INFO:         buf={:?}", safe_ascii(&rbuf[ .. r_sz]));
            let mut parser = http1::RequestParser::new((&rbuf[ .. r_sz]).iter().map(|&x| x));
            let mut req = http1::Request::default();
            if let Err(e) = parser.parse_first_line(&base_url, &mut req) {
              println!("INFO:       invalid first line: {:?}", e);
              return;
            }
            if let Err(e) = parser.parse_headers(&mut req) {
              println!("INFO:       invalid headers: {:?}", e);
              return;
            }
            let header_len = parser.pos();
            drop(parser);
            println!("INFO:       valid request header: len={}", header_len);
            let mut route_host: Option<SmolStr> = None;
            let mut payload_len = None;
            for h in req.headers.iter() {
              if route_host.is_some() &&
                 payload_len.is_some()
              {
                break;
              }
              match (h.name.as_ref(), h.value.as_ref()) {
                (Ok(&http1::HeaderName::Host), Ok(&http1::HeaderValue::Domain(ref host_s))) => {
                  println!("INFO:       valid host: {:?}", safe_ascii(host_s.as_bytes()));
                  if route_host.is_none() {
                    route_host = Some(host_s.into());
                  }
                }
                (Ok(&http1::HeaderName::ContentLength), Ok(&http1::HeaderValue::Length(len))) => {
                  if payload_len.is_none() {
                    payload_len = Some(len as usize);
                  }
                }
                _ => {}
              }
            }
            let mut route_port = if let Some(host_s) = route_host.as_ref() {
              config.hostport.get(host_s).map(|&port| port)
            } else {
              None
            };
            if route_port.is_none() {
              route_port = config.def_port
            };
            if route_port.is_none() {
              println!("INFO:       no route to host");
              let rep = HttpResponse::not_found();
              let rep = rep.to_raw();
              let mut buf = BufWriter::new(&mut stream);
              rep.encode(&mut buf).unwrap();
              buf.flush().unwrap();
              println!("INFO:       write done");
              return;
            }
            let route_port = route_port.unwrap();
            let payload_len = payload_len.unwrap_or(0);
            if payload_len <= 0 {
              println!("INFO:       no payload");
            } else {
              println!("INFO:       payload len={}", payload_len);
            }
            const MAX_PAYLOAD: usize = 8192;
            //const MAX_PAYLOAD: usize = 65536;
            if payload_len > MAX_PAYLOAD {
              println!("INFO:       payload too large");
              let rep = HttpResponse::from_status(HttpStatus::BadRequest);
              let rep = rep.to_raw();
              let mut buf = BufWriter::new(&mut stream);
              rep.encode(&mut buf).unwrap();
              buf.flush().unwrap();
              println!("INFO:       write done");
              return;
            } else if r_sz < header_len + payload_len {
              rbuf.resize(max(rcap, header_len + payload_len), 0);
              if let Err(e) = stream.read_exact(&mut rbuf[r_sz .. header_len + payload_len]) {
                println!("INFO:       payload read error: {:?}", e);
                let rep = HttpResponse::from_status(HttpStatus::BadRequest);
                let rep = rep.to_raw();
                let mut buf = BufWriter::new(&mut stream);
                rep.encode(&mut buf).unwrap();
                buf.flush().unwrap();
                println!("INFO:       write done");
                return;
              }
            }
            req.set_payload(&rbuf[header_len .. header_len + payload_len]);
            let req = match HttpRequest::try_from_raw_strip_headers(req) {
              Err(_) => {
                println!("INFO:       request conversion failure");
                let rep = HttpResponse::from_status(HttpStatus::BadRequest);
                let rep = rep.to_raw();
                let mut buf = BufWriter::new(&mut stream);
                rep.encode(&mut buf).unwrap();
                buf.flush().unwrap();
                println!("INFO:       write done");
                return;
              }
              Ok((req, _)) => req
            };
            let (back_tx, front_rx) = sync_channel(1);
            println!("INFO:       route to port = {:?}", route_port);
            let front_tx = match backends.get(&route_port) {
              None => {
                println!("INFO:       bug: no backend for port = {}", route_port);
                let rep = HttpResponse::not_found();
                let rep = rep.to_raw();
                let mut buf = BufWriter::new(&mut stream);
                rep.encode(&mut buf).unwrap();
                buf.flush().unwrap();
                println!("INFO:       write done");
                return;
              }
              Some(front_tx) => front_tx
            };
            match front_tx.lock().unwrap().send((get_time_coarse(), req, back_tx)) {
              Ok(_) => {}
              _ => {
                println!("INFO:       backend: send error");
                let rep = HttpResponse::not_found();
                let rep = rep.to_raw();
                let mut buf = BufWriter::new(&mut stream);
                rep.encode(&mut buf).unwrap();
                buf.flush().unwrap();
                println!("INFO:       write done");
                return;
              }
            }
            drop(front_tx);
            match front_rx.recv_timeout(StdDuration::from_secs(2)) {
              Err(_) => {
                println!("INFO:       backend: recv error");
                let rep = HttpResponse::not_found();
                let rep = rep.to_raw();
                let mut buf = BufWriter::new(&mut stream);
                rep.encode(&mut buf).unwrap();
                buf.flush().unwrap();
                println!("INFO:       write done");
              }
              Ok(None) => {
                println!("INFO:       no match");
                let rep = HttpResponse::not_found();
                let rep = rep.to_raw();
                let mut buf = BufWriter::new(&mut stream);
                rep.encode(&mut buf).unwrap();
                buf.flush().unwrap();
                println!("INFO:       write done");
              }
              Ok(Some(rep)) => {
                println!("INFO:       matched response");
                let mut rep = rep.to_raw();
                rep.push_header(http1::HeaderName::StrictTransportSecurity, "max-age=63072000");
                rep.push_header(http1::HeaderName::ContentSecurityPolicy, "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; form-action 'self'; img-src 'self'; frame-ancestors 'self'; base-uri 'none'");
                rep.push_header(http1::HeaderName::XContentTypeOptions, "nosniff");
                rep.push_header(http1::HeaderName::XFrameOptions, "SAMEORIGIN");
                let mut buf = BufWriter::new(&mut stream);
                rep.encode(&mut buf).unwrap();
                buf.flush().unwrap();
                println!("INFO:       write done");
              }
            }
          }
        }
      });
    }
  }
}
