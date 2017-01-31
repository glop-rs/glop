extern crate futures;
extern crate futures_cpupool;
extern crate serde_json;
extern crate tokio_core;
extern crate tokio_service;

use std;
use std::collections::HashMap;
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::error::Error as StdError;

use self::futures::{Future, Stream, Sink};
use self::futures::sync::mpsc;
use self::tokio_core::io::Io;
use self::tokio_service::Service as TokioService;

use super::error::Error;
use super::grammar;
use super::runtime;
use super::value::Obj;

#[derive(Serialize, Deserialize)]
pub enum Request {
    Add { source: String, name: String },
    Remove { name: String },
    // Start { name: String },
    // Stop { name: String },
    List,
    SendTo(Envelope), /* RecvFrom { handle: String },
                       * Introduce { names: Vec<String> }, */
}

#[derive(Serialize, Deserialize)]
pub enum Response {
    Add,
    Remove,
    // Start,
    // Stop,
    List { names: Vec<String> },
    SendTo, /* RecvFrom { topic: String, contents: Obj },
             * Introduce, */
}

pub struct Codec;

impl tokio_core::io::Codec for Codec {
    type In = Request;
    type Out = Response;

    fn decode(&mut self, buf: &mut tokio_core::io::EasyBuf) -> std::io::Result<Option<Self::In>> {
        if let Some(i) = buf.as_slice().iter().position(|&b| b == b'\n') {
            // remove the serialized frame from the buffer.
            let line = buf.drain_to(i);

            // Also remove the '\n'
            buf.drain_to(1);

            // Turn this data into a UTF string and
            // return it in a Frame.
            let maybe_req: Result<Self::In, serde_json::error::Error> =
                serde_json::from_slice(line.as_slice());
            match maybe_req {
                Ok(req) => Ok(Some(req)),
                Err(e) => Err(std::io::Error::new(std::io::ErrorKind::Other, e.description())),
            }
        } else {
            Ok(None)
        }
    }

