//use crate::{GatewayBackendHandle, Worker};
use crate::{Context};

use native_tls::{Error as TlsError, Identity};
use service_base::prelude::*;
use service_base::route::*;
use uacme::{Error as UacmeError, Directory, DirectoryUrl, create_p384_key};
use uacme::persist::{FilePersist};

use std::fs::{File};
use std::io::{Read};
use std::thread::{sleep, spawn};
use std::time::{Duration};

#[derive(Debug)]
pub enum AcmeErr {
  _Top,
  Tls(TlsError),
  Uacme(UacmeError),
}

impl From<TlsError> for AcmeErr {
  fn from(e: TlsError) -> AcmeErr {
    AcmeErr::Tls(e)
  }
}

impl From<UacmeError> for AcmeErr {
  fn from(e: UacmeError) -> AcmeErr {
    AcmeErr::Uacme(e)
  }
}

pub struct Acme {
}

impl Acme {
  pub fn identity<S: AsRef<str>>(domain: S, ctx: Context) -> Result<Identity, AcmeErr> {
    let domain = domain.as_ref();
    let mut crt = Vec::new();
    let mut key = Vec::new();
    // FIXME: determine whether staging or production env.
    //let mut crt_f = File::open(&format!("/var/tmp/acme-staging/{}.crt", domain)).unwrap();
    let mut crt_f = File::open(&format!("/var/tmp/acme/{}.crt", domain)).unwrap();
    crt_f.read_to_end(&mut crt).unwrap();
    drop(crt_f);
    //let mut key_f = File::open(&format!("/var/tmp/acme-staging/{}.key", domain)).unwrap();
    let mut key_f = File::open(&format!("/var/tmp/acme/{}.key", domain)).unwrap();
    key_f.read_to_end(&mut key).unwrap();
    drop(key_f);
    let id = Identity::from_pkcs8(&crt, &key)?;
    Ok(id)
  }

  pub fn fresh_identity<S: AsRef<str>, S_: AsRef<str>>(domain: S, alt_domains: &[S_], ctx: Context) -> Result<(), AcmeErr> {
    let domain = domain.as_ref().to_string();
    let alt_domains: Vec<_> = alt_domains.iter().map(|s| s.as_ref().to_string()).collect();
    spawn(move || {
      let mut acme_nr = 0;
      loop {
        acme_nr += 1;
        let res = AcmeWorker::fresh_identity(&domain, &alt_domains, ctx.clone());
        println!("INFO:   acme attempt {}: result={:?}", acme_nr, res);
        if res.is_ok() {
          break;
        }
        sleep(Duration::from_secs(30));
      }
    });
    Ok(())
  }
}

pub struct AcmeWorker {
  // TODO
}

impl AcmeWorker {
  pub fn fresh_identity<S: AsRef<str>, S_: AsRef<str>>(domain: S, alt_domains: &[S_], ctx: Context) -> Result<(), AcmeErr> {
    let domain = domain.as_ref();
    let alt_domains: Vec<_> = alt_domains.iter().map(|s| s.as_ref()).collect();
    // FIXME: determine whether staging or production env.
    /*let url = DirectoryUrl::LetsEncryptStaging;
    let persist = FilePersist::new("/var/tmp/acme-staging");*/
    let url = DirectoryUrl::LetsEncrypt;
    let persist = FilePersist::new("/var/tmp/acme");
    println!("DEBUG:  acme: file persist... done");
    let dir = Directory::from_url(persist.clone(), url)?;
    println!("DEBUG:  acme: directory from url... done");
    let acct = dir.account(&format!("dns@{}", domain))?;
    println!("DEBUG:  acme: account... done");
    let mut order = acct.new_order(domain, &alt_domains)?;
    println!("DEBUG:  acme: new order... done");
    let mut acme_token = None;
    let csr = loop {
      if let Some(token) = acme_token.take() {
        ctx.router.lock().unwrap()
          .remove(80, GET, (".well-known", "acme-challenge", token));
      }
      if let Some(csr) = order.confirm_validations() {
        break csr;
      }
      println!("DEBUG:  acme: confirm validations returned None... done");
      let auths = order.authorizations()?;
      println!("DEBUG:  acme: authorizations... done");
      if auths.len() <= 0 {
        return Err(AcmeErr::_Top);
      }
      let challenge = auths[0].http_challenge();
      println!("DEBUG:  acme: get challenge... done");
      let token = challenge.http_token().to_string();
      println!("DEBUG:  acme: get token... done");
      let proof = challenge.http_proof().to_string();
      println!("DEBUG:  acme: get proof... done");
      acme_token = token.clone().into();
      ctx.router.lock().unwrap()
        .insert(80, GET, (".well-known", "acme-challenge", token), Box::new(move |_, _, _| {
          ok().with_payload_str_mime(proof.clone(), Mime::TextPlain).into()
        }));
      println!("DEBUG:  acme: challenge validation: waiting...");
      challenge.validate(10_000)?;
      println!("DEBUG:  acme: challenge validation: done");
      order.refresh()?;
      println!("DEBUG:  acme: refresh... done");
    };
    if let Some(token) = acme_token.take() {
      ctx.router.lock().unwrap()
        .remove(80, GET, (".well-known", "acme-challenge", token));
    }
    let secret_key = create_p384_key();
    println!("DEBUG:  acme: create key... done");
    let cert_order = csr.finalize_pkey(secret_key, 10_000)?;
    println!("DEBUG:  acme: finalize key... done");
    let cert = cert_order.download_and_save_cert()?;
    println!("DEBUG:  acme: download and save cert... done");
    persist.fresh_symlinks(domain)?;
    println!("DEBUG:  acme: fresh symlinks... done");
    println!("DEBUG:  acme: done");
    Ok(())
  }
}
