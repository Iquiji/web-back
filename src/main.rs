#![warn(clippy::all)]

use futures::future;
use futures::future::Either;
use futures::stream::Stream;
use hyper::rt::Future;
use hyper::service::{service_fn, make_service_fn};
use hyper::{Body, Request, Response, Server};
use hyper::server::conn::AddrStream;
use hyper::{Method, StatusCode};
use std::fs::OpenOptions;
use std::io::prelude::*;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::{Arc, Mutex};
use std::fs::File;
use std::time::{Duration, SystemTime};
use serde_json::Result;
use serde::{Deserialize, Serialize};
use chrono::prelude::*;
static COUNT: AtomicUsize = AtomicUsize::new(0);

// Können wir später gegen was anderes austauschen
type ChatMessageType = String;

#[derive(Serialize,Deserialize)]
struct ChatMessage {
    username: String,
    msg: String,
    timestamp:  String,
}

fn next_count(mutex_file:&Arc<Mutex<File>>) -> usize {
    let wegwerf_count = COUNT.fetch_add(1, Relaxed);
    let mut local = mutex_file.lock().unwrap();
    local.seek(std::io::SeekFrom::Start(0)).unwrap();
    local
        .write_all(wegwerf_count.to_string().as_bytes())
        .unwrap();
    println!("Besucher: {:?} wurde beliefert", &wegwerf_count);
    wegwerf_count
}
fn get_time() -> String {
    let utc: DateTime<Utc> = Utc::now();
    return utc.format("%Y-%m-%d %H:%M:%S").to_string()     
}

fn main() {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open("foo.txt")
        .expect("Es ist ein fehler aufgetreten");
    let mut data = String::new();
    file.read_to_string(&mut data).unwrap();
    COUNT.fetch_add(data.parse().unwrap_or_default(), Relaxed);
    let mutex_file = Arc::new(Mutex::new(file));
    let port: u16 = std::env::args()
        .nth(1)
        .expect("expected port number as first argument")
        .parse()
        .expect("valid u16 port number");
    let addr = ([0, 0, 0, 0], port).into();
    let chat_vec: Vec<ChatMessage> = vec!(ChatMessage{
            username: "tester".to_string(),
            msg:"Meow!".to_string(),
            timestamp: get_time(),
        },ChatMessage{
            username: "tester2".to_string(),
            msg: "Meow?".to_string(),
            timestamp: get_time(),
        });
    let chat = Arc::new(Mutex::new(chat_vec));
    let new_svc = make_service_fn(move |addr_stream: &AddrStream| {
        let mutex_file = mutex_file.clone();
        let chat = chat.clone();
        let remote_addr = addr_stream.remote_addr();

        service_fn(move |req: Request<Body>| {
            let mut response = Response::new(Body::empty());

            let user_agent = req.headers().get("User-Agent");

            eprintln!(
                "{:?} {:?} {:?} {:?} ",
                req.method(),
                remote_addr,
                req.uri().path(),
                user_agent
            );

            match (req.method(), req.uri().path()) {
                (&Method::GET, "/") => {
                    let wegwerf_count = next_count(&mutex_file);
                    let body = format!(
                        "sie sind der {:010}te idiot der diese Website besucht!",
                        wegwerf_count
                    );
                    *response.body_mut() = Body::from(body);
                },
                (&Method::GET, "/counter.js") => {
                *response.body_mut() = Body::from(format!("function pageViewCount() {{ return {} }};",next_count(&mutex_file)));
                println!("counter.js beliefert");
                },
                (&Method::GET, "/chat") => {
                    let headers = response.headers_mut();
                    headers.append(hyper::header::ACCESS_CONTROL_ALLOW_ORIGIN , hyper::header::HeaderValue::from_str("*").unwrap());
                    headers.append(hyper::header::CONTENT_TYPE,hyper::header::HeaderValue::from_str("application/json;encoding=utf-8").unwrap());
                    let local = chat.lock().unwrap();
                    *response.body_mut() = Body::from(serde_json::to_string(&*local).unwrap());
                    println!("someone catched da chat")
                },
                (&Method::POST, "/chat") => {
                    let headers = response.headers_mut();
                    headers.append(hyper::header::ACCESS_CONTROL_ALLOW_ORIGIN , hyper::header::HeaderValue::from_str("*").unwrap());
                    let sheet = Vec::<u8>::new();
                    let chat = chat.clone();
                    return
                        req.into_body()
                            .fold(sheet,move|mut sheet,chunk| {
                                sheet.extend_from_slice(chunk.as_ref());
                                future::result(Ok::<_, hyper::Error>(sheet))
                            })
                            .map(move|sheet|{ 
                                let mut message: ChatMessage = serde_json::from_slice(&sheet).expect("War wohl kein JSON X-(");
                                let mut local = chat.lock().unwrap();
                                message.timestamp = get_time();
                                local.push(message);
                                response}).boxed();
                },
                (&Method::POST, _) | (&Method::PUT, _) => {
                    *response.status_mut() = StatusCode::METHOD_NOT_ALLOWED;
                    return 
                        req.into_body()
                            .for_each(|chunk| {
                                println!("Got chunk {:?}", chunk);
                                future::ok(())
                            })
                           .map(|()| response).boxed();
                }
                _ => {
                    *response.status_mut() = StatusCode::NOT_FOUND;
                }
            };

            future::result(Ok::<_, hyper::Error>(response)).boxed()
        })
    });

    let server = Server::bind(&addr)
        .serve(new_svc)
        .map_err(|e| eprintln!("server error: {}", e));

    hyper::rt::run(server);
}