    fn encode(&mut self, msg: Self::Out, buf: &mut Vec<u8>) -> std::io::Result<()> {
        match serde_json::to_writer(buf, &msg) {
            Ok(_) => Ok(()),
            Err(e) => Err(std::io::Error::new(std::io::ErrorKind::Other, e.description())),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Envelope {
    // src: String,
    dst: String,
    topic: String,
    contents: Obj,
}

pub struct Agent {
    matches: Vec<runtime::Match>,
    st: runtime::State,
    receiver: mpsc::Receiver<Envelope>,
    match_index: usize,
}

impl Agent {
    pub fn new_from_file(path: &str, receiver: mpsc::Receiver<Envelope>) -> Result<Agent, Error> {
        let glop_contents = read_file(path)?;
        let glop = grammar::glop(&glop_contents).map_err(Error::Parse)?;
        let mut st = runtime::State::new();
        st.push_msg("init", Obj::new());
        let m_excs = glop.matches
            .iter()
            .map(|m_ast| runtime::Match::new_from_ast(&m_ast))
            .collect::<Vec<_>>();
        Ok(Agent {
            matches: m_excs,
            st: st,
            receiver: receiver,
            match_index: 0,
        })
    }
}

impl Agent {
    fn poll_matches(&mut self) -> futures::Poll<Option<()>, runtime::Error> {
        let i = self.match_index % self.matches.len();
        let m = &self.matches[i];
        let actions = match self.st.eval(m) {
            Some(ref mut txn) => {
                match txn.apply(m) {
                    Ok(actions) => actions,
                    Err(e) => return Err(e),
                }
            }
            None => return Ok(futures::Async::NotReady),
        };
        // TODO: intelligent selection of next match?
        self.match_index = self.match_index + 1;
        // TODO: graceful agent termination (nothing left to do)?
        match self.st.commit(&actions) {
            Ok(_) => Ok(futures::Async::Ready(Some(()))),
            Err(e) => Err(e),
        }
    }
}

impl futures::stream::Stream for Agent {
    type Item = ();
    type Error = runtime::Error;

    fn poll(&mut self) -> futures::Poll<Option<Self::Item>, Self::Error> {
        // TODO: poll mpsc channel (receiver end) for state changes & apply?
        match self.receiver.poll() {
            Ok(futures::Async::Ready(Some(env))) => self.st.push_msg(&env.topic, env.contents),
            Ok(futures::Async::Ready(None)) => return Ok(futures::Async::Ready(None)),
            Err(_) => return Ok(futures::Async::Ready(None)),
            _ => {}
        }
        self.poll_matches()
    }
}

fn read_file(path: &str) -> Result<String, Error> {
    let mut f = std::fs::File::open(path)?;
    let mut s = String::new();
    f.read_to_string(&mut s).map_err(Error::IO)?;
    Ok(s)
}

#[derive(Clone)]
pub struct Service {
    senders: Arc<Mutex<HashMap<String, mpsc::Sender<Envelope>>>>,
    handle: tokio_core::reactor::Handle,
    pool: futures_cpupool::CpuPool,
}

impl Service {
    pub fn new(h: &tokio_core::reactor::Handle) -> Service {
        Service {
            senders: Arc::new(Mutex::new(HashMap::new())),
            handle: h.clone(),
            pool: futures_cpupool::CpuPool::new_num_cpus(),
        }
    }

    fn do_call(&self, req: Request) -> Result<Response, runtime::Error> {
        let mut senders = self.senders.lock().unwrap();
        let res = match req {
            Request::Add { source: ref add_source, name: ref add_name } => {
                let (sender, receiver) = mpsc::channel(10);
                senders.insert(add_name.clone(), sender);
                let agent =
                    Agent::new_from_file(add_source, receiver).map_err(runtime::Error::Base)?;
                self.handle.spawn(self.pool.spawn(agent.for_each(|_| Ok(())).then(|_| Ok(()))));
                Response::Add
            }
            Request::Remove { ref name } => {
                senders.remove(name);
                Response::Remove
            }
            // Start { name: String },
            // Stop { name: String },
            Request::List => Response::List { names: senders.keys().cloned().collect() },
            Request::SendTo(env) => {
                let sender = match senders.get(&env.dst) {
                    Some(s) => s.clone(),
                    None => return Ok(Response::SendTo), // TODO: handle unmatched dst
                };
                self.handle.spawn(sender.send(env).then(|_| Ok(())));
                Response::SendTo
            }
            // RecvFrom { handle: String },
            // Introduce { names: Vec<String> },
        };
        Ok(res)
    }
}

impl TokioService for Service {
    type Request = Request;
    type Response = Response;

    type Error = std::io::Error;

    type Future = futures::BoxFuture<Self::Response, Self::Error>;

    fn call(&self, req: Self::Request) -> Self::Future {
        match self.do_call(req) {
            Ok(res) => futures::future::ok(res).boxed(),
            Err(err) => {
                futures::future::err(std::io::Error::new(std::io::ErrorKind::Other,
                                                         err.description()))
                    .boxed()
            }
        }
    }
}

pub fn run_server() -> Result<(), std::io::Error> {
    let mut core = tokio_core::reactor::Core::new()?;
    let handle = core.handle();
    let addr = "127.0.0.1:0".parse().unwrap();
    let listener = tokio_core::net::TcpListener::bind(&addr, &handle)?;
    let listen_addr = &listener.local_addr()?;
    println!("{}", listen_addr);
    let connections = listener.incoming();
    let service = Service::new(&handle);
    let server = connections.for_each(move |(socket, _peer_addr)| {
        let (wr, rd) = socket.framed(Codec).split();
        let service = service.clone();
        let responses = rd.and_then(move |req| service.call(req));
        let responder = wr.send_all(responses).then(|_| Ok(()));
        handle.spawn(responder);
        Ok(())
    });
    core.run(server)
}
