mod support;

use std::thread;
use std::time::Duration;

use aegisx::config::Route;
use support::{Http1, Origin, proxy};

#[test]
fn concurrency_limits_cap_in_flight_requests_per_actor () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {
        config.routes.push(Route { name: "slow".to_string(), path: "/slow".to_string(), upstream: "default".to_string(), concurrency_limit: Some(2), ..Route::default() });
        config.routes.push(Route { name: "rest".to_string(), path: "/".to_string(), upstream: "default".to_string(), ..Route::default() });
    });
    let addr = running.addr();

    let workers: Vec<_> = (0..4).map(|index| thread::spawn(move || {

        thread::sleep(Duration::from_millis(index * 40));

        Http1::connect(addr).get("/slow/400").status

    })).collect();

    let mut statuses: Vec<u16> = workers.into_iter().map(|worker| worker.join().expect("worker")).collect();

    statuses.sort_unstable();

    assert_eq!(statuses, vec![200, 200, 503, 503]);
    assert_eq!(Http1::connect(addr).get("/slow/10").status, 200);
    assert_eq!(Http1::connect(addr).get("/x").status, 200);

    running.stop().expect("stop");

}

#[test]
fn global_concurrency_limit_applies_to_every_route () {

    let origin = Origin::start();
    let running = proxy(origin.addr, |config| {
        config.limits.concurrency_limit = 1;
        config.routes.push(Route { name: "all".to_string(), path: "/".to_string(), upstream: "default".to_string(), ..Route::default() });
    });
    let addr = running.addr();

    let first = thread::spawn(move || Http1::connect(addr).get("/slow/400").status);

    thread::sleep(Duration::from_millis(80));

    assert_eq!(Http1::connect(addr).get("/quick").status, 503);
    assert_eq!(first.join().expect("first"), 200);
    assert_eq!(Http1::connect(addr).get("/quick").status, 200);

    running.stop().expect("stop");

}
