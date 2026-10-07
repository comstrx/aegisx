use async_trait::async_trait;
use pingora::prelude::*;
use pingora::server::configuration::ServerConf;

pub struct Bare { upstream: String }

#[async_trait]
impl ProxyHttp for Bare {
    type CTX = ();
    fn new_ctx(&self) -> () {}
    async fn upstream_peer(&self, _session: &mut Session, _ctx: &mut ()) -> Result<Box<HttpPeer>> {
        Ok(Box::new(HttpPeer::new(self.upstream.as_str(), false, String::new())))
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let listen = args.get(1).cloned().unwrap_or_else(|| "127.0.0.1:8081".into());
    let upstream = args.get(2).cloned().unwrap_or_else(|| "127.0.0.1:3000".into());
    let threads: usize = args.get(3).and_then(|v| v.parse().ok()).unwrap_or(8);
    let steal = args.get(4).map(|v| v == "steal").unwrap_or(true);
    let conf = ServerConf { threads, work_stealing: steal, upstream_keepalive_pool_size: 256, ..ServerConf::default() };
    let mut server = Server::new_with_opt_and_conf(None, conf);
    server.bootstrap();
    let mut proxy = http_proxy_service(&server.configuration, Bare { upstream });
    proxy.add_tcp(&listen);
    server.add_service(proxy);
    server.run_forever();
}
