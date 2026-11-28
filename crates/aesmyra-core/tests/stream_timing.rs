//! Zeitmessung: kommen Textstücke einzeln an oder erst am Ende?
use aesmyra_core::api::{Api, UiEvent};
use std::io::Write;
use std::time::{Duration, Instant};

#[tokio::test(flavor = "multi_thread")]
async fn stuecke_kommen_sofort() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    std::thread::spawn(move || {
        let (mut sock, _) = listener.accept().unwrap();
        let mut buf = [0u8; 8192];
        let _ = std::io::Read::read(&mut sock, &mut buf);
        let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n";
        sock.write_all(head.as_bytes()).unwrap();
        sock.flush().unwrap();
        for i in 0..6 {
            if i == 0 {
                let f = "event: run.started\ndata: {\"run_id\":\"r9\"}\n\n";
                sock.write_all(f.as_bytes()).unwrap();
            } else {
                let f = format!("event: assistant.delta\ndata: {{\"delta\":\"stueck{i}\\n\"}}\n\n");
                sock.write_all(f.as_bytes()).unwrap();
            }
            sock.flush().unwrap();
            std::thread::sleep(Duration::from_millis(800));
        }
        let f = "event: done\ndata: {}\n\n";
        sock.write_all(f.as_bytes()).unwrap();
        sock.flush().unwrap();
        std::thread::sleep(Duration::from_millis(200));
    });

    let api = Api::new(format!("http://{addr}"), "test").unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<UiEvent>(64);
    let t0 = Instant::now();
    tokio::spawn(async move {
        let _ = api.chat_stream("s1", "hi", &tx).await;
        drop(tx);
    });

    let (mut first_delta, mut deltas) = (None::<f64>, Vec::<f64>::new());
    while let Some(ev) = rx.recv().await {
        if let UiEvent::Delta { .. } = ev {
            let t = t0.elapsed().as_secs_f64();
            deltas.push(t);
            if first_delta.is_none() {
                first_delta = Some(t);
            }
        }
    }
    println!("erste Delta nach {first_delta:?}s, alle: {deltas:?}");
    assert!(deltas.len() >= 3, "nur {} Deltas", deltas.len());
    assert!(
        first_delta.unwrap() < 2.0,
        "erste Delta kam erst nach {:?}s — Stücke werden zurückgehalten",
        first_delta
    );
}
